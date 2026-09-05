use crate::internship;
use crate::models::{
    AuthGuide, ChannelHealth, ChannelResults, SearchCandidate, SearchCapabilities, SearchChannel,
    SearchSetupResult,
};
use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::task::JoinSet;

const MAX_DISCOVERY_RESULTS: usize = 20;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const SETUP_TIMEOUT: Duration = Duration::from_secs(300);
const AUTH_CHECK_TIMEOUT: Duration = Duration::from_secs(45);
const AUTH_LOGIN_TIMEOUT: Duration = Duration::from_secs(330);
const OPENCLI_BRIDGE_URL: &str = "https://chromewebstore.google.com/detail/opencli/ildkmabpimmkaediidaifkhjpohdnifk";
const NPM_TOOL_DIR: &str = "tools/npm";

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub location: String,
    pub work_mode: String,
    pub rss_feeds: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RawChannelResults {
    pub channel: SearchChannel,
    pub backend: String,
    pub checked_at: String,
    pub value: Value,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSearchManifest {
    pub schema_version: u8,
    pub generated_at: String,
    pub query: String,
    pub health: Vec<ChannelHealth>,
    pub results: Vec<ChannelResults>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct SearchSetupPlan {
    channels: Vec<SearchChannel>,
    manual_steps: Vec<String>,
}

#[async_trait]
pub trait SearchChannelAdapter: Send + Sync {
    fn channel(&self) -> SearchChannel;
    fn dependencies(&self) -> ChannelHealth;
    async fn doctor(&self) -> ChannelHealth { self.dependencies() }
    async fn search(&self, request: &SearchRequest) -> Result<RawChannelResults>;
    fn normalize(&self, raw: RawChannelResults) -> Result<ChannelResults>;
}

pub async fn capabilities(paths: &AppPaths) -> Result<SearchCapabilities> {
    let profile = internship::load(paths)?;
    let request = request_from_profile(&profile, None);
    let mut pending = JoinSet::new();
    for (index, adapter) in make_adapters(paths, &request).into_iter().enumerate() {
        pending.spawn(async move { (index, adapter.doctor().await) });
    }
    let mut channels = Vec::new();
    while let Some(result) = pending.join_next().await {
        channels.push(result.context("渠道检查任务异常结束")?);
    }
    channels.sort_by_key(|(index, _)| *index);
    let channels = channels.into_iter().map(|(_, health)| health).collect();
    Ok(SearchCapabilities {
        checked_at: Utc::now().to_rfc3339(),
        channels,
        warnings: Vec::new(),
    })
}

pub async fn run(paths: &AppPaths, payload: &Value) -> Result<ChannelSearchManifest> {
    let profile = internship::load(paths)?;
    let request = request_from_profile(&profile, Some(payload));
    let adapters = make_adapters(paths, &request);
    let mut health = Vec::new();
    let mut results = Vec::new();
    let mut warnings = Vec::new();
    let mut pending = JoinSet::new();

    for adapter in adapters {
        let request = request.clone();
        pending.spawn(async move {
            let channel = adapter.channel();
            let channel_health = adapter.doctor().await;
            let normalized = if channel_health.available && channel_health.authenticated {
                adapter.search(&request).await.and_then(|raw| adapter.normalize(raw))
            } else {
                Ok(ChannelResults {
                    channel,
                    backend: channel_health.backend.clone(),
                    checked_at: channel_health.checked_at.clone(),
                    candidates: Vec::new(),
                    warnings: vec![channel_health.message.clone()],
                })
            };
            (channel_health, normalized)
        });
    }

    while let Some(joined) = pending.join_next().await {
        match joined {
            Ok((channel_health, Ok(result))) => {
                for warning in &result.warnings {
                    warnings.push(format!("{}：{}", channel_health.channel.display_name(), warning));
                }
                health.push(channel_health);
                results.push(result);
            }
            Ok((channel_health, Err(_))) => {
                let channel = channel_health.channel.clone();
                health.push(channel_health);
                let message = "渠道暂时不可用，已跳过本渠道并继续其他搜索".to_owned();
                warnings.push(format!("{}：{}", channel.display_name(), message));
                results.push(ChannelResults {
                    channel: channel.clone(),
                    backend: backend_for_channel(&health, &channel),
                    checked_at: Utc::now().to_rfc3339(),
                    candidates: Vec::new(),
                    warnings: vec![message],
                });
            }
            Err(_) => {
                warnings.push("一个搜索渠道任务异常结束，其他渠道结果仍可用".into());
            }
        }
    }

    results.sort_by(|left, right| left.channel.as_str().cmp(right.channel.as_str()));
    dedupe_results(&mut results);
    health.sort_by(|left, right| left.channel.as_str().cmp(right.channel.as_str()));
    Ok(ChannelSearchManifest {
        schema_version: 1,
        generated_at: Utc::now().to_rfc3339(),
        query: request.query,
        health,
        results,
        warnings,
    })
}

fn setup_plan(
    paths: &AppPaths,
    requested: Option<Vec<SearchChannel>>,
) -> Result<SearchSetupPlan> {
    let profile = internship::load(paths)?;
    let request = request_from_profile(&profile, None);
    let health = make_adapters(paths, &request)
        .into_iter()
        .map(|adapter| adapter.dependencies())
        .collect::<Vec<_>>();
    let channels = requested
        .map(unique_channels)
        .unwrap_or_else(|| {
            health
                .iter()
                .filter(|item| !item.available)
                .map(|item| item.channel.clone())
                .collect()
        });
    let mut manual_steps = Vec::new();
    if channels.contains(&SearchChannel::Facebook) {
        manual_steps.push("首次使用 Facebook 前，在自己的 Chrome 中安装 OpenCLI 扩展并保持已登录；CareerOS 不代填密码。".into());
    }
    if channels.contains(&SearchChannel::LinkedIn) {
        manual_steps.push("首次使用 LinkedIn 时打开登录引导，在自己的浏览器完成登录并允许上游 MCP 复用会话。".into());
    }
    if channels.contains(&SearchChannel::Twitter) {
        manual_steps.push("首次使用 Twitter / X 时优先在自己的 Chrome 中登录并保持 OpenCLI 可用；若使用 twitter-cli 备选后端，则按上游工具自行配置本地认证。CareerOS 不读取或保存凭据。".into());
    }
    if channels.contains(&SearchChannel::Rss) {
        manual_steps.push("RSS 不需要登录；请在 Internship 画像中填写每行一个 RSS/Atom 地址。".into());
    }
    Ok(SearchSetupPlan {
        channels,
        manual_steps,
    })
}

pub async fn setup(
    paths: &AppPaths,
    requested: Option<Vec<SearchChannel>>,
) -> Result<SearchSetupResult> {
    let plan = setup_plan(paths, requested).map_err(|error| anyhow::anyhow!(error))?;
    let mut messages = Vec::new();
    let mut completed = true;
    let tool_dir = paths.data_root.join(NPM_TOOL_DIR);
    let tool_dir_display = tool_dir.display().to_string();
    let selected = &plan.channels;

    if selected.iter().any(|channel| matches!(channel, SearchChannel::Facebook | SearchChannel::Twitter))
        && find_executable(paths, "opencli").is_none()
    {
        if let Some(npm) = find_executable(paths, "npm") {
            if run_user_command(
                paths,
                &npm,
                &["install", "--global", "--prefix", &tool_dir_display, "@jackwener/opencli"],
                SETUP_TIMEOUT,
            )
            .await
            {
                messages.push("OpenCLI 已安装到 CareerOS 用户工具目录。".into());
            } else {
                completed = false;
                messages.push("OpenCLI 安装失败，请检查 Node.js 版本和网络后重试。".into());
            }
        } else {
            completed = false;
            messages.push("未找到 npm；请先安装 Node.js 20.18.1 或更高版本。".into());
        }
    }

    if selected.iter().any(|channel| matches!(channel, SearchChannel::Exa | SearchChannel::LinkedIn))
        && find_executable(paths, "mcporter").is_none()
    {
        if let Some(npm) = find_executable(paths, "npm") {
            if run_user_command(
                paths,
                &npm,
                &["install", "--global", "--prefix", &tool_dir_display, "mcporter"],
                SETUP_TIMEOUT,
            )
            .await
            {
                messages.push("mcporter 已安装到 CareerOS 用户工具目录。".into());
            } else {
                completed = false;
                messages.push("mcporter 安装失败，请检查 Node.js 24+ 和网络后重试。".into());
            }
        } else {
            completed = false;
            messages.push("未找到 npm；无法安装 mcporter（当前版本需要 Node.js 24+）。".into());
        }
    }

    let needs_uv = selected.iter().any(|channel| {
        matches!(channel, SearchChannel::LinkedIn)
            || (matches!(channel, SearchChannel::Twitter)
                && find_executable(paths, "opencli").is_none()
                && find_twitter_cli(paths).is_none())
    });
    if needs_uv && find_executable(paths, "uv").is_none() {
        let python = find_executable(paths, "python3").or_else(|| find_executable(paths, "python"));
        let installed = if let Some(python) = python {
            let args = if python.file_name().and_then(|value| value.to_str()) == Some("python") {
                vec!["-m", "pip", "install", "--user", "uv"]
            } else {
                vec!["-m", "pip", "install", "--user", "uv"]
            };
            run_user_command(paths, &python, &args, SETUP_TIMEOUT).await
        } else {
            false
        };
        if installed {
            messages.push("uv 已安装到用户环境。".into());
        } else {
            completed = false;
            messages.push("未能安装 uv；请先准备 Python 3 和 pip。".into());
        }
    }

    if selected.contains(&SearchChannel::Twitter)
        && find_executable(paths, "opencli").is_none()
        && find_twitter_cli(paths).is_none()
    {
        if let Some(uv) = find_executable(paths, "uv") {
            if run_user_command(paths, &uv, &["tool", "install", "twitter-cli"], SETUP_TIMEOUT).await {
                messages.push("twitter-cli 已安装到用户环境。".into());
            } else {
                completed = false;
                messages.push("twitter-cli 安装失败，请稍后重试。".into());
            }
        } else {
            completed = false;
            messages.push("未找到 uv，无法安装 twitter-cli。".into());
        }
    }

    if selected.contains(&SearchChannel::Exa) {
        if let Some(mcporter) = find_executable(paths, "mcporter") {
            if run_user_command(
                paths,
                &mcporter,
                &["config", "add", "exa", "https://mcp.exa.ai/mcp", "--scope", "home"],
                SETUP_TIMEOUT,
            )
            .await
            {
                messages.push("Exa 已写入用户级 mcporter 配置。".into());
            } else {
                completed = false;
                messages.push("Exa 配置失败；请在上游 mcporter 中完成配置。".into());
            }
        } else {
            completed = false;
            messages.push("未找到 mcporter，跳过 Exa 配置。".into());
        }
    }
    if selected.contains(&SearchChannel::LinkedIn) {
        if let Some(mcporter) = find_executable(paths, "mcporter") {
            if run_user_command(
                paths,
                &mcporter,
                &[
                    "config",
                    "add",
                    "linkedin",
                    "--command",
                    "uvx",
                    "--scope",
                    "home",
                    "--",
                    "mcp-server-linkedin@latest",
                ],
                SETUP_TIMEOUT,
            )
            .await
            {
                messages.push("LinkedIn MCP 已写入用户级 mcporter 配置；登录仍由用户在浏览器完成。".into());
            } else {
                completed = false;
                messages.push("LinkedIn MCP 配置失败；请在上游 mcporter 中完成配置。".into());
            }
        } else {
            completed = false;
            messages.push("未找到 mcporter，跳过 LinkedIn MCP 配置。".into());
        }
    }
    messages.extend(plan.manual_steps);
    Ok(SearchSetupResult {
        completed,
        messages,
        capabilities: capabilities(paths).await?,
    })
}

pub async fn begin_auth(paths: &AppPaths, channel: SearchChannel) -> Result<AuthGuide> {
    let mut guide = auth_guide(channel.clone())?;
    if channel == SearchChannel::LinkedIn {
        let uvx = find_executable(paths, "uvx").context("请先一键启用 LinkedIn MCP")?;
        let output = auth_output(
            paths, &uvx, &["mcp-server-linkedin@latest", "--login"], AUTH_LOGIN_TIMEOUT,
        ).await.map_err(|probe| anyhow::anyhow!(probe.message()))?;
        if !output.status.success() {
            bail!("LinkedIn 登录未完成；请关闭遗留的 MCP 登录窗口后重试，并检查网络或浏览器下载是否受阻。");
        }
    } else if let Some(opencli) = find_executable(paths, "opencli") {
        // Let OpenCLI open the browser/profile used by its search adapter.
        // Never treat opening an arbitrary default-browser URL as authentication.
        let output = auth_output(
            paths, &opencli, &[channel.as_str(), "login", "--timeout", "300", "-f", "json"],
            AUTH_LOGIN_TIMEOUT,
        ).await.map_err(|probe| anyhow::anyhow!(probe.message()))?;
        match output.status.code() {
            Some(0) => {},
            Some(69) => {
                guide.url = Some(OPENCLI_BRIDGE_URL.into());
                guide.instructions.insert(0, "未连接到 OpenCLI Browser Bridge。请在用于搜索的 Chrome 中安装并启用此扩展，保持 Chrome 打开；多个 Profile 时请在 OpenCLI 中选定搜索用的 Profile，然后重试连接。".into());
            },
            Some(75) => bail!("等待浏览器认证超时；请确认登录和安全验证已完成，然后重试连接。"),
            Some(77) => bail!("浏览器尚未完成登录或安全验证，请重试连接。"),
            _ => bail!("OpenCLI 登录无法完成；请检查工具版本、Chrome 和 Browser Bridge 扩展后重试。"),
        }
    } else if channel == SearchChannel::Twitter && find_twitter_cli(paths).is_some() {
        guide.url = Some("https://x.com/i/flow/login".into());
        guide.instructions.insert(0, "twitter-cli 将自行验证本地认证；请在它支持的浏览器中完成登录并允许上游工具访问会话。".into());
    } else {
        bail!("请先一键启用此搜索渠道");
    }
    Ok(guide)
}

fn auth_guide(channel: SearchChannel) -> Result<AuthGuide> {
    let guide = match channel {
        SearchChannel::Facebook => AuthGuide {
            channel,
            title: "准备 Facebook 登录态".into(),
            url: None,
            instructions: vec![
                "在自己的 Chrome 中打开 Facebook 并完成登录。".into(),
                "保持 OpenCLI 浏览器桥接可用；CareerOS 只调用搜索命令，不读取密码、Cookie 或 Token。".into(),
            ],
        },
        SearchChannel::LinkedIn => AuthGuide {
            channel,
            title: "准备 LinkedIn 登录态".into(),
            url: None,
            instructions: vec![
                "请在 LinkedIn MCP 打开的独立浏览器中完成登录；普通浏览器的登录不会自动成为 MCP 会话。".into(),
                "CareerOS 不代替登录，也不读取或保存账号凭据。".into(),
            ],
        },
        SearchChannel::Twitter => AuthGuide {
            channel,
            title: "准备 Twitter / X 登录态".into(),
            url: None,
            instructions: vec![
                "优先在自己的 Chrome 中登录 Twitter / X 并保持浏览器会话。".into(),
                "也可以自行按 twitter-cli 文档配置本地 Token/Cookie；CareerOS 不读取、打印或保存它们。".into(),
            ],
        },
        _ => bail!("该搜索渠道不需要社交平台登录引导"),
    };
    Ok(guide)
}

fn make_adapters(paths: &AppPaths, request: &SearchRequest) -> Vec<Box<dyn SearchChannelAdapter>> {
    let mcporter = find_executable(paths, "mcporter");
    let opencli = find_executable(paths, "opencli");
    let twitter = find_twitter_cli(paths);
    vec![
        Box::new(WebAtsAdapter),
        Box::new(ExaAdapter { paths: paths.clone(), executable: mcporter.clone() }),
        Box::new(RssAdapter { feeds: request.rss_feeds.clone() }),
        Box::new(LinkedInAdapter { paths: paths.clone(), executable: mcporter, uvx: find_executable(paths, "uvx") }),
        Box::new(FacebookAdapter { paths: paths.clone(), executable: opencli.clone() }),
        Box::new(TwitterAdapter {
            paths: paths.clone(),
            opencli,
            twitter,
        }),
    ]
}

struct WebAtsAdapter;

#[async_trait]
impl SearchChannelAdapter for WebAtsAdapter {
    fn channel(&self) -> SearchChannel { SearchChannel::WebAts }

    fn dependencies(&self) -> ChannelHealth {
        health(
            SearchChannel::WebAts,
            "codex_web_search",
            true,
            true,
            "ready",
            "由当前 Codex 线程执行官方网页和 ATS 搜索。",
        )
    }

    async fn search(&self, _request: &SearchRequest) -> Result<RawChannelResults> {
        Ok(raw(
            SearchChannel::WebAts,
            "codex_web_search",
            Value::Null,
            vec!["官方 Web / ATS 将由 Codex 线程直接检索。".into()],
        ))
    }

    fn normalize(&self, raw: RawChannelResults) -> Result<ChannelResults> {
        Ok(ChannelResults {
            channel: raw.channel,
            backend: raw.backend,
            checked_at: raw.checked_at,
            candidates: Vec::new(),
            warnings: raw.warnings,
        })
    }
}

struct ExaAdapter { paths: AppPaths, executable: Option<PathBuf> }

#[async_trait]
impl SearchChannelAdapter for ExaAdapter {
    fn channel(&self) -> SearchChannel { SearchChannel::Exa }

    fn dependencies(&self) -> ChannelHealth {
        let available = self.executable.is_some();
        health(
            SearchChannel::Exa,
            "mcporter:exa.web_search_exa",
            available,
            available,
            if available { "ready" } else { "setup_required" },
            if available {
                "mcporter 可用；若上游返回配置错误，请执行用户级 Exa 配置。"
            } else {
                "缺少 mcporter，可从 Internship 搜索设置执行用户级安装。"
            },
        )
    }

    async fn search(&self, request: &SearchRequest) -> Result<RawChannelResults> {
        let executable = self.executable.as_ref().context("mcporter 不可用")?;
        let params = json!({"query": request.query, "numResults": 10});
        let value = command_json(
            &self.paths,
            executable,
            &[
                "call".into(),
                "exa.web_search_exa".into(),
                "--params".into(),
                params.to_string(),
                "--timeout".into(),
                "120000".into(),
                "--output".into(),
                "json".into(),
            ],
        )
        .await?;
        Ok(raw(SearchChannel::Exa, "mcporter:exa.web_search_exa", value, Vec::new()))
    }

    fn normalize(&self, raw: RawChannelResults) -> Result<ChannelResults> {
        normalize_json(raw)
    }
}

struct RssAdapter { feeds: Vec<String> }

#[async_trait]
impl SearchChannelAdapter for RssAdapter {
    fn channel(&self) -> SearchChannel { SearchChannel::Rss }

    fn dependencies(&self) -> ChannelHealth {
        let available = !self.feeds.is_empty();
        health(
            SearchChannel::Rss,
            "reqwest_rss",
            available,
            true,
            if available { "ready" } else { "setup_required" },
            if available {
                "已配置 RSS/Atom 地址，将直接抓取并解析。"
            } else {
                "尚未配置 RSS；可在 Internship 画像中每行填写一个 RSS/Atom 地址。"
            },
        )
    }

    async fn search(&self, _request: &SearchRequest) -> Result<RawChannelResults> {
        let client = reqwest::Client::builder()
            .timeout(COMMAND_TIMEOUT)
            .user_agent("CareerOS Internship Search")
            .build()?;
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        for feed in &self.feeds {
            match client.get(feed).send().await {
                Ok(response) if response.status().is_success() => match response.text().await {
                    Ok(body) => entries.extend(parse_feed(&body, feed)),
                    Err(_) => warnings.push("一个 RSS 地址的正文读取失败".into()),
                },
                Ok(_) => warnings.push("一个 RSS 地址返回了不可用的 HTTP 状态".into()),
                Err(_) => warnings.push("一个 RSS 地址暂时无法访问".into()),
            }
        }
        if entries.is_empty() && warnings.is_empty() {
            warnings.push("RSS 地址没有返回可识别的 item 或 entry".into());
        }
        Ok(raw(SearchChannel::Rss, "reqwest_rss", Value::Array(entries), warnings))
    }

    fn normalize(&self, raw: RawChannelResults) -> Result<ChannelResults> {
        normalize_json(raw)
    }
}

struct LinkedInAdapter { paths: AppPaths, executable: Option<PathBuf>, uvx: Option<PathBuf> }

#[async_trait]
impl SearchChannelAdapter for LinkedInAdapter {
    fn channel(&self) -> SearchChannel { SearchChannel::LinkedIn }

    async fn doctor(&self) -> ChannelHealth {
        let base = self.dependencies();
        let Some(uvx) = self.uvx.as_ref().filter(|_| base.available) else { return base; };
        // Resolve cached Python packages during refresh, without opening a login window.
        let output = auth_output(&self.paths, uvx,
            &["--offline", "mcp-server-linkedin@latest", "--status"], AUTH_CHECK_TIMEOUT).await;
        auth_health(base, linkedin_auth_probe(output))
    }

    fn dependencies(&self) -> ChannelHealth {
        let available = self.executable.is_some() && self.uvx.is_some();
        health(
            SearchChannel::LinkedIn,
            "mcporter:linkedin.search_jobs",
            available,
            false,
            if available { "login_required" } else { "setup_required" },
            if available {
                "工具可用；首次使用需在浏览器完成 LinkedIn 登录。"
            } else {
                "缺少 mcporter，可执行用户级安装和 LinkedIn MCP 配置。"
            },
        )
    }

    async fn search(&self, request: &SearchRequest) -> Result<RawChannelResults> {
        let executable = self.executable.as_ref().context("mcporter 不可用")?;
        let mut params = Map::new();
        params.insert("keywords".into(), Value::String(request.query.clone()));
        params.insert("max_pages".into(), Value::Number(2.into()));
        params.insert("job_type".into(), Value::String("internship".into()));
        params.insert("experience_level".into(), Value::String("internship".into()));
        if !request.location.trim().is_empty() {
            params.insert("location".into(), Value::String(request.location.clone()));
        }
        if !request.work_mode.trim().is_empty() {
            params.insert("work_type".into(), Value::String(request.work_mode.clone()));
        }
        let value = command_json(
            &self.paths,
            executable,
            &[
                "call".into(),
                "linkedin.search_jobs".into(),
                "--params".into(),
                Value::Object(params).to_string(),
                "--timeout".into(),
                "120000".into(),
                "--output".into(),
                "json".into(),
            ],
        )
        .await?;
        Ok(raw(
            SearchChannel::LinkedIn,
            "mcporter:linkedin.search_jobs",
            value,
            Vec::new(),
        ))
    }

    fn normalize(&self, raw: RawChannelResults) -> Result<ChannelResults> {
        normalize_json(raw)
    }
}

struct FacebookAdapter { paths: AppPaths, executable: Option<PathBuf> }

#[async_trait]
impl SearchChannelAdapter for FacebookAdapter {
    fn channel(&self) -> SearchChannel { SearchChannel::Facebook }

    async fn doctor(&self) -> ChannelHealth {
        let base = self.dependencies();
        let Some(executable) = self.executable.as_ref() else { return base; };
        auth_health(base, opencli_auth_probe(&self.paths, executable, "facebook").await)
    }

    fn dependencies(&self) -> ChannelHealth {
        let available = self.executable.is_some();
        health(
            SearchChannel::Facebook,
            "opencli:facebook",
            available,
            false,
            if available { "login_required" } else { "setup_required" },
            if available {
                "OpenCLI 可用；请在自己的 Chrome 中保持 Facebook 登录态。"
            } else {
                "缺少 OpenCLI，可执行用户级安装；Chrome 扩展和登录仍需用户完成。"
            },
        )
    }

    async fn search(&self, request: &SearchRequest) -> Result<RawChannelResults> {
        let executable = self.executable.as_ref().context("OpenCLI 不可用")?;
        let value = command_json(
            &self.paths,
            executable,
            &[
                "facebook".into(),
                "search".into(),
                request.query.clone(),
                "-f".into(),
                "json".into(),
            ],
        )
        .await?;
        Ok(raw(SearchChannel::Facebook, "opencli:facebook", value, Vec::new()))
    }

    fn normalize(&self, raw: RawChannelResults) -> Result<ChannelResults> {
        normalize_json(raw)
    }
}

struct TwitterAdapter { paths: AppPaths, opencli: Option<PathBuf>, twitter: Option<PathBuf> }

#[async_trait]
impl SearchChannelAdapter for TwitterAdapter {
    fn channel(&self) -> SearchChannel { SearchChannel::Twitter }

    async fn doctor(&self) -> ChannelHealth {
        let mut base = self.dependencies();
        let mut probe = AuthProbe::CheckFailed;
        if let Some(opencli) = &self.opencli {
            probe = opencli_auth_probe(&self.paths, opencli, "twitter").await;
            if probe == AuthProbe::Ready { return auth_health(base, probe); }
        }
        if let Some(twitter) = &self.twitter {
            let fallback = twitter_auth_probe(auth_output(
                &self.paths, twitter, &["status", "--json"], AUTH_CHECK_TIMEOUT,
            ).await);
            if fallback == AuthProbe::Ready || self.opencli.is_none() {
                base.backend = "twitter-cli".into();
                return auth_health(base, fallback);
            }
        }
        if base.available { auth_health(base, probe) } else { base }
    }

    fn dependencies(&self) -> ChannelHealth {
        let backend = select_twitter_backend(self.opencli.is_some(), self.twitter.is_some());
        match backend {
            Some(backend) => health(
                SearchChannel::Twitter,
                backend,
                true,
                false,
                "login_required",
                "搜索工具可用；请复用 Chrome 登录态或按上游工具自行配置本地认证。",
            ),
            None => health(
                SearchChannel::Twitter,
                "opencli:twitter",
                false,
                false,
                "setup_required",
                "缺少 OpenCLI 和 twitter-cli，可执行用户级安装；登录仍需用户完成。",
            ),
        }
    }

    async fn search(&self, request: &SearchRequest) -> Result<RawChannelResults> {
        if let Some(opencli) = self.opencli.as_ref() {
            let value = command_json(
                &self.paths,
                opencli,
                &[
                    "twitter".into(),
                    "search".into(),
                    request.query.clone(),
                    "-f".into(),
                    "json".into(),
                ],
            )
            .await;
            if let Ok(value) = value {
                return Ok(raw(SearchChannel::Twitter, "opencli:twitter", value, Vec::new()));
            }
            if self.twitter.is_none() { bail!("OpenCLI Twitter 搜索失败"); }
        }
        let twitter = self.twitter.as_ref().context("Twitter 搜索工具不可用")?;
        let value = command_json(
            &self.paths,
            twitter,
            &[
                "search".into(),
                request.query.clone(),
                "--json".into(),
                "--max".into(),
                "10".into(),
            ],
        )
        .await?;
        Ok(raw(SearchChannel::Twitter, "twitter-cli", value, Vec::new()))
    }

    fn normalize(&self, raw: RawChannelResults) -> Result<ChannelResults> {
        normalize_json(raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthProbe { Ready, LoginRequired, BridgeRequired, CheckTimedOut, CheckFailed }

impl AuthProbe {
    fn status(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::LoginRequired => "login_required",
            Self::BridgeRequired => "bridge_required",
            Self::CheckTimedOut => "check_timed_out",
            Self::CheckFailed => "check_failed",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Ready => "已由搜索后端确认登录会话，可用于搜索。",
            Self::LoginRequired => "未检测到有效会话；请点击连接渠道，在搜索工具打开的浏览器中完成登录。",
            Self::BridgeRequired => "无法连接 OpenCLI Browser Bridge；请保持 Chrome 和扩展打开，并确认 OpenCLI 选中了正确的浏览器 Profile，再连接渠道。",
            Self::CheckTimedOut => "登录状态检查超时，尚不能确认连接；请检查浏览器和网络后重试。这不代表账号已退出。",
            Self::CheckFailed => "无法验证会话；请检查工具版本、网络或浏览器状态。首次使用 LinkedIn 请点击连接渠道以准备 MCP 浏览器。",
        }
    }
}

fn auth_health(mut base: ChannelHealth, probe: AuthProbe) -> ChannelHealth {
    base.authenticated = probe == AuthProbe::Ready;
    base.status = probe.status().into();
    base.message = probe.message().into();
    base.checked_at = Utc::now().to_rfc3339();
    base
}

// Auth responses stay in memory and are reduced to allow-listed status signals.
// Never surface upstream stdout/stderr (which may contain identity information).
async fn auth_output(
    paths: &AppPaths, program: &Path, args: &[&str], timeout: Duration,
) -> std::result::Result<std::process::Output, AuthProbe> {
    let mut command = channel_command(paths, program);
    // The upstream's default 45-second bridge wait would consume our whole
    // check budget before it can report BROWSER_CONNECT. Keep room for its verdict.
    command.env("OPENCLI_BROWSER_CONNECT_TIMEOUT", "10");
    if args.contains(&"--login") {
        // Give the upstream browser time to close and persist before our outer deadline.
        command.env("LOGIN_TIMEOUT", "300");
    }
    tokio::time::timeout(timeout, command.args(args).stdout(Stdio::piped()).stderr(Stdio::null()).output())
        .await.map_err(|_| AuthProbe::CheckTimedOut)?
        .map_err(|_| AuthProbe::CheckFailed)
}

async fn opencli_auth_probe(paths: &AppPaths, program: &Path, site: &str) -> AuthProbe {
    let output = auth_output(paths, program,
        &["auth", "status", "--site", site, "--full", "--timeout", "20", "-f", "json"],
        AUTH_CHECK_TIMEOUT,
    ).await;
    let output = match output { Ok(output) => output, Err(probe) => return probe };
    if !output.status.success() {
        return match output.status.code() {
            Some(69) => AuthProbe::BridgeRequired,
            Some(77) => AuthProbe::LoginRequired,
            Some(75) => AuthProbe::CheckTimedOut,
            _ => AuthProbe::CheckFailed,
        };
    }
    // 'auth status' can exit 0 with an error row, so exit status alone is insufficient.
    let Ok(value) = parse_json_output(&output.stdout) else { return AuthProbe::CheckFailed; };
    let row = value.as_array().and_then(|rows| rows.iter().find(|row| row["site"] == site));
    let Some(row) = row else { return AuthProbe::CheckFailed; };
    match row["status"].as_str() {
        Some("logged_in") if row["logged_in"] == true => AuthProbe::Ready,
        Some("not_logged_in") => AuthProbe::LoginRequired,
        Some("error") => match row["error"].as_str().unwrap_or("").split(':').next() {
            Some("BROWSER_CONNECT") => AuthProbe::BridgeRequired,
            Some("AUTH_REQUIRED") => AuthProbe::LoginRequired,
            Some("TIMEOUT") => AuthProbe::CheckTimedOut,
            _ => AuthProbe::CheckFailed,
        },
        _ => AuthProbe::CheckFailed,
    }
}

fn linkedin_auth_probe(output: std::result::Result<std::process::Output, AuthProbe>) -> AuthProbe {
    let output = match output { Ok(output) => output, Err(probe) => return probe };
    let text = String::from_utf8_lossy(&output.stdout);
    // --status can exit 0 for an unverified foreign-runtime bridge. Require its
    // explicit validation verdict, not the exit code or the presence of a profile.
    if output.status.success() && text.lines().any(|line| line.starts_with("✅ Session is valid (profile:")) {
        AuthProbe::Ready
    } else if text.contains("No valid source session found") || text.contains("Session expired or invalid") {
        AuthProbe::LoginRequired
    } else {
        AuthProbe::CheckFailed
    }
}

fn twitter_auth_probe(output: std::result::Result<std::process::Output, AuthProbe>) -> AuthProbe {
    let output = match output { Ok(output) => output, Err(probe) => return probe };
    let Ok(value) = parse_json_output(&output.stdout) else { return AuthProbe::CheckFailed; };
    if output.status.success() && value["ok"] == true && value["data"]["authenticated"] == true {
        AuthProbe::Ready
    } else if value["error"]["code"] == "not_authenticated" {
        AuthProbe::LoginRequired
    } else {
        AuthProbe::CheckFailed
    }
}

fn request_from_profile(profile: &internship::InternshipProfile, payload: Option<&Value>) -> SearchRequest {
    let payload_feeds = payload
        .and_then(|value| value.get("rssFeeds"))
        .map(parse_string_list)
        .unwrap_or_default();
    let mut rss_feeds = profile
        .rss_feeds
        .iter()
        .filter(|feed| is_http_url(feed))
        .take(20)
        .cloned()
        .collect::<Vec<_>>();
    for feed in payload_feeds.into_iter().filter(|feed| is_http_url(feed)).take(20) {
        if !rss_feeds.contains(&feed) {
            rss_feeds.push(feed);
        }
        if rss_feeds.len() >= 20 {
            break;
        }
    }
    let query = payload
        .and_then(|value| value.get("query"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            [
                profile.target_roles.as_str(),
                profile.industries.as_str(),
                profile.regions.as_str(),
                profile.work_mode.as_str(),
            ]
            .into_iter()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("; ")
        });
    SearchRequest {
        query: if query.is_empty() { "industry internship".into() } else { query },
        location: profile.regions.clone(),
        work_mode: profile.work_mode.clone(),
        rss_feeds,
    }
}

fn parse_string_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(values) => values.iter().filter_map(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned).collect(),
        Value::String(value) => value.lines().map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

fn raw(channel: SearchChannel, backend: &str, value: Value, warnings: Vec<String>) -> RawChannelResults {
    RawChannelResults {
        channel,
        backend: backend.into(),
        checked_at: Utc::now().to_rfc3339(),
        value,
        warnings,
    }
}

fn health(
    channel: SearchChannel,
    backend: &str,
    available: bool,
    authenticated: bool,
    status: &str,
    message: &str,
) -> ChannelHealth {
    ChannelHealth {
        channel,
        backend: backend.into(),
        available,
        authenticated,
        status: status.into(),
        message: message.into(),
        checked_at: Utc::now().to_rfc3339(),
    }
}

fn backend_for_channel(health: &[ChannelHealth], channel: &SearchChannel) -> String {
    health
        .iter()
        .find(|item| &item.channel == channel)
        .map(|item| item.backend.clone())
        .unwrap_or_else(|| channel.as_str().into())
}

fn unique_channels(channels: Vec<SearchChannel>) -> Vec<SearchChannel> {
    let mut seen = HashSet::new();
    channels.into_iter().filter(|channel| seen.insert(channel.clone())).collect()
}

fn select_twitter_backend(has_opencli: bool, has_twitter_cli: bool) -> Option<&'static str> {
    if has_opencli {
        Some("opencli:twitter")
    } else if has_twitter_cli {
        Some("twitter-cli")
    } else {
        None
    }
}

fn find_twitter_cli(paths: &AppPaths) -> Option<PathBuf> {
    find_executable(paths, "twitter").or_else(|| find_executable(paths, "twitter-cli"))
}

fn dedupe_results(results: &mut [ChannelResults]) {
    let mut seen = HashSet::new();
    let mut remaining = MAX_DISCOVERY_RESULTS;
    let mut order = (0..results.len()).collect::<Vec<_>>();
    order.sort_by_key(|index| channel_priority(&results[*index].channel));
    for index in order {
        let result = &mut results[index];
        result.candidates.retain(|candidate| {
            if remaining == 0 {
                return false;
            }
            let key = candidate_key(candidate);
            if seen.insert(key) {
                remaining -= 1;
                true
            } else {
                false
            }
        });
    }
}

fn channel_priority(channel: &SearchChannel) -> u8 {
    match channel {
        SearchChannel::WebAts => 0,
        SearchChannel::Exa => 1,
        SearchChannel::Rss => 2,
        SearchChannel::LinkedIn => 3,
        SearchChannel::Facebook => 4,
        SearchChannel::Twitter => 5,
    }
}

fn candidate_key(candidate: &SearchCandidate) -> String {
    let url = canonical_url(&candidate.url);
    if !url.is_empty() {
        return format!("url:{url}");
    }
    format!(
        "text:{}:{}",
        normalize_text(candidate.organization.as_deref().unwrap_or("")),
        normalize_text(&candidate.title)
    )
}

fn normalize_json(raw: RawChannelResults) -> Result<ChannelResults> {
    let mut candidates = Vec::new();
    collect_json_candidates(
        &raw.value,
        &raw.channel,
        &raw.backend,
        &raw.checked_at,
        None,
        &mut candidates,
    );
    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate_key(candidate)));
    candidates.truncate(MAX_DISCOVERY_RESULTS);
    Ok(ChannelResults {
        channel: raw.channel,
        backend: raw.backend,
        checked_at: raw.checked_at,
        candidates,
        warnings: raw.warnings,
    })
}

fn collect_json_candidates(
    value: &Value,
    channel: &SearchChannel,
    backend: &str,
    checked_at: &str,
    source_title: Option<&str>,
    candidates: &mut Vec<SearchCandidate>,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_json_candidates(value, channel, backend, checked_at, source_title, candidates);
            }
        }
        Value::Object(object) => {
            let title = first_string(object, &["title", "job_title", "jobTitle", "name"])
                .or_else(|| first_string(object, &["text", "content", "message", "full_text"]).map(first_line));
            let url = first_string(object, &[
                "url", "link", "job_url", "jobUrl", "permalink", "permalink_url", "permalinkUrl",
                "tweet_url", "tweetUrl", "source_url", "sourceUrl", "href",
            ]);
            if let (Some(title), Some(url)) = (title, url) {
                if is_http_url(&url) {
                    candidates.push(SearchCandidate {
                        title: trim_excerpt(&title, 300),
                        organization: first_string(object, &[
                            "organization", "company", "company_name", "companyName", "employer", "author",
                            "author_name", "authorName", "username", "screen_name", "screenName",
                        ]),
                        description: first_string(object, &[
                            "description", "summary", "snippet", "full_text", "message", "text", "content",
                        ]).map(|value| trim_excerpt(&value, 800)),
                        url,
                        published_at: first_string(object, &[
                            "published_at", "publishedAt", "date", "created_at", "createdAt", "timestamp",
                        ]),
                        location: first_string(object, &["location", "locations", "workplace", "place"]),
                        source_title: first_string(object, &["source_title", "sourceTitle", "source", "feedTitle"])
                            .or_else(|| source_title.map(str::to_owned)),
                        evidence_type: "secondary".into(),
                        channel: channel.clone(),
                        backend: backend.into(),
                        checked_at: checked_at.into(),
                    });
                }
            }
            for (key, nested) in object {
                let nested_source = if matches!(key.as_str(), "feedTitle" | "sourceTitle") {
                    nested.as_str().or(source_title)
                } else {
                    source_title
                };
                collect_json_candidates(nested, channel, backend, checked_at, nested_source, candidates);
            }
        }
        _ => {}
    }
}

fn first_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| object.get(*key).and_then(|value| match value {
        Value::String(value) if !value.trim().is_empty() => Some(value.trim().to_owned()),
        Value::Array(values) => values.iter().filter_map(Value::as_str).find(|value| !value.trim().is_empty()).map(str::to_owned),
        _ => None,
    }))
}

fn first_line(value: String) -> String {
    value.lines().find(|line| !line.trim().is_empty()).unwrap_or(&value).trim().to_owned()
}

fn trim_excerpt(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect::<String>().trim().to_owned()
}

fn parse_feed(xml: &str, feed_url: &str) -> Vec<Value> {
    let mut values = Vec::new();
    for pattern in [r"(?is)<item\b[^>]*>(.*?)</item\s*>", r"(?is)<entry\b[^>]*>(.*?)</entry\s*>"] {
        let Ok(regex) = Regex::new(pattern) else { continue };
        for capture in regex.captures_iter(xml) {
            let body = &capture[1];
            let title = xml_tag(body, "title").unwrap_or_else(|| "Untitled internship".into());
            let description = xml_tag(body, "description").or_else(|| xml_tag(body, "summary"));
            let link = xml_link(body).unwrap_or_else(|| feed_url.to_owned());
            values.push(json!({
                "title": title,
                "description": description,
                "url": link,
                "sourceTitle": xml_tag(body, "source").or_else(|| Some(feed_url.to_owned())),
                "publishedAt": xml_tag(body, "pubDate").or_else(|| xml_tag(body, "updated")),
            }));
        }
    }
    values
}

fn xml_tag(body: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"(?is)<{tag}\b[^>]*>(.*?)</{tag}\s*>");
    let regex = Regex::new(&pattern).ok()?;
    regex
        .captures(body)
        .map(|capture| decode_xml(&capture[1]))
        .filter(|value| !value.trim().is_empty())
}

fn xml_link(body: &str) -> Option<String> {
    let link_tag = Regex::new(r"(?is)<link\b([^>]*)>(.*?)</link\s*>").ok()?;
    if let Some(capture) = link_tag.captures(body) {
        let attrs = &capture[1];
        let content = decode_xml(&capture[2]);
        if is_http_url(content.trim()) {
            return Some(content.trim().to_owned());
        }
        let href = Regex::new(r#"(?i)href\s*=\s*[\"']([^\"']+)[\"']"#).ok()?;
        if let Some(value) = href.captures(attrs).and_then(|value| value.get(1)) {
            return Some(decode_xml(value.as_str()));
        }
    }
    let atom_link = Regex::new(r#"(?i)<link\b[^>]*href\s*=\s*[\"']([^\"']+)[\"'][^>]*/?>"#).ok()?;
    atom_link.captures(body).and_then(|capture| Some(decode_xml(&capture[1])))
}

fn decode_xml(value: &str) -> String {
    value
        .replace("<![CDATA[", "")
        .replace("]]>", "")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

async fn command_json(paths: &AppPaths, program: &Path, args: &[String]) -> Result<Value> {
    let output = tokio::time::timeout(
        COMMAND_TIMEOUT,
        channel_command(paths, program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
    )
    .await
    .context("搜索渠道命令超时")?
    .context("搜索渠道命令无法启动")?;
    if !output.status.success() {
        bail!("搜索渠道命令执行失败");
    }
    parse_json_output(&output.stdout)
}

fn parse_json_output(bytes: &[u8]) -> Result<Value> {
    if let Ok(value) = serde_json::from_slice(bytes) {
        return Ok(value);
    }
    let text = String::from_utf8_lossy(bytes);
    for marker in ['{', '['] {
        if let Some(index) = text.find(marker) {
            if let Ok(value) = serde_json::from_str::<Value>(&text[index..]) {
                return Ok(value);
            }
        }
    }
    bail!("搜索渠道返回了不可识别的结果")
}

async fn run_user_command(paths: &AppPaths, program: &Path, args: &[&str], timeout: Duration) -> bool {
    let result = tokio::time::timeout(
        timeout,
        channel_command(paths, program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await;
    matches!(result, Ok(Ok(status)) if status.success())
}

// Finder-launched apps do not inherit the user's interactive shell PATH.
// Apply the same bounded search path to discovery AND child processes.
fn executable_dirs(paths: &AppPaths) -> Vec<PathBuf> {
    let mut directories = vec![
        paths.data_root.join("tools/node/bin"),
        paths.data_root.join(NPM_TOOL_DIR).join("bin"),
        PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        directories.extend(std::env::split_paths(&path));
    }
    if let Some(home) = dirs::home_dir() {
        for relative in [".local/bin", ".cargo/bin", "Library/Python/3.12/bin", "Library/Python/3.13/bin"] {
            directories.push(home.join(relative));
        }
    }
    directories
}

fn channel_command(paths: &AppPaths, program: &Path) -> Command {
    let mut command = Command::new(program);
    if let Ok(path) = std::env::join_paths(executable_dirs(paths)) {
        command.env("PATH", path);
    }
    command.env("UV_CACHE_DIR", paths.cache.join("uv"));
    command.current_dir(&paths.data_root).stdin(Stdio::null()).kill_on_drop(true);
    command
}

fn find_executable(paths: &AppPaths, name: &str) -> Option<PathBuf> {
    executable_dirs(paths).into_iter().map(|dir| dir.join(name)).find(|path| path.is_file())
}

fn canonical_url(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return value.trim().trim_end_matches('/').to_ascii_lowercase();
    };
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_ascii_lowercase()
}

fn normalize_text(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|character| if character.is_alphanumeric() { character } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn is_http_url(value: &str) -> bool {
    url::Url::parse(value)
        .map(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fake_cli(root: &Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn facebook_existing_browser_session_is_reported_connected() {
        let temp = tempfile::tempdir().unwrap();
        let executable = fake_cli(temp.path(), "opencli", "printf '%s' '[{\"site\":\"facebook\",\"status\":\"logged_in\",\"logged_in\":true}]'");
        let adapter = FacebookAdapter { paths: test_paths(temp.path()), executable: Some(executable) };
        assert!(adapter.doctor().await.authenticated, "a successful upstream session check must reach the UI");
    }

    fn test_paths(root: &Path) -> AppPaths {
        AppPaths {
            data_root: root.into(), database: root.join("db"), generated: root.join("generated"),
            profile: root.join("profile"), workspaces: root.join("workspaces"),
            codex_home: root.join("codex"), backups: root.join("backups"),
            cache: root.join("cache"), logs: root.join("logs"), runtime: root.join("runtime"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn login_runs_the_backend_flow_and_refresh_detects_the_new_session() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = test_paths(temp.path());
        let bin = paths.data_root.join(NPM_TOOL_DIR).join("bin");
        std::fs::create_dir_all(&bin)?;
        let executable = fake_cli(&bin, "opencli", r#"
case "$*" in
  'facebook login --timeout 300 -f json') touch "$0.session"; exit 0;;
  'auth status --site facebook --full --timeout 20 -f json')
    if [ -f "$0.session" ]; then
      printf '%s' '[{"site":"facebook","status":"logged_in","logged_in":true,"identity":"private-person"}]'
    else
      printf '%s' '[{"site":"facebook","status":"not_logged_in","logged_in":false}]'
    fi;;
  *) exit 2;;
esac"#);
        let adapter = FacebookAdapter { paths: paths.clone(), executable: Some(executable) };
        assert_eq!(adapter.doctor().await.status, "login_required");
        let guide = begin_auth(&paths, SearchChannel::Facebook).await?;
        assert!(guide.url.is_none(), "the backend, not the system browser, opened login");
        let health = adapter.doctor().await;
        assert!(health.authenticated);
        assert!(!serde_json::to_string(&health)?.contains("private-person"));
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn opencli_error_rows_are_not_success_even_with_exit_zero() {
        let temp = tempfile::tempdir().unwrap();
        let paths = test_paths(temp.path());
        for (code, expected) in [
            ("BROWSER_CONNECT", AuthProbe::BridgeRequired),
            ("TIMEOUT", AuthProbe::CheckTimedOut),
            ("AUTH_REQUIRED", AuthProbe::LoginRequired),
            ("UNKNOWN", AuthProbe::CheckFailed),
        ] {
            let script = format!("printf '%s' '[{{\"site\":\"facebook\",\"status\":\"error\",\"error\":\"{code}: private-detail\"}}]'");
            let executable = fake_cli(temp.path(), "opencli", &script);
            assert_eq!(opencli_auth_probe(&paths, &executable, "facebook").await, expected);
            assert!(!auth_health(FacebookAdapter { paths: paths.clone(), executable: Some(executable) }.dependencies(), expected).message.contains("private-detail"));
        }
        let executable = fake_cli(temp.path(), "opencli", "printf '%s' '[{\"site\":\"twitter\",\"status\":\"logged_in\",\"logged_in\":true}]'");
        assert_eq!(opencli_auth_probe(&paths, &executable, "facebook").await, AuthProbe::CheckFailed);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bridge_failure_returns_extension_setup_not_a_fake_login_page() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = test_paths(temp.path());
        let bin = paths.data_root.join(NPM_TOOL_DIR).join("bin");
        std::fs::create_dir_all(&bin)?;
        fake_cli(&bin, "opencli", "exit 69");
        let guide = begin_auth(&paths, SearchChannel::Facebook).await?;
        assert_eq!(guide.url.as_deref(), Some(OPENCLI_BRIDGE_URL));
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn linkedin_requires_an_explicit_valid_session_not_just_exit_zero() {
        let temp = tempfile::tempdir().unwrap();
        let paths = test_paths(temp.path());
        for (message, exit, expected) in [
            ("✅ Session is valid (profile: /private/session)", 0, AuthProbe::Ready),
            ("Source cookie validity is not verified in this mode.", 0, AuthProbe::CheckFailed),
            ("❌ Session expired or invalid (profile: /private/session)", 1, AuthProbe::LoginRequired),
            ("❌ No valid source session found", 1, AuthProbe::LoginRequired),
            ("unknown successful output", 0, AuthProbe::CheckFailed),
        ] {
            let executable = fake_cli(temp.path(), "uvx", &format!("printf '%s' '{message}'; exit {exit}"));
            let output = auth_output(&paths, &executable, &["--offline", "mcp-server-linkedin@latest", "--status"], AUTH_CHECK_TIMEOUT).await;
            assert_eq!(linkedin_auth_probe(output), expected);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn linkedin_login_and_status_use_the_same_upstream_session() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = test_paths(temp.path());
        let bin = paths.data_root.join(NPM_TOOL_DIR).join("bin");
        std::fs::create_dir_all(&bin)?;
        let uvx = fake_cli(&bin, "uvx", r#"
case "$*" in
  'mcp-server-linkedin@latest --login')
    [ "$LOGIN_TIMEOUT" = 300 ] || exit 2
    touch "$0.session";;
  '--offline mcp-server-linkedin@latest --status')
    if [ -f "$0.session" ]; then
      printf '%s' '✅ Session is valid (profile: /upstream-owned)'
    else
      printf '%s' '❌ No valid source session found'; exit 1
    fi;;
  *) exit 2;;
esac"#);
        let adapter = LinkedInAdapter {
            paths: paths.clone(), executable: Some(bin.join("mcporter")), uvx: Some(uvx),
        };
        assert!(!adapter.doctor().await.authenticated);
        assert!(begin_auth(&paths, SearchChannel::LinkedIn).await?.url.is_none());
        assert!(adapter.doctor().await.authenticated);
        Ok(())
    }

    // Opt-in only: checks the installed browser bridge without exposing account
    // identities. Normal test/CI runs never inspect the user's browser session.
    #[tokio::test]
    #[ignore = "requires the user's installed OpenCLI/browser bridge"]
    async fn live_opencli_auth_status_reports_only_safe_health() -> Result<()> {
        let paths = AppPaths::resolve(None)?;
        let executable = find_executable(&paths, "opencli").context("OpenCLI not installed")?;
        let health = FacebookAdapter { paths, executable: Some(executable) }.doctor().await;
        println!("{}", serde_json::to_string(&health)?);
        assert!(["ready", "login_required", "bridge_required"].contains(&health.status.as_str()));
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn twitter_uses_authenticated_fallback_for_health_and_search() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = test_paths(temp.path());
        let opencli = fake_cli(temp.path(), "opencli", "exit 77");
        let twitter = fake_cli(temp.path(), "twitter", r#"
case "$*" in
  'status --json') printf '%s' '{"ok":true,"data":{"authenticated":true}}';;
  'search internship --json --max 10') printf '%s' '{"ok":true,"data":[]}';;
  *) exit 2;;
esac"#);
        let adapter = TwitterAdapter { paths, opencli: Some(opencli), twitter: Some(twitter) };
        let health = adapter.doctor().await;
        assert!(health.authenticated);
        assert_eq!(health.backend, "twitter-cli");
        let result = adapter.search(&SearchRequest { query: "internship".into(), location: "".into(), work_mode: "".into(), rss_feeds: vec![] }).await?;
        assert_eq!(result.backend, "twitter-cli");
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn auth_commands_have_a_bounded_timeout() {
        let temp = tempfile::tempdir().unwrap();
        let executable = fake_cli(temp.path(), "slow", "exec sleep 5");
        let result = auth_output(&test_paths(temp.path()), &executable, &[], Duration::from_millis(20)).await;
        assert!(matches!(result, Err(AuthProbe::CheckTimedOut)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn gui_subprocesses_resolve_managed_node_without_the_shell_path() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = test_paths(temp.path());
        let bin = paths.data_root.join("tools/node/bin");
        std::fs::create_dir_all(&bin)?;
        fake_cli(&bin, "careeros-test-runtime", "printf '%s' 'managed-runtime'");
        let executable = fake_cli(temp.path(), "launcher", "exec /usr/bin/env careeros-test-runtime");
        let output = channel_command(&paths, &executable).output().await?;
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "managed-runtime");
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unauthenticated_channels_do_not_run_or_block_other_search_results() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = test_paths(temp.path());
        let bin = paths.data_root.join(NPM_TOOL_DIR).join("bin");
        std::fs::create_dir_all(&bin)?;
        fake_cli(&bin, "mcporter", r#"printf '%s' '{"results":[{"title":"Intern","url":"https://example.com/job"}]}'"#);
        let opencli = fake_cli(&bin, "opencli", r#"[ "$1" = auth ] || touch "$0.search-ran"; exit 77"#);
        fake_cli(&bin, "twitter", r#"printf '%s' '{"ok":false,"error":{"code":"not_authenticated"}}'; exit 1"#);
        fake_cli(&bin, "uvx", "printf '%s' '❌ No valid source session found'; exit 1");
        let capabilities = capabilities(&paths).await?;
        assert_eq!(capabilities.channels.len(), 6);
        assert_eq!(capabilities.channels[0].channel, SearchChannel::WebAts);
        assert!(!capabilities.channels.iter().find(|health| health.channel == SearchChannel::Facebook).unwrap().authenticated);
        let manifest = run(&paths, &json!({"query":"internship"})).await?;
        assert_eq!(manifest.health.len(), 6);
        assert_eq!(manifest.results.len(), 6);
        let exa = manifest.results.iter().find(|result| result.channel == SearchChannel::Exa).unwrap();
        assert_eq!(exa.candidates.len(), 1);
        assert!(manifest.warnings.iter().any(|warning| warning.contains("Facebook")));
        assert!(!PathBuf::from(format!("{}.search-ran", opencli.display())).exists());
        Ok(())
    }

    #[test]
    fn twitter_prefers_opencli_then_falls_back_to_twitter_cli() {
        assert_eq!(select_twitter_backend(true, true), Some("opencli:twitter"));
        assert_eq!(select_twitter_backend(false, true), Some("twitter-cli"));
        assert_eq!(select_twitter_backend(false, false), None);
    }

    #[test]
    fn channel_wire_names_match_frontend_contract() -> Result<()> {
        assert_eq!(serde_json::to_value(SearchChannel::LinkedIn)?, "linkedin");
        assert_eq!(SearchChannel::parse("linked_in"), SearchChannel::LinkedIn);
        assert_eq!(SearchChannel::parse("linkedin"), SearchChannel::LinkedIn);
        Ok(())
    }

    #[test]
    fn cli_json_is_normalized_with_channel_and_backend_evidence() -> Result<()> {
        let value = normalize_json(raw(
            SearchChannel::Twitter,
            "twitter-cli",
            json!({"data":[{"title":"ML intern","company":"Example","url":"https://x.example/jobs/1","text":"A useful role"}]}),
            Vec::new(),
        ))?;
        assert_eq!(value.candidates.len(), 1);
        assert_eq!(value.candidates[0].channel, SearchChannel::Twitter);
        assert_eq!(value.candidates[0].backend, "twitter-cli");
        assert_eq!(value.candidates[0].evidence_type, "secondary");
        Ok(())
    }

    #[test]
    fn normalized_results_dedupe_same_url_across_channels() -> Result<()> {
        let mut results = vec![
            ChannelResults {
                channel: SearchChannel::Exa,
                backend: "exa".into(),
                checked_at: "now".into(),
                candidates: vec![SearchCandidate {
                    title: "Role".into(), organization: Some("Company".into()), description: None,
                    url: "https://example.com/job/#top".into(), published_at: None, location: None,
                    source_title: None, evidence_type: "secondary".into(), channel: SearchChannel::Exa,
                    backend: "exa".into(), checked_at: "now".into(),
                }], warnings: Vec::new(),
            },
            ChannelResults {
                channel: SearchChannel::WebAts,
                backend: "codex_web_search".into(),
                checked_at: "now".into(),
                candidates: vec![SearchCandidate {
                    title: "Role".into(), organization: Some("Company".into()), description: None,
                    url: "https://example.com/job/".into(), published_at: None, location: None,
                    source_title: None, evidence_type: "primary".into(), channel: SearchChannel::WebAts,
                    backend: "codex_web_search".into(), checked_at: "now".into(),
                }], warnings: Vec::new(),
            },
        ];
        dedupe_results(&mut results);
        assert_eq!(results.iter().map(|result| result.candidates.len()).sum::<usize>(), 1);
        assert_eq!(results.iter().find_map(|result| result.candidates.first()).map(|candidate| &candidate.channel), Some(&SearchChannel::WebAts));
        Ok(())
    }

    #[test]
    fn auth_guide_does_not_contain_credential_fields() -> Result<()> {
        let guide = auth_guide(SearchChannel::Twitter)?;
        let serialized = serde_json::to_string(&guide)?;
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("cookie"));
        assert!(!serialized.contains("token"));
        Ok(())
    }

    #[test]
    fn rss_entries_are_parsed_without_an_xml_dependency() {
        let entries = parse_feed(
            "<rss><channel><item><title>ML Intern</title><link>https://example.com/jobs/1</link><description><![CDATA[Build models]]></description></item></channel></rss>",
            "https://example.com/feed.xml",
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["title"], "ML Intern");
        assert_eq!(entries[0]["url"], "https://example.com/jobs/1");
    }
}
