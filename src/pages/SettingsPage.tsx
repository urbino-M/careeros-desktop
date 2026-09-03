import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  CircleAlert,
  CheckCircle2,
  ExternalLink,
  FileText,
  Globe2,
  KeyRound,
  Mail,
  PlugZap,
  RefreshCw,
  Save,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import { ErrorState, LoadingState, searchChannelLabels } from "../components/Ui";
import type { AuthGuide, CvCustomizationSettings, GmailStatus, ProviderInfo, SearchCapabilities, SearchChannel, TaskModelDefault } from "../types";
import { useDesktopUpdates } from "../updates/UpdateManager";

const taskLabels: Record<string, string> = {
  full_search: "完整检索",
  research_pi: "按姓名研究联系人",
  material_revision: "材料修订",
  reply_followup: "回复处理",
  maintenance: "检查与维护",
};

const searchAuthChannels: SearchChannel[] = ["facebook", "linkedin", "twitter"];
const SEARCH_AUTH_TIMEOUT_MS = 60_000;

export function SettingsPage({ onRestartOnboarding, focusSection }: { onRestartOnboarding: () => Promise<void>; focusSection?: "search-channels" }) {
  const [providers, setProviders] = useState<ProviderInfo[]>();
  const [defaults, setDefaults] = useState<TaskModelDefault[]>();
  const [cvCustomization, setCvCustomization] = useState<CvCustomizationSettings>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const load = () => Promise.all([api.providers(), api.taskDefaults(), api.cvCustomization()])
    .then(([p, d, cv]) => { setProviders(p); setDefaults(d); setCvCustomization(cv); })
    .catch((value) => setError(errorMessage(value)));
  useEffect(() => { load(); }, []);
  useEffect(() => {
    if (!focusSection || !providers) return;
    const timer = window.setTimeout(() => document.getElementById(focusSection)?.scrollIntoView({ behavior: "smooth", block: "start" }), 0);
    return () => window.clearTimeout(timer);
  }, [focusSection, providers]);

  if (error) return <div className="page"><ErrorState message={error} retry={load} /></div>;
  if (!providers || !defaults || !cvCustomization) return <div className="page"><LoadingState label="正在读取本机设置" /></div>;

  return (
    <div className="page settings-page">
      <header className="page-header">
        <div className="eyebrow">LOCAL-FIRST CONTROL</div>
        <h1>设置</h1>
        <p>账号凭据保存在当前用户的 CareerOS 私有凭据文件中；数据库、材料和 Codex 状态保存在系统标准应用目录。</p>
        <button className="button ghost settings-onboarding-button" onClick={() => void onRestartOnboarding().catch((value) => setNotice(errorMessage(value)))}>重新打开开始使用引导</button>
      </header>
      {notice && <div className="inline-notice">{notice}</div>}

      <SettingsSection index="01" title="OpenAI / Codex" icon={Bot} badge="V1 已启用">
        <CodexAccountCard onNotice={setNotice} />
      </SettingsSection>

      <SettingsSection index="02" title="任务默认模型" icon={PlugZap} badge="可逐次覆盖">
        <div className="defaults-table">
          {defaults.map((value) => (
            <DefaultRow key={value.taskType} value={value} providers={providers} onSaved={(message) => { setNotice(message); load(); }} />
          ))}
        </div>
      </SettingsSection>

      <SettingsSection index="03" title="CV 定制" icon={FileText} badge="Agent 自动遵从">
        <CvCustomizationCard
          value={cvCustomization}
          onSaved={(saved) => {
            setCvCustomization(saved);
            setNotice(saved.enabled ? "CV 定制规则已启用，后续 Agent 任务会自动遵从。" : "CV 定制规则已保存但当前未启用。");
          }}
          onError={(value) => setNotice(errorMessage(value))}
        />
      </SettingsSection>

      <SettingsSection index="04" title="Gmail 草稿" icon={Mail} badge="只创建草稿">
        <GmailSettingsCard onNotice={setNotice} />
      </SettingsSection>

      <SettingsSection index="05" title="模型中转站 / DeepSeek" icon={KeyRound} badge="Responses 直连">
        <ProviderConnectionSettings
          providers={providers.filter((provider) => provider.id !== "openai")}
          onChanged={(message) => { setNotice(message); load(); }}
        />
      </SettingsSection>

      <SettingsSection index="06" title="管理信息搜索渠道" icon={Globe2} badge="一键启用 · 浏览器连接" id="search-channels">
        <SearchChannelsSettings />
      </SettingsSection>

      <SettingsSection index="07" title="应用更新" icon={RefreshCw} badge="签名校验">
        <ApplicationUpdateCard />
      </SettingsSection>

    </div>
  );
}

function SearchChannelsSettings() {
  const [capabilities, setCapabilities] = useState<SearchCapabilities>();
  const [authGuide, setAuthGuide] = useState<AuthGuide>();
  const [authPending, setAuthPending] = useState<SearchChannel>();
  const [authTimedOut, setAuthTimedOut] = useState(false);
  const [busy, setBusy] = useState(false);
  const [setupPending, setSetupPending] = useState<SearchChannel>();
  const [notice, setNotice] = useState("");

  const refresh = async (announce = true) => {
    setBusy(true);
    try {
      const next = await api.searchCapabilities();
      setCapabilities(next);
      const guidedHealth = authGuide && next.channels.find((item) => item.channel === authGuide.channel);
      if (guidedHealth?.authenticated && authGuide) {
        setAuthPending(undefined);
        setAuthTimedOut(false);
        setNotice(`${searchChannelLabels[authGuide.channel]} 已连接。`);
      } else if (announce) {
        setNotice("信息搜索渠道状态已刷新。");
      }
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => { void refresh(false); }, []);

  useEffect(() => {
    if (!authPending) return;
    let active = true;
    const startedAt = Date.now();
    const check = async () => {
      try {
        const next = await api.searchCapabilities();
        if (!active) return;
        setCapabilities(next);
        const health = next.channels.find((item) => item.channel === authPending);
        if (health?.authenticated) {
          setAuthPending(undefined);
          setAuthTimedOut(false);
          setNotice(`${searchChannelLabels[authPending]} 已连接。`);
          return;
        }
        if (Date.now() - startedAt >= SEARCH_AUTH_TIMEOUT_MS) {
          setAuthPending(undefined);
          setAuthTimedOut(true);
          setNotice(`${searchChannelLabels[authPending]} 认证页已打开，但尚未检测到登录状态；完成认证后可点击“立即检查”。`);
        }
      } catch (value) {
        if (active) setNotice(errorMessage(value));
      }
    };
    void check();
    const timer = window.setInterval(() => void check(), 2_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [authPending]);

  const openAuth = async (channel: SearchChannel) => {
    const guide = await api.beginSearchChannelAuth(channel);
    setAuthGuide(guide);
    setAuthTimedOut(false);
    if (guide.url) await openUrl(guide.url);
    setAuthPending(channel);
    setNotice(`${searchChannelLabels[channel]} 认证页已打开；完成登录后 CareerOS 会自动检查状态。`);
  };

  const connect = async (channel: SearchChannel) => {
    setBusy(true);
    setNotice("");
    try {
      let setupMessage = "";
      if (channel === "linkedin") {
        const result = await api.setupSearchCapabilities([channel]);
        setCapabilities(result.capabilities);
        setupMessage = result.messages.filter(Boolean).join("\n");
        const health = result.capabilities.channels.find((item) => item.channel === channel);
        if (!health?.available) {
          setNotice(setupMessage || "LinkedIn 尚未准备好，请稍后重试。");
          return;
        }
      }
      await openAuth(channel);
      if (setupMessage) setNotice(`${setupMessage}\nLinkedIn 认证页已打开；完成登录后 CareerOS 会自动检查状态。`);
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  const enable = async (channel: SearchChannel) => {
    setSetupPending(channel);
    setBusy(true);
    setNotice("");
    try {
      const result = await api.setupSearchCapabilities([channel]);
      setCapabilities(result.capabilities);
      const health = result.capabilities.channels.find((item) => item.channel === channel);
      const messages = result.messages.filter(Boolean).join("\n");
      if (health?.available && isSearchAuthChannel(channel) && !health.authenticated) {
        await openAuth(channel);
        setNotice(`${messages ? `${messages}\n` : ""}${searchChannelLabels[channel]} 已准备，已打开浏览器认证页；完成登录后 CareerOS 会自动检查状态。`);
      } else {
        setNotice(messages || `${searchChannelLabels[channel]} 已准备。`);
      }
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
      setSetupPending(undefined);
    }
  };

  const readyCount = capabilities?.channels.filter((health) => health.available && (!isSearchAuthChannel(health.channel) || health.authenticated)).length ?? 0;
  return (
    <div className="search-channels-settings">
      <div className="settings-info-card search-channels-intro">
        <div className="settings-icon"><Globe2 size={22} /></div>
        <div>
          <h3>让 CareerOS 连接更多信息来源</h3>
          <p>官方 Web / ATS、Exa、RSS、LinkedIn、Facebook 和 Twitter / X 会分别检查。可用渠道并行搜索，某个渠道失败不会阻断其他渠道。</p>
          {capabilities ? <span className={readyCount === capabilities.channels.length ? "connected-label" : "gmail-state"}><CheckCircle2 size={15} /> {readyCount} / {capabilities.channels.length} 个渠道可用</span> : <span className="gmail-state">正在检查渠道…</span>}
        </div>
        <button className="button ghost" disabled={busy} onClick={() => void refresh()}><RefreshCw size={15} className={busy ? "spinning" : ""} /> 刷新状态</button>
      </div>

      {notice && <div className="inline-notice search-channels-notice"><CircleAlert size={16} /><span>{notice}</span></div>}

      <div className="channel-health-grid settings-channel-grid">
        {(capabilities?.channels ?? []).map((health) => {
          const authRequired = isSearchAuthChannel(health.channel);
          const pending = authPending === health.channel;
          const state = channelState(health, pending);
          return (
            <article className={`channel-health-item channel-${health.available ? "available" : "missing"}`} key={health.channel}>
              <div className="channel-health-topline"><strong>{searchChannelLabels[health.channel]}</strong><span className={`channel-state channel-state-${state.tone}`}>{state.label}</span></div>
              <span className="channel-backend">{health.backend}</span>
              <p>{health.message}</p>
              <div className="channel-health-footer">
                <small>检查于 {formatSettingsDate(health.checkedAt)}</small>
                {!health.available && health.channel !== "rss" && <button className="button ghost" disabled={busy} onClick={() => void enable(health.channel)}>{setupPending === health.channel ? "安装中…" : "一键启用"}</button>}
                {!health.available && health.channel === "rss" && <small>请在 Internship 画像中添加 RSS 地址</small>}
                {health.available && authRequired && <button className="button ghost" disabled={busy || pending} onClick={() => void connect(health.channel)}>{pending ? "等待认证…" : health.authenticated ? "重新连接" : "连接渠道"}</button>}
                {health.available && !authRequired && <span className="connected-label"><CheckCircle2 size={14} /> 可直接使用</span>}
              </div>
            </article>
          );
        })}
      </div>
      {!capabilities && <div className="planning-section-note">正在读取信息搜索渠道状态…</div>}

      {authGuide && <div className="auth-guide settings-auth-guide">
        <div>
          <strong>{authGuide.title}</strong>
          {authGuide.instructions.map((instruction) => <span key={instruction}>· {instruction}</span>)}
          {authPending && <span className="gmail-state">正在等待浏览器完成认证，CareerOS 每 2 秒检查一次。</span>}
          {!authPending && authTimedOut && <span className="gmail-state">未检测到已连接状态；请确认浏览器登录成功后点击“立即检查”。</span>}
        </div>
        <div className="auth-guide-actions">
          {authGuide.url && <button className="button ghost" onClick={() => void openUrl(authGuide.url!)}><ExternalLink size={14} /> 再次打开</button>}
          <button className="button ghost" disabled={busy} onClick={() => void refresh()}><RefreshCw size={14} /> 立即检查</button>
        </div>
      </div>}
    </div>
  );
}

function ApplicationUpdateCard() {
  const updates = useDesktopUpdates();
  const busy = updates.phase === "checking" || updates.phase === "downloading" || updates.phase === "installing";
  return (
    <article className="application-update-card">
      <div className="settings-icon"><RefreshCw size={22} /></div>
      <div>
        <h3>CareerOS v{updates.currentVersion}</h3>
        <p>启动后自动检查 GitHub Release，之后每小时检查一次。下载的更新包必须通过内置公钥验签，失败时不会安装。</p>
        <span className={updates.phase === "error" ? "gmail-state" : "connected-label"}><ShieldCheck size={15} /> {updates.message}</span>
        <small>上次检查：{updates.lastCheckedLabel}</small>
      </div>
      <button className="button secondary" disabled={busy} onClick={() => void updates.checkNow()}><RefreshCw size={16} className={busy ? "spinning" : ""} /> {updates.phase === "checking" ? "检查中…" : "检查更新"}</button>
    </article>
  );
}

function CvCustomizationCard({ value, onSaved, onError }: { value: CvCustomizationSettings; onSaved: (value: CvCustomizationSettings) => void; onError: (value: unknown) => void }) {
  const [draft, setDraft] = useState(value);
  const [busy, setBusy] = useState(false);
  useEffect(() => setDraft(value), [value]);
  const save = async () => {
    setBusy(true);
    try { onSaved(await api.saveCvCustomization(draft)); }
    catch (value) { onError(value); }
    finally { setBusy(false); }
  };
  return (
    <article className="cv-customization-card">
      <div className="cv-customization-intro">
        <div><h3>统一控制所有目标 CV 的选择与呈现</h3><p>完整检索、按姓名研究和 CV 修订都会读取这里。事实真实性、正好两页、文章与专利在项目前等硬规则始终优先。</p></div>
        <label className="cv-customization-toggle"><input type="checkbox" checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} /><span>{draft.enabled ? "已启用" : "未启用"}</span></label>
      </div>
      <div className="cv-customization-grid">
        <label><span>重点强调</span><textarea value={draft.emphasize} onChange={(event) => setDraft({ ...draft, emphasize: event.target.value })} placeholder="填写希望优先呈现的经历、方法、成果类型或能力。" /></label>
        <label><span>需要排除或弱化</span><textarea value={draft.exclude} onChange={(event) => setDraft({ ...draft, exclude: event.target.value })} placeholder="填写与目标无关、已经过时或不希望重复出现的内容。" /></label>
        <label className="wide"><span>其他定制要求</span><textarea value={draft.instructions} onChange={(event) => setDraft({ ...draft, instructions: event.target.value })} placeholder="填写章节顺序、语气、语言或针对不同机会的组织要求。" /></label>
      </div>
      <div className="cv-customization-actions"><span>这些内容只作为定制指令，不会被当作个人事实或研究证据。</span><button className="button primary" disabled={busy} onClick={() => void save()}><Save size={16} /> {busy ? "保存中…" : "保存 CV 定制"}</button></div>
    </article>
  );
}

function GmailSettingsCard({ onNotice }: { onNotice: (value: string) => void }) {
  const [status, setStatus] = useState<GmailStatus>();
  const [busy, setBusy] = useState(false);
  const refresh = () => api.gmailStatus().then(setStatus).catch((value) => onNotice(errorMessage(value)));
  useEffect(() => {
    refresh();
    const timer = window.setInterval(() => {
      if (status?.oauthStatus === "pending") refresh();
    }, 1800);
    return () => window.clearInterval(timer);
  }, [status?.oauthStatus]);

  const chooseClient = async () => {
    const selected = await openDialog({ multiple: false, directory: false, filters: [{ name: "Google OAuth JSON", extensions: ["json"] }] });
    if (!selected) return;
    setBusy(true);
    try { await api.importGmailClient(selected); onNotice("OAuth 桌面客户端已安全导入；现在可以连接 Gmail。"); refresh(); }
    catch (value) { onNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };
  const connect = async () => {
    setBusy(true);
    try {
      const result = await api.startGmailOAuth();
      await openUrl(result.authorizationUrl);
      onNotice("已在系统浏览器打开 Google 授权页。完成后直接回到 CareerOS。");
      refresh();
    } catch (value) { onNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };
  return (
    <div className="gmail-settings-card">
      <div className="settings-info-card">
        <div className="settings-icon"><Mail size={22} /></div>
        <div>
          <h3>系统浏览器 OAuth 与远端草稿核验</h3>
          <p>只申请 Gmail Compose 权限；凭据保存在当前用户的 CareerOS 私有文件中。CareerOS 没有发送接口，创建后还会用 draft ID 从 Gmail 重新读取确认。</p>
          {status ? (
            <span className={status.connectionOk ? "connected-label" : "gmail-state"}>
              {status.connectionOk ? <><CheckCircle2 size={15} /> 已连接 {status.accountEmail}</> : status.oauthStatus === "pending" ? "等待浏览器授权…" : status.oauthMessage || (status.configured ? "客户端已配置，尚未连接" : "尚未配置")}
            </span>
          ) : <span className="gmail-state">正在检查…</span>}
        </div>
        <span className="reserved-chip">绝不发送</span>
      </div>
      <div className="gmail-action-row">
        <button className="button ghost" onClick={() => openUrl("https://developers.google.com/workspace/gmail/api/quickstart/nodejs")}><ExternalLink size={16} /> Google 官方 JSON 创建教程</button>
        <button className="button secondary" disabled={busy} onClick={chooseClient}><KeyRound size={16} /> 选择 OAuth 客户端 JSON</button>
        <button className="button primary" disabled={busy || !status?.configured} onClick={connect}><ExternalLink size={16} /> 连接 Gmail</button>
        <button className="button ghost" disabled={busy} onClick={refresh}>刷新状态</button>
      </div>
      <div className="settings-footnote">请选择“桌面应用”类型的 OAuth 客户端 JSON。若 OAuth 应用仍处于测试模式，请在 Google Cloud 中把准备连接的账号加入测试用户。</div>
    </div>
  );
}

function SettingsSection({ index, title, icon: Icon, badge, id, children }: { index: string; title: string; icon: typeof Bot; badge: string; id?: string; children: React.ReactNode }) {
  return (
    <section className="settings-section" id={id}>
      <div className="section-heading"><div><span className="section-index">{index}</span><h2><Icon size={20} /> {title}</h2></div><span>{badge}</span></div>
      {children}
    </section>
  );
}

function isSearchAuthChannel(channel: SearchChannel) {
  return searchAuthChannels.includes(channel);
}

function channelState(health: SearchCapabilities["channels"][number], pending: boolean) {
  if (pending) return { label: "等待认证", tone: "pending" };
  if (!health.available) return { label: health.channel === "rss" ? "待配置" : "需准备", tone: "missing" };
  if (isSearchAuthChannel(health.channel) && !health.authenticated) return { label: "需连接", tone: "missing" };
  return { label: "可用", tone: "ready" };
}

function formatSettingsDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function CodexAccountCard({ onNotice }: { onNotice: (value: string) => void }) {
  const [checking, setChecking] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const [status, setStatus] = useState<Record<string, unknown>>();
  const check = async (announce = true) => {
    setChecking(true);
    try {
      const result = await api.codexAccount();
      setStatus(result);
      if (announce) onNotice(accountIsChatGpt(result) ? "ChatGPT / Codex 已连接。" : "Codex 账号状态已刷新。");
    }
    catch (value) { onNotice(errorMessage(value)); }
    finally { setChecking(false); }
  };
  const connect = async () => {
    setChecking(true);
    try {
      const result = await api.connectChatGpt();
      const url = findUrl(result);
      const loginId = findKeyString(result, "loginId");
      if (!url || !loginId) throw new Error("Codex 没有返回完整的授权地址，请重试");
      await openUrl(url);
      setWaiting(true);
      onNotice("等待浏览器完成 ChatGPT 授权；成功后 CareerOS 会自动识别并回到应用。");
      void api.waitForChatGptLogin(loginId)
        .then((account) => {
          setStatus(account);
          onNotice("ChatGPT / Codex 已连接。浏览器成功页可以关闭。");
        })
        .catch((value) => onNotice(errorMessage(value)))
        .finally(() => setWaiting(false));
    } catch (value) { onNotice(errorMessage(value)); }
    finally { setChecking(false); }
  };
  const connected = status ? accountIsChatGpt(status) : false;
  const accountEmail = status ? findKeyString(status, "email") : undefined;
  return (
    <article className="account-card">
      <div className="settings-icon"><Bot size={23} /></div>
      <div className="account-copy">
        <h3>ChatGPT / Codex OAuth</h3>
        <p>使用系统浏览器登录；不在 CareerOS 中输入 ChatGPT 密码。</p>
        {waiting && <span className="gmail-state">等待浏览器授权…</span>}
        {!waiting && connected && <span className="connected-label"><CheckCircle2 size={15} /> 已连接{accountEmail ? ` ${accountEmail}` : ""}</span>}
        {!waiting && status && !connected && <span className="gmail-state">尚未连接</span>}
        {!waiting && !status && <span className="gmail-state">按需检查，不会在进入设置时启动 Codex</span>}
        {status && <details className="technical-details"><summary>账号技术详情</summary><pre>{JSON.stringify(status, null, 2)}</pre></details>}
      </div>
      <div className="account-actions"><button className="button primary" disabled={checking || waiting} onClick={connect}><ExternalLink size={16} /> {waiting ? "等待授权" : "连接 ChatGPT"}</button><button className="button ghost" disabled={checking} onClick={() => void check()}>{checking ? "检查中…" : "检查状态"}</button></div>
    </article>
  );
}

function ProviderConnectionSettings({ providers, onChanged }: { providers: ProviderInfo[]; onChanged: (message: string) => void }) {
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);
  const connect = async () => {
    setBusy(true);
    try {
      const provider = await api.connectResponsesProvider({ baseUrl, apiKey });
      setApiKey("");
      onChanged(`${provider.displayName} 已连接；发现 ${provider.models.length} 个可用模型。`);
    } catch (value) {
      onChanged(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };
  const disconnect = async (provider: ProviderInfo) => {
    setBusy(true);
    try {
      await api.disconnectResponsesProvider(provider.id);
      onChanged(`${provider.displayName} 已断开，API Key 已从 CareerOS 凭据文件删除。`);
    } catch (value) {
      onChanged(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="provider-settings-stack">
      <article className="provider-connect-card">
        <div className="provider-connect-heading">
          <div><h3>用 URL + API Key 接入</h3><p>自动读取 <code>/models</code> 并做一次最小 <code>/responses</code> 兼容性探测。密钥保存在当前用户的 CareerOS 私有凭据文件中。</p></div>
          <button className="button ghost" disabled={busy} onClick={() => setBaseUrl("https://api.deepseek.com")}>使用 DeepSeek 官方地址</button>
        </div>
        <div className="provider-connect-fields">
          <label><span>Base URL</span><input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://relay.example/v1" /></label>
          <label><span>API Key</span><input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder="只保存到 CareerOS 私有凭据文件" /></label>
          <button className="button primary" disabled={busy || !baseUrl.trim() || apiKey.trim().length < 8} onClick={connect}><PlugZap size={16} /> {busy ? "正在验证…" : "验证并连接"}</button>
        </div>
        <div className="settings-footnote">当前直连只接受 OpenAI Responses 兼容服务。若地址只有 Chat Completions，连接时会明确拦截；本次探测会产生极少量模型 token。</div>
      </article>
      {providers.length > 0 && <div className="provider-grid">
        {providers.map((provider) => (
          <article className={`provider-card${provider.enabled ? "" : " disabled"}`} key={provider.id}>
            <div className="provider-top"><h3>{provider.displayName}</h3><span>{provider.enabled ? "已连接" : "已断开"}</span></div>
            <p className="provider-url">{provider.baseUrl}</p>
            <p>{provider.validationMessage || "尚未完成 Responses 校验"}</p>
            <div className="provider-model-chips">{provider.models.filter((model) => model.enabled).slice(0, 8).map((model) => <span key={model.id}>{model.displayName}</span>)}</div>
            {provider.enabled && <button className="button ghost danger" disabled={busy} onClick={() => void disconnect(provider)}>断开并删除密钥</button>}
          </article>
        ))}
      </div>}
    </div>
  );
}

function DefaultRow({ value, providers, onSaved }: { value: TaskModelDefault; providers: ProviderInfo[]; onSaved: (message: string) => void }) {
  const [draft, setDraft] = useState(value);
  const provider = providers.find((item) => item.id === draft.providerId);
  const enabledProviders = providers.filter((item) => item.enabled);
  const selectedModel = provider?.models.find((model) => model.id === draft.modelId);
  const reasoningLevels = selectedModel?.reasoningLevels.length ? selectedModel.reasoningLevels : ["low", "medium", "high"];
  return (
    <div className="default-row">
      <strong>{taskLabels[value.taskType] || value.taskType}</strong>
      <select value={draft.providerId} onChange={(event) => { const next = enabledProviders.find((item) => item.id === event.target.value)!; const model = next.models.find((item) => item.enabled); const levels = model?.reasoningLevels || []; setDraft({ ...draft, providerId: next.id, modelId: model?.id || "", reasoning: levels.includes("high") ? "high" : levels[0] || "medium" }); }}>{enabledProviders.map((item) => <option value={item.id} key={item.id}>{item.displayName}</option>)}</select>
      <select value={draft.modelId} onChange={(event) => { const model = provider?.models.find((item) => item.id === event.target.value); const levels = model?.reasoningLevels || []; setDraft({ ...draft, modelId: event.target.value, reasoning: levels.includes(draft.reasoning) ? draft.reasoning : levels.includes("high") ? "high" : levels[0] || "medium" }); }}>{provider?.models.filter((model) => model.enabled).map((model) => <option value={model.id} key={model.id}>{model.displayName}</option>)}</select>
      <select value={draft.reasoning} onChange={(event) => setDraft({ ...draft, reasoning: event.target.value })}>{reasoningLevels.map((item) => <option value={item} key={item}>{item}</option>)}</select>
      <button onClick={() => api.saveTaskDefault(draft).then(() => onSaved(`${taskLabels[draft.taskType]}默认模型已保存。`))}><Save size={15} /> 保存</button>
    </div>
  );
}

function findUrl(value: unknown): string | undefined {
  if (typeof value === "string" && /^https?:\/\//.test(value)) return value;
  if (Array.isArray(value)) { for (const item of value) { const result = findUrl(item); if (result) return result; } }
  if (value && typeof value === "object") { for (const item of Object.values(value)) { const result = findUrl(item); if (result) return result; } }
  return undefined;
}

function findKeyString(value: unknown, key: string): string | undefined {
  if (Array.isArray(value)) {
    for (const item of value) { const result = findKeyString(item, key); if (result) return result; }
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if (typeof record[key] === "string") return record[key] as string;
    for (const item of Object.values(record)) { const result = findKeyString(item, key); if (result) return result; }
  }
  return undefined;
}

function accountIsChatGpt(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(accountIsChatGpt);
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  if (record.authMode === "chatgpt" || record.type === "chatgpt" || record.type === "chatgpt_oauth") return true;
  return Object.values(record).some(accountIsChatGpt);
}
