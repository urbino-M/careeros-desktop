import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  CheckCircle2,
  Database,
  ExternalLink,
  KeyRound,
  LockKeyhole,
  Mail,
  PlugZap,
  Save,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import { ErrorState, LoadingState } from "../components/Ui";
import type { GmailStatus, MigrationReport, ProviderInfo, TaskModelDefault } from "../types";

const taskLabels: Record<string, string> = {
  full_search: "完整检索",
  research_pi: "按姓名研究 PI",
  material_revision: "材料修订",
  reply_followup: "回复处理",
  maintenance: "检查与维护",
};

export function SettingsPage() {
  const [providers, setProviders] = useState<ProviderInfo[]>();
  const [defaults, setDefaults] = useState<TaskModelDefault[]>();
  const [migration, setMigration] = useState<MigrationReport>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const load = () => Promise.all([api.providers(), api.taskDefaults(), api.migration()])
    .then(([p, d, m]) => { setProviders(p); setDefaults(d); setMigration(m); })
    .catch((value) => setError(errorMessage(value)));
  useEffect(() => { load(); }, []);

  if (error) return <div className="page"><ErrorState message={error} retry={load} /></div>;
  if (!providers || !defaults || !migration) return <div className="page"><LoadingState label="正在读取本机设置" /></div>;

  return (
    <div className="page settings-page">
      <header className="page-header">
        <div className="eyebrow">LOCAL-FIRST CONTROL</div>
        <h1>设置</h1>
        <p>账号凭据只进入 macOS Keychain；数据库、材料和 Codex 状态保存在系统标准应用目录。</p>
      </header>
      {notice && <div className="inline-notice">{notice}</div>}

      <SettingsSection index="01" title="OpenAI / Codex" icon={Bot} badge="V1 已启用">
        <div className="settings-card-grid">
          <CodexAccountCard onNotice={setNotice} />
          <ApiKeyCard onNotice={setNotice} />
        </div>
      </SettingsSection>

      <SettingsSection index="02" title="任务默认模型" icon={PlugZap} badge="可逐次覆盖">
        <div className="defaults-table">
          {defaults.map((value) => (
            <DefaultRow key={value.taskType} value={value} providers={providers} onSaved={(message) => { setNotice(message); load(); }} />
          ))}
        </div>
      </SettingsSection>

      <SettingsSection index="03" title="Gmail 草稿" icon={Mail} badge="只创建草稿">
        <GmailSettingsCard onNotice={setNotice} />
      </SettingsSection>

      <SettingsSection index="04" title="第三方模型" icon={KeyRound} badge="预留入口">
        <div className="provider-grid">
          {providers.filter((provider) => provider.id !== "openai").map((provider) => (
            <article className="provider-card disabled" key={provider.id}>
              <div className="provider-top"><h3>{provider.displayName}</h3><span>当前版本未启用</span></div>
              <p>{provider.connectionMode === "internal_gateway" ? "预留 PostdocOS 内置 Responses 协议转换入口。" : "预留连接 CC Switch 等外部本地路由。"}</p>
              <button disabled>配置连接</button>
            </article>
          ))}
        </div>
        <div className="settings-footnote">这些入口已具备数据库和 Provider Adapter 基础，但不能被任务误选；本轮不会实现或承诺 DeepSeek 调用。</div>
      </SettingsSection>

      <SettingsSection index="05" title="数据迁移与隐私" icon={Database} badge="旧库只读">
        <div className="migration-card">
          <div className="migration-summary"><ShieldCheck size={26} /><div><h3>schema v7 数据副本已校验</h3><p>旧数据库校验值不会因桌面迁移而变化；所有新表和状态修正只写入本机副本。</p></div></div>
          <div className="migration-stats">
            <span><strong>{migration.applications}</strong>申请</span>
            <span><strong>{migration.opportunities}</strong>机会</span>
            <span><strong>{migration.legacyJobs}</strong>任务</span>
            <span><strong>{migration.revisions}</strong>修订</span>
            <span><strong>{migration.gmailDrafts}</strong>草稿记录</span>
          </div>
          <details className="technical-details"><summary>迁移详情</summary><pre>{JSON.stringify(migration, null, 2)}</pre></details>
        </div>
      </SettingsSection>
    </div>
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
  const importLegacy = async () => {
    setBusy(true);
    try {
      const imported = await api.importLegacyGmail();
      onNotice(imported ? "已把旧版 Gmail 凭据导入 macOS Keychain；旧文件保持不变。" : "没有找到可导入的旧版 Gmail 凭据。");
      refresh();
    } catch (value) { onNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };
  const connect = async () => {
    setBusy(true);
    try {
      const result = await api.startGmailOAuth();
      await openUrl(result.authorizationUrl);
      onNotice("已在系统浏览器打开 Google 授权页。完成后直接回到 PostdocOS。");
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
          <p>只申请 Gmail Compose 权限；凭据进入 Keychain。PostdocOS 没有发送接口，创建后还会用 draft ID 从 Gmail 重新读取确认。</p>
          {status ? (
            <span className={status.connectionOk ? "connected-label" : "gmail-state"}>
              {status.connectionOk ? <><CheckCircle2 size={15} /> 已连接 {status.accountEmail}</> : status.oauthStatus === "pending" ? "等待浏览器授权…" : status.oauthMessage || (status.configured ? "客户端已配置，尚未连接" : "尚未配置")}
            </span>
          ) : <span className="gmail-state">正在检查…</span>}
        </div>
        <span className="reserved-chip">绝不发送</span>
      </div>
      <div className="gmail-action-row">
        <button className="button secondary" disabled={busy} onClick={chooseClient}><KeyRound size={16} /> 选择 OAuth 客户端 JSON</button>
        <button className="button secondary" disabled={busy} onClick={importLegacy}>导入旧版凭据</button>
        <button className="button primary" disabled={busy || !status?.configured} onClick={connect}><ExternalLink size={16} /> 连接 Gmail</button>
        <button className="button ghost" disabled={busy} onClick={refresh}>刷新状态</button>
      </div>
      <div className="oauth-guide"><strong>若 Google 显示 403：</strong>这不是等待时间问题。请在 Google Cloud 的 OAuth 同意屏幕中把 <code>urbinohbmiao@gmail.com</code> 加为测试用户，并确认客户端类型为“桌面应用”，然后再点连接。</div>
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
  useEffect(() => { void check(false); }, []);
  const connect = async () => {
    setChecking(true);
    try {
      const result = await api.connectChatGpt();
      const url = findUrl(result);
      const loginId = findKeyString(result, "loginId");
      if (!url || !loginId) throw new Error("Codex 没有返回完整的授权地址，请重试");
      await openUrl(url);
      setWaiting(true);
      onNotice("等待浏览器完成 ChatGPT 授权；成功后 PostdocOS 会自动识别并回到应用。");
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
        <p>使用系统浏览器登录；不在 PostdocOS 中输入 ChatGPT 密码。</p>
        {waiting && <span className="gmail-state">等待浏览器授权…</span>}
        {!waiting && connected && <span className="connected-label"><CheckCircle2 size={15} /> 已连接{accountEmail ? ` ${accountEmail}` : ""}</span>}
        {!waiting && status && !connected && <span className="gmail-state">尚未连接</span>}
        {status && <details className="technical-details"><summary>账号技术详情</summary><pre>{JSON.stringify(status, null, 2)}</pre></details>}
      </div>
      <div className="account-actions"><button className="button primary" disabled={checking || waiting} onClick={connect}><ExternalLink size={16} /> {waiting ? "等待授权" : "连接 ChatGPT"}</button><button className="button ghost" disabled={checking} onClick={() => void check()}>{checking ? "检查中…" : "检查状态"}</button></div>
    </article>
  );
}

function ApiKeyCard({ onNotice }: { onNotice: (value: string) => void }) {
  const [key, setKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => { api.hasOpenAiKey().then(setHasKey).catch(() => undefined); }, []);
  const save = async () => {
    setBusy(true);
    try { await api.saveOpenAiKey(key); setHasKey(true); setKey(""); onNotice("OpenAI API Key 已保存到 macOS Keychain。"); }
    catch (value) { onNotice(errorMessage(value)); }
    finally { setBusy(false); }
  };
  return (
    <article className="account-card">
      <div className="settings-icon"><LockKeyhole size={23} /></div>
      <div className="account-copy"><h3>OpenAI API Key</h3><p>密钥不写入 SQLite、配置文件或日志。</p>{hasKey && <span className="connected-label"><CheckCircle2 size={15} /> Keychain 已保存</span>}<input type="password" value={key} onChange={(event) => setKey(event.target.value)} placeholder="sk-…" /></div>
      <div className="account-actions"><button className="button secondary" disabled={busy || !key} onClick={save}><Save size={16} /> 保存密钥</button>{hasKey && <button className="button ghost danger" onClick={() => api.removeOpenAiKey().then(() => { setHasKey(false); onNotice("API Key 已从 Keychain 删除。"); })}>删除</button>}</div>
    </article>
  );
}

function DefaultRow({ value, providers, onSaved }: { value: TaskModelDefault; providers: ProviderInfo[]; onSaved: (message: string) => void }) {
  const [draft, setDraft] = useState(value);
  const provider = providers.find((item) => item.id === draft.providerId);
  const enabledProviders = providers.filter((item) => item.enabled);
  return (
    <div className="default-row">
      <strong>{taskLabels[value.taskType] || value.taskType}</strong>
      <select value={draft.providerId} onChange={(event) => { const next = enabledProviders.find((item) => item.id === event.target.value)!; setDraft({ ...draft, providerId: next.id, modelId: next.models.find((model) => model.enabled)?.id || "" }); }}>{enabledProviders.map((item) => <option value={item.id} key={item.id}>{item.displayName}</option>)}</select>
      <select value={draft.modelId} onChange={(event) => setDraft({ ...draft, modelId: event.target.value })}>{provider?.models.filter((model) => model.enabled).map((model) => <option value={model.id} key={model.id}>{model.displayName}</option>)}</select>
      <select value={draft.reasoning} onChange={(event) => setDraft({ ...draft, reasoning: event.target.value })}>{["low", "medium", "high", "xhigh"].map((item) => <option value={item} key={item}>{item}</option>)}</select>
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
