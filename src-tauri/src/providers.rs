use crate::models::{ProviderRuntimeConfig, ProviderRuntimeModel};
use anyhow::{bail, Context, Result};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionRequest {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct ProviderDiscovery {
    pub id: String,
    pub display_name: String,
    pub adapter_kind: String,
    pub base_url: String,
    pub secret_reference: String,
    pub models: Vec<AdapterModel>,
    pub validation_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterModel {
    pub id: String,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
    pub reasoning_levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    pub reasoning: bool,
    pub tools: bool,
    pub vision: bool,
    pub streaming: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedProviderConfig {
    pub config_overrides: Vec<String>,
    pub env_key: String,
    pub secret_reference: String,
}

pub async fn discover_responses_provider(
    request: &ProviderConnectionRequest,
) -> Result<ProviderDiscovery> {
    let api_key = request.api_key.trim();
    if api_key.len() < 8 {
        bail!("API Key 不能为空，且长度不能少于 8 个字符")
    }
    let base_url = normalize_base_url(&request.base_url)?;
    let flavor = ProviderFlavor::from_url(&base_url);
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .user_agent(format!("PostdocOS/{}", env!("CARGO_PKG_VERSION")))
        .build()?;

    let response = client
        .get(endpoint(&base_url, "models")?)
        .bearer_auth(api_key)
        .send()
        .await
        .context("无法连接模型中转站的 /models 接口")?;
    let status = response.status();
    let body = response.text().await.context("无法读取 /models 响应")?;
    if !status.is_success() {
        bail!(
            "模型列表校验失败（HTTP {}）：{}",
            status.as_u16(),
            safe_remote_message(&body, api_key)
        )
    }
    let value: Value = serde_json::from_str(&body).context("/models 没有返回 JSON")?;
    let model_ids = parse_model_ids(&value);
    if model_ids.is_empty() {
        bail!("/models 已响应，但没有发现可供 Codex 使用的文本模型")
    }
    let probe_model = flavor.select_probe_model(&model_ids);
    probe_responses(&client, &base_url, api_key, &probe_model).await?;

    let models = model_ids
        .into_iter()
        .filter(|model| is_probably_agent_model(model))
        .map(|model| flavor.model(&model))
        .collect::<Vec<_>>();
    if models.is_empty() {
        bail!("模型列表中没有发现适合 Codex Agent 的文本模型")
    }
    let id = flavor.provider_id(&base_url);
    Ok(ProviderDiscovery {
        display_name: flavor.display_name(&base_url),
        adapter_kind: flavor.adapter_kind().into(),
        secret_reference: format!("model-provider:{id}:api-key"),
        validation_message: format!(
            "Responses 已验证；发现 {} 个可用模型（探测模型：{}）",
            models.len(), probe_model
        ),
        id,
        base_url,
        models,
    })
}

pub fn prepare_codex_provider(
    codex_home: &Path,
    config: &ProviderRuntimeConfig,
) -> Result<PreparedProviderConfig> {
    let env_key = provider_env_key(&config.id);
    let provider_dir = codex_home.join("providers");
    fs::create_dir_all(&provider_dir)?;
    let catalog_path = provider_dir.join(format!("{}-models.json", config.id));
    let config_snapshot_path = provider_dir.join(format!("{}-config.toml", config.id));

    write_atomic(
        &catalog_path,
        serde_json::to_string_pretty(&model_catalog(config))?.as_bytes(),
    )?;

    let default_model = config.models.first().context("该服务商没有已启用的模型")?;
    let mut root = toml::Table::new();
    root.insert("model".into(), toml::Value::String(default_model.slug.clone()));
    root.insert("model_provider".into(), toml::Value::String(config.id.clone()));
    root.insert(
        "model_catalog_json".into(),
        toml::Value::String(catalog_path.display().to_string()),
    );
    root.insert("preferred_auth_method".into(), toml::Value::String("apikey".into()));
    root.insert("forced_login_method".into(), toml::Value::String("api".into()));
    let mut provider = toml::Table::new();
    provider.insert("name".into(), toml::Value::String(config.display_name.clone()));
    provider.insert("base_url".into(), toml::Value::String(config.base_url.clone()));
    provider.insert("env_key".into(), toml::Value::String(env_key.clone()));
    provider.insert("wire_api".into(), toml::Value::String("responses".into()));
    provider.insert("requires_openai_auth".into(), toml::Value::Boolean(false));
    provider.insert("supports_websockets".into(), toml::Value::Boolean(false));
    let mut providers = toml::Table::new();
    providers.insert(config.id.clone(), toml::Value::Table(provider));
    root.insert("model_providers".into(), toml::Value::Table(providers));
    write_atomic(&config_snapshot_path, toml::to_string_pretty(&root)?.as_bytes())?;

    let quote = |value: &str| toml::Value::String(value.to_owned()).to_string();
    Ok(PreparedProviderConfig {
        config_overrides: vec![
            format!("model={}", quote(&default_model.slug)),
            format!("model_provider={}", quote(&config.id)),
            format!("model_catalog_json={}", quote(&catalog_path.display().to_string())),
            format!("preferred_auth_method={}", quote("apikey")),
            format!("forced_login_method={}", quote("api")),
            format!("model_providers.{}.name={}", config.id, quote(&config.display_name)),
            format!("model_providers.{}.base_url={}", config.id, quote(&config.base_url)),
            format!("model_providers.{}.env_key={}", config.id, quote(&env_key)),
            format!("model_providers.{}.wire_api={}", config.id, quote("responses")),
            format!("model_providers.{}.requires_openai_auth=false", config.id),
            format!("model_providers.{}.supports_websockets=false", config.id),
        ],
        env_key,
        secret_reference: config.secret_reference.clone(),
    })
}

fn normalize_base_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("请输入中转站 URL")
    }
    let mut url = Url::parse(trimmed).context("中转站 URL 必须是完整的 http(s) 地址")?;
    if !url.username().is_empty() || url.password().is_some() {
        bail!("中转站 URL 不能包含用户名或密码")
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("中转站 URL 不能包含查询参数或片段")
    }
    let host = url.host_str().context("中转站 URL 缺少主机名")?;
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(host) => {}
        "http" => bail!("远程中转站必须使用 HTTPS；HTTP 只允许 localhost/127.0.0.1"),
        _ => bail!("中转站 URL 只支持 HTTPS，或本机 HTTP"),
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(if path.is_empty() { "" } else { &path });
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn endpoint(base_url: &str, suffix: &str) -> Result<Url> {
    Url::parse(&format!("{}/{suffix}", base_url.trim_end_matches('/')))
        .context("无法构造模型服务接口地址")
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn parse_model_ids(value: &Value) -> Vec<String> {
    let source = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array);
    let mut seen = HashSet::new();
    let mut models = source
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().or_else(|| item.get("id").and_then(Value::as_str)))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert((*id).to_owned()))
        .map(str::to_owned)
        .take(200)
        .collect::<Vec<_>>();
    models.sort();
    models
}

async fn probe_responses(client: &Client, base_url: &str, api_key: &str, model: &str) -> Result<()> {
    let response = client
        .post(endpoint(base_url, "responses")?)
        .bearer_auth(api_key)
        .json(&json!({
            "model": model,
            "input": "Reply with OK.",
            "max_output_tokens": 16,
            "stream": false
        }))
        .send()
        .await
        .context("无法连接模型中转站的 /responses 接口")?;
    let status = response.status();
    let body = response.text().await.context("无法读取 /responses 响应")?;
    if !status.is_success() {
        let hint = if matches!(status, StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED) {
            "该地址可能只支持 Chat Completions；当前 Codex 直连要求 Responses API。"
        } else {
            "请确认该 Key 有权调用探测模型，且中转站完整兼容 Responses API。"
        };
        bail!(
            "Responses 兼容性探测失败（HTTP {}）：{} {}",
            status.as_u16(),
            safe_remote_message(&body, api_key),
            hint
        )
    }
    let value: Value = serde_json::from_str(&body).context("/responses 没有返回 JSON")?;
    if !value.is_object() {
        bail!("/responses 返回格式不兼容")
    }
    Ok(())
}

fn safe_remote_message(body: &str, secret: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let redacted = compact.replace(secret, "***");
    if redacted.is_empty() {
        "远端没有返回错误详情".into()
    } else {
        redacted.chars().take(500).collect()
    }
}

fn is_probably_agent_model(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    ![
        "embedding", "whisper", "moderation", "rerank", "text-to-speech", "tts-",
        "image-generation", "dall-e",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn provider_env_key(provider_id: &str) -> String {
    let normalized = provider_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("POSTDOCOS_{normalized}_API_KEY")
}

fn model_catalog(config: &ProviderRuntimeConfig) -> Value {
    let context_window = if config.adapter_kind == "deepseek_responses" {
        1_048_576
    } else {
        128_000
    };
    json!({
        "models": config.models.iter().enumerate()
            .map(|(index, model)| model_catalog_entry(model, index, context_window))
            .collect::<Vec<_>>()
    })
}

fn model_catalog_entry(model: &ProviderRuntimeModel, index: usize, context_window: i64) -> Value {
    let default_reasoning = if model.reasoning_levels.iter().any(|level| level == "high") {
        "high"
    } else {
        model.reasoning_levels.first().map(String::as_str).unwrap_or("none")
    };
    let reasoning = model.reasoning_levels.iter()
        .map(|effort| json!({"effort": effort, "description": format!("{} reasoning", effort)}))
        .collect::<Vec<_>>();
    let modalities = if model.supports_vision { json!(["text", "image"]) } else { json!(["text"]) };
    let auto_compact_token_limit = (context_window * 3 / 4).min(256_000);
    json!({
        "slug": model.slug,
        "prefer_websockets": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": "freeform",
        "web_search_tool_type": "text",
        "input_modalities": modalities,
        "supports_image_detail_original": model.supports_vision,
        "truncation_policy": {"mode": "tokens", "limit": 10000},
        "supports_parallel_tool_calls": true,
        "tool_mode": null,
        "multi_agent_version": "v2",
        "use_responses_lite": false,
        "include_skills_usage_instructions": false,
        "auto_review_model_override": null,
        "context_window": context_window,
        "max_context_window": context_window,
        "effective_context_window_percent": 95,
        "auto_compact_token_limit": auto_compact_token_limit,
        "comp_hash": "3000",
        "reasoning_summary_format": "experimental",
        "default_reasoning_summary": "none",
        "display_name": model.display_name,
        "description": "Responses-compatible model connected through PostdocOS.",
        "default_reasoning_level": default_reasoning,
        "supported_reasoning_levels": reasoning,
        "shell_type": "shell_command",
        "visibility": "list",
        "minimal_client_version": "0.144.0",
        "supported_in_api": true,
        "availability_nux": null,
        "upgrade": null,
        "priority": index + 1,
        "model_messages": null,
        "experimental_supported_tools": [],
        "supports_search_tool": true,
        "default_service_tier": null,
        "supports_reasoning_summaries": true,
        "base_instructions": "You are Codex, an agent working in the user's workspace. Follow developer and AGENTS.md instructions, use available tools carefully, complete the requested work, validate it, and report the result."
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path.file_name().and_then(|name| name.to_str()).context("配置文件名无效")?;
    let temporary = path.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&temporary, bytes)
        .with_context(|| format!("无法写入配置临时文件 {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("无法更新配置文件 {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderFlavor {
    DeepSeek,
    GenericResponses,
}

impl ProviderFlavor {
    fn from_url(base_url: &str) -> Self {
        let host = Url::parse(base_url).ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();
        if host == "deepseek.com" || host.ends_with(".deepseek.com") {
            Self::DeepSeek
        } else {
            Self::GenericResponses
        }
    }

    fn adapter_kind(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek_responses",
            Self::GenericResponses => "responses_relay",
        }
    }

    fn provider_id(self, base_url: &str) -> String {
        if self == Self::DeepSeek {
            return "deepseek".into();
        }
        let digest = Sha256::digest(base_url.as_bytes());
        format!("relay-{}", &format!("{digest:x}")[..12])
    }

    fn display_name(self, base_url: &str) -> String {
        if self == Self::DeepSeek {
            return "DeepSeek".into();
        }
        Url::parse(base_url).ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "Responses 中转站".into())
    }

    fn select_probe_model(self, models: &[String]) -> String {
        let preferred = match self {
            Self::DeepSeek => ["deepseek-v4-flash", "deepseek-v4-pro"].as_slice(),
            Self::GenericResponses => ["gpt-5.6-sol", "gpt-5", "gpt-4.1"].as_slice(),
        };
        preferred.iter()
            .find(|preferred| models.iter().any(|model| model == **preferred))
            .map(|model| (*model).to_owned())
            .or_else(|| models.iter().find(|model| is_probably_agent_model(model)).cloned())
            .unwrap_or_else(|| models[0].clone())
    }

    fn model(self, id: &str) -> AdapterModel {
        let vision = id.to_ascii_lowercase().contains("vision");
        let reasoning_levels = match self {
            Self::DeepSeek => vec!["low".into(), "high".into(), "max".into()],
            Self::GenericResponses => vec!["low".into(), "medium".into(), "high".into(), "xhigh".into()],
        };
        AdapterModel {
            id: id.into(),
            display_name: id.into(),
            capabilities: ModelCapabilities {
                reasoning: true,
                tools: true,
                vision,
                streaming: true,
            },
            reasoning_levels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn base_url_requires_https_except_for_loopback() {
        assert_eq!(normalize_base_url(" https://relay.example/v1/ ").unwrap(), "https://relay.example/v1");
        assert_eq!(normalize_base_url("http://127.0.0.1:8042/v1/").unwrap(), "http://127.0.0.1:8042/v1");
        assert!(normalize_base_url("http://relay.example/v1").is_err());
        assert!(normalize_base_url("https://user:secret@relay.example/v1").is_err());
        assert!(normalize_base_url("https://relay.example/v1?token=secret").is_err());
    }

    #[test]
    fn model_list_parser_accepts_openai_and_simple_shapes() {
        assert_eq!(parse_model_ids(&json!({"data":[{"id":"gpt-b"},{"id":"gpt-a"}]})), vec!["gpt-a", "gpt-b"]);
        assert_eq!(parse_model_ids(&json!({"models":["deepseek-v4-pro"]})), vec!["deepseek-v4-pro"]);
    }

    #[test]
    fn deepseek_is_first_class_and_generic_ids_are_stable() {
        let deepseek = ProviderFlavor::from_url("https://api.deepseek.com");
        assert_eq!(deepseek, ProviderFlavor::DeepSeek);
        assert_eq!(deepseek.provider_id("https://api.deepseek.com"), "deepseek");
        let generic = ProviderFlavor::from_url("https://relay.example/v1");
        assert_eq!(generic.provider_id("https://relay.example/v1"), generic.provider_id("https://relay.example/v1"));
        assert!(generic.provider_id("https://relay.example/v1").starts_with("relay-"));
    }

    #[test]
    fn generated_profile_uses_env_key_and_never_contains_a_secret() -> Result<()> {
        let temp = TempDir::new()?;
        let config = ProviderRuntimeConfig {
            id: "deepseek".into(),
            display_name: "DeepSeek".into(),
            adapter_kind: "deepseek_responses".into(),
            base_url: "https://api.deepseek.com".into(),
            secret_reference: "model-provider:deepseek:api-key".into(),
            models: vec![ProviderRuntimeModel {
                slug: "deepseek-v4-flash".into(),
                display_name: "DeepSeek V4 Flash".into(),
                supports_vision: false,
                reasoning_levels: vec!["low".into(), "high".into(), "max".into()],
            }],
        };
        let prepared = prepare_codex_provider(temp.path(), &config)?;
        let profile = fs::read_to_string(temp.path().join("providers/deepseek-config.toml"))?;
        assert!(profile.contains("env_key"));
        assert!(profile.contains("POSTDOCOS_DEEPSEEK_API_KEY"));
        assert!(profile.contains("supports_websockets = false"));
        assert!(!profile.contains("sk-test-secret"));
        assert!(prepared.config_overrides.iter().any(|value| value == "model_provider=\"deepseek\""));
        assert!(prepared.config_overrides.iter().any(|value| value == "model_providers.deepseek.supports_websockets=false"));
        assert!(prepared.config_overrides.iter().all(|value| !value.contains("sk-test-secret")));
        let _: toml::Table = toml::from_str(&profile)?;
        let catalog: Value = serde_json::from_str(&fs::read_to_string(temp.path().join("providers/deepseek-models.json"))?)?;
        assert_eq!(catalog["models"][0]["slug"], "deepseek-v4-flash");
        assert_eq!(catalog["models"][0]["auto_compact_token_limit"], 256_000);
        Ok(())
    }

    #[tokio::test]
    async fn discovery_validates_models_and_responses_without_a_real_key() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            for response_body in [
                r#"{"data":[{"id":"gpt-test"},{"id":"embedding-test"}]}"#,
                r#"{"id":"resp-test","object":"response","status":"completed"}"#,
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = vec![0_u8; 8192];
                let _ = stream.read(&mut request).await.unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let discovery = discover_responses_provider(&ProviderConnectionRequest {
            base_url: format!("http://{address}"),
            api_key: "test-api-key".into(),
        })
        .await?;
        server.await?;
        assert!(discovery.id.starts_with("relay-"));
        assert_eq!(discovery.models.len(), 1);
        assert_eq!(discovery.models[0].id, "gpt-test");
        assert!(discovery.validation_message.contains("Responses 已验证"));
        Ok(())
    }
}
