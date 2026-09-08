import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  CheckCircle2,
  ExternalLink,
  FileText,
  KeyRound,
  Mail,
  PlugZap,
  RefreshCw,
  Save,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import { ErrorState, LoadingState, uiNotice, useUiNotice, type UiNotice } from "../components/Ui";
import { modelDisplayLabel, reasoningLabel } from "../components/ModelControls";
import type { CvCustomizationSettings, GmailStatus, ProviderInfo, TaskModelDefault } from "../types";
import { useDesktopUpdates } from "../updates/UpdateManager";
import { InterfacePreferences } from "../components/InterfacePreferences";
import { t } from "../i18n";

const taskLabels: Record<string, string> = {
  full_search: "完整检索",
  research_pi: "按姓名研究联系人",
  material_revision: "材料修订",
  reply_followup: "回复处理",
  maintenance: "检查与维护",
};

export function SettingsPage({ onRestartOnboarding }: { onRestartOnboarding: () => Promise<void> }) {
  const [providers, setProviders] = useState<ProviderInfo[]>();
  const [defaults, setDefaults] = useState<TaskModelDefault[]>();
  const [cvCustomization, setCvCustomization] = useState<CvCustomizationSettings>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useUiNotice();
  const load = () => Promise.all([api.providers(), api.taskDefaults(), api.cvCustomization()])
    .then(([p, d, cv]) => { setProviders(p); setDefaults(d); setCvCustomization(cv); })
    .catch((value) => setError(errorMessage(value)));
  useEffect(() => { load(); }, []);

  if (error) return <div className="page"><ErrorState message={error} retry={load} /></div>;
  if (!providers || !defaults || !cvCustomization) return <div className="page"><LoadingState label={t("正在读取本机设置")} /></div>;

  return (
    <div className="page settings-page">
      <header className="page-header">
        <div className="eyebrow">{t("本机设置")}</div>
        <h1>{t("设置")}</h1>
        <p>{t("账号凭据保存在当前用户的 CareerOS 私有凭据文件中；数据库、材料和 Codex 状态保存在系统标准应用目录。")}</p>
        <button className="button ghost settings-onboarding-button" onClick={() => void onRestartOnboarding().catch((value) => setNotice(errorMessage(value)))}>{t("上传 / 更换 CV 与个人偏好")}</button>
      </header>
      {notice && <div className="inline-notice">{notice}</div>}

      <section className="interface-settings panel" aria-label={t("界面偏好")}>
        <div><h2>{t("界面偏好")}</h2><p>{t("立即生效，自动保存在本机；不改变 CV、邮件或任务指令的语言。")}</p></div>
        <InterfacePreferences />
      </section>

      <SettingsSection index="01" title="OpenAI / Codex" icon={Bot} badge={t("V1 已启用")}>
        <CodexAccountCard onNotice={setNotice} />
      </SettingsSection>

      <SettingsSection index="02" title={t("任务默认模型")} icon={PlugZap} badge={t("可逐次覆盖")}>
        <div className="defaults-table">
          {defaults.map((value) => (
            <DefaultRow key={value.taskType} value={value} providers={providers} onSaved={(message) => { setNotice(message); load(); }} />
          ))}
        </div>
      </SettingsSection>

      <SettingsSection index="03" title={t("CV 定制")} icon={FileText} badge={t("Agent 自动遵从")}>
        <CvCustomizationCard
          value={cvCustomization}
          onSaved={(saved) => {
            setCvCustomization(saved);
            setNotice(saved.enabled ? "CV 定制规则与页数已保存，后续 Agent 任务会自动遵从。" : "CV 页数已保存；自由定制项当前未启用。");
          }}
          onError={(value) => setNotice(errorMessage(value))}
        />
      </SettingsSection>

      <SettingsSection index="04" title={t("Gmail 草稿")} icon={Mail} badge={t("只创建草稿")}>
        <GmailSettingsCard onNotice={setNotice} />
      </SettingsSection>

      <SettingsSection index="05" title={t("模型中转站 / DeepSeek")} icon={KeyRound} badge={t("Responses 直连")}>
        <ProviderConnectionSettings
          providers={providers.filter((provider) => provider.id !== "openai")}
          onChanged={(message) => { setNotice(message); load(); }}
        />
      </SettingsSection>

      <SettingsSection index="06" title={t("应用更新")} icon={RefreshCw} badge={t("签名校验")}>
        <ApplicationUpdateCard />
      </SettingsSection>

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
        <p>{t("启动后自动检查 GitHub Release，之后每小时检查一次。下载的更新包必须通过内置公钥验签，失败时不会安装。")}</p>
        <span className={updates.phase === "error" ? "gmail-state" : "connected-label"}><ShieldCheck size={15} /> {updates.message}</span>
        <small>{t("上次检查：")}{updates.lastCheckedLabel}</small>
      </div>
      <button className="button secondary" disabled={busy} onClick={() => void updates.checkNow()}><RefreshCw size={16} className={busy ? "spinning" : ""} /> {updates.phase === "checking" ? t("检查中…") : t("检查更新")}</button>
    </article>
  );
}

export function CvCustomizationCard({ value, onSaved, onError }: { value: CvCustomizationSettings; onSaved: (value: CvCustomizationSettings) => void; onError: (value: unknown) => void }) {
  const [draft, setDraft] = useState(value);
  const [busy, setBusy] = useState(false);
  useEffect(() => setDraft(value), [value]);
  const save = async () => {
    setBusy(true);
    try { onSaved(await api.saveCvCustomization(draft)); }
    catch (value) { onError(value); }
    finally { setBusy(false); }
  };
  const setPageMode = (mode: "auto" | "fixed") => setDraft((current) => ({
    ...current,
    pageCount: mode === "fixed"
      ? { mode, value: current.pageCount.value ?? 2 }
      : { mode },
  }));
  return (
    <article className="cv-customization-card">
      <div className="cv-customization-intro">
        <div><h3>{t("统一控制所有目标 CV 的呈现方式")}</h3><p>{t("默认按原 CV、学科和目标岗位选择内容。推荐人有就保留、没有就省略，也可以在定制要求中调整；不会编造经历或联系方式。")}</p></div>
        <label className="cv-customization-toggle"><input type="checkbox" checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} /><span>{draft.enabled ? t("已启用") : t("未启用")}</span></label>
      </div>
      <div className="cv-page-count-control">
        <label><span>{t("CV 页数")}</span><select value={draft.pageCount.mode} onChange={(event) => setPageMode(event.target.value as "auto" | "fixed")}><option value="auto">{t("自动选择")}</option><option value="fixed">{t("指定页数")}</option></select></label>
        {draft.pageCount.mode === "fixed" && <label><span>{t("指定为")}</span><input type="number" min={1} max={20} value={draft.pageCount.value ?? 2} onChange={(event) => setDraft({ ...draft, pageCount: { mode: "fixed", value: Number(event.target.value) } })} /><small>{t("页（支持 1、2、3…）")}</small></label>}
        <p>{t("章节、顺序和条目数量按原 CV 与修改要求调整，不锁定数量；新增事实必须有来源。")}</p>
      </div>
      <div className="cv-customization-grid">
        <label><span>{t("重点强调")}</span><textarea value={draft.emphasize} onChange={(event) => setDraft({ ...draft, emphasize: event.target.value })} placeholder={t("填写希望优先呈现的经历、方法、成果类型或能力。")} /></label>
        <label><span>{t("需要弱化或简写")}</span><textarea value={draft.exclude} onChange={(event) => setDraft({ ...draft, exclude: event.target.value })} placeholder={t("填写需要省略、简写或避免重复表述的内容，也可调整推荐人展示。")} /></label>
        <label className="wide"><span>{t("其他定制要求")}</span><textarea value={draft.instructions} onChange={(event) => setDraft({ ...draft, instructions: event.target.value })} placeholder={t("填写语气、语言、信息详略或针对不同机会的表达要求。")} /></label>
      </div>
      <div className="cv-customization-actions"><span>{t("页数始终生效；其他内容只作为定制指令，不会被当作个人事实或研究证据。")}</span><button className="button primary" disabled={busy} onClick={() => void save()}><Save size={16} /> {busy ? t("保存中…") : t("保存 CV 定制")}</button></div>
    </article>
  );
}

function GmailSettingsCard({ onNotice }: { onNotice: (value: UiNotice) => void }) {
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
          <h3>{t("系统浏览器 OAuth 与远端草稿核验")}</h3>
          <p>{t("只申请 Gmail Compose 权限；凭据保存在当前用户的 CareerOS 私有文件中。CareerOS 没有发送接口，创建后还会用 draft ID 从 Gmail 重新读取确认。")}</p>
          {status ? (
            <span className={status.connectionOk ? "connected-label" : "gmail-state"}>
              {status.connectionOk ? <><CheckCircle2 size={15} /> {t(" 已连接 ")}{status.accountEmail}</> : status.oauthStatus === "pending" ? t("等待浏览器授权…") : status.oauthMessage ? t(status.oauthMessage) : (status.configured ? t("客户端已配置，尚未连接") : t("尚未配置"))}
            </span>
          ) : <span className="gmail-state">{t("正在检查…")}</span>}
        </div>
        <span className="reserved-chip">{t("绝不发送")}</span>
      </div>
      <div className="gmail-action-row">
        <button className="button ghost" onClick={() => openUrl("https://developers.google.com/workspace/gmail/api/quickstart/nodejs")}><ExternalLink size={16} /> {t(" Google 官方 JSON 创建教程")}</button>
        <button className="button secondary" disabled={busy} onClick={chooseClient}><KeyRound size={16} /> {t(" 选择 OAuth 客户端 JSON")}</button>
        <button className="button primary" disabled={busy || !status?.configured} onClick={connect}><ExternalLink size={16} /> {t(" 连接 Gmail")}</button>
        <button className="button ghost" disabled={busy} onClick={refresh}>{t("刷新状态")}</button>
      </div>
      <div className="settings-footnote">{t("请选择“桌面应用”类型的 OAuth 客户端 JSON。若 OAuth 应用仍处于测试模式，请在 Google Cloud 中把准备连接的账号加入测试用户。")}</div>
    </div>
  );
}

function SettingsSection({ index, title, icon: Icon, badge, children }: { index: string; title: string; icon: typeof Bot; badge: string; children: React.ReactNode }) {
  return (
    <section className="settings-section">
      <div className="section-heading"><div><span className="section-index">{index}</span><h2><Icon size={20} /> {title}</h2></div><span>{badge}</span></div>
      {children}
    </section>
  );
}

function CodexAccountCard({ onNotice }: { onNotice: (value: UiNotice) => void }) {
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
        <p>{t("使用系统浏览器登录；不在 CareerOS 中输入 ChatGPT 密码。")}</p>
        {waiting && <span className="gmail-state">{t("等待浏览器授权…")}</span>}
        {!waiting && connected && <span className="connected-label"><CheckCircle2 size={15} /> {t(" 已连接")}{accountEmail ? ` ${accountEmail}` : ""}</span>}
        {!waiting && status && !connected && <span className="gmail-state">{t("尚未连接")}</span>}
        {!waiting && !status && <span className="gmail-state">{t("按需检查，不会在进入设置时启动 Codex")}</span>}
        {status && <details className="technical-details"><summary>{t("账号技术详情")}</summary><pre>{JSON.stringify(status, null, 2)}</pre></details>}
      </div>
      <div className="account-actions"><button className="button primary" disabled={checking || waiting} onClick={connect}><ExternalLink size={16} /> {waiting ? t("等待授权") : t("连接 ChatGPT")}</button><button className="button ghost" disabled={checking} onClick={() => void check()}>{checking ? t("检查中…") : t("检查状态")}</button></div>
    </article>
  );
}

export function providerValidationLabel(message?: string): string {
  if (!message) return t("尚未完成 Responses 校验");
  // This sentence is emitted by providers.rs. Keep the discovered model name verbatim.
  const verified = /^Responses 已验证；发现 (\d+) 个可用模型（探测模型：([\s\S]+)）$/.exec(message);
  if (verified) return t("Responses 已验证；发现 {0} 个可用模型（探测模型：{1}）",verified[1],verified[2]);
  return message === "已断开" || message === "Responses 已验证" ? t(message) : message;
}

export function ProviderConnectionSettings({ providers, onChanged }: { providers: ProviderInfo[]; onChanged: (message: UiNotice) => void }) {
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);
  const connect = async () => {
    setBusy(true);
    try {
      const provider = await api.connectResponsesProvider({ baseUrl, apiKey });
      setApiKey("");
      onChanged(uiNotice("{0} 已连接；发现 {1} 个可用模型。", provider.displayName, provider.models.length));
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
      onChanged(uiNotice("{0} 已断开，API Key 已从 CareerOS 凭据文件删除。", provider.displayName));
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
          <div><h3>{t("用 URL + API Key 接入")}</h3><p>{t("自动读取 ")}<code>/models</code> {t(" 并做一次最小 ")}<code>/responses</code> {t(" 兼容性探测。密钥保存在当前用户的 CareerOS 私有凭据文件中。")}</p></div>
          <button className="button ghost" disabled={busy} onClick={() => setBaseUrl("https://api.deepseek.com")}>{t("使用 DeepSeek 官方地址")}</button>
        </div>
        <div className="provider-connect-fields">
          <label><span>{t("服务地址")}</span><input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://relay.example/v1" /></label>
          <label><span>{t("API 密钥")}</span><input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={t("只保存到 CareerOS 私有凭据文件")} /></label>
          <button className="button primary" disabled={busy || !baseUrl.trim() || apiKey.trim().length < 8} onClick={connect}><PlugZap size={16} /> {busy ? t("正在验证…") : t("验证并连接")}</button>
        </div>
        <div className="settings-footnote">{t("当前直连只接受 OpenAI Responses 兼容服务。若地址只有 Chat Completions，连接时会明确拦截；本次探测会产生极少量模型 token。")}</div>
      </article>
      {providers.length > 0 && <div className="provider-grid">
        {providers.map((provider) => (
          <article className={`provider-card${provider.enabled ? "" : " disabled"}`} key={provider.id}>
            <div className="provider-top"><h3>{provider.displayName}</h3><span>{provider.enabled ? t("已连接") : t("已断开")}</span></div>
            <p className="provider-url">{provider.baseUrl}</p>
            <p>{providerValidationLabel(provider.validationMessage)}</p>
            <div className="provider-model-chips">{provider.models.filter((model) => model.enabled).slice(0, 8).map((model) => <span key={model.id}>{modelDisplayLabel(provider.id,model.displayName)}</span>)}</div>
            {provider.enabled && <button className="button ghost danger" disabled={busy} onClick={() => void disconnect(provider)}>{t("断开并删除密钥")}</button>}
          </article>
        ))}
      </div>}
    </div>
  );
}

export function DefaultRow({ value, providers, onSaved }: { value: TaskModelDefault; providers: ProviderInfo[]; onSaved: (message: UiNotice) => void }) {
  const [draft, setDraft] = useState(value);
  const provider = providers.find((item) => item.id === draft.providerId);
  const enabledProviders = providers.filter((item) => item.enabled);
  const selectedModel = provider?.models.find((model) => model.id === draft.modelId);
  const reasoningLevels = selectedModel?.reasoningLevels.length ? selectedModel.reasoningLevels : ["low", "medium", "high"];
  return (
    <div className="default-row">
      <strong>{t(taskLabels[value.taskType] || value.taskType)}</strong>
      <select aria-label={t("Agent 服务")} value={draft.providerId} onChange={(event) => { const next = enabledProviders.find((item) => item.id === event.target.value)!; const model = next.models.find((item) => item.enabled); const levels = model?.reasoningLevels || []; setDraft({ ...draft, providerId: next.id, modelId: model?.id || "", reasoning: levels.includes("high") ? "high" : levels[0] || "medium" }); }}>{enabledProviders.map((item) => <option value={item.id} key={item.id}>{item.displayName}</option>)}</select>
      <select aria-label={t("模型")} value={draft.modelId} onChange={(event) => { const model = provider?.models.find((item) => item.id === event.target.value); const levels = model?.reasoningLevels || []; setDraft({ ...draft, modelId: event.target.value, reasoning: levels.includes(draft.reasoning) ? draft.reasoning : levels.includes("high") ? "high" : levels[0] || "medium" }); }}>{provider?.models.filter((model) => model.enabled).map((model) => <option value={model.id} key={model.id}>{modelDisplayLabel(draft.providerId,model.displayName)}</option>)}</select>
      <select aria-label={t("推理强度")} value={draft.reasoning} onChange={(event) => setDraft({ ...draft, reasoning: event.target.value })}>{reasoningLevels.map((item) => <option value={item} key={item}>{reasoningLabel(item)}</option>)}</select>
      <button onClick={() => api.saveTaskDefault(draft).then(() => onSaved(uiNotice("{0}默认模型已保存。", () => t(taskLabels[draft.taskType] || draft.taskType))))}><Save size={15} /> {t(" 保存")}</button>
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
