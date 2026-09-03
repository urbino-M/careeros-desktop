import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowRight,
  Check,
  CheckCircle2,
  CircleAlert,
  ExternalLink,
  GitBranch,
  KeyRound,
  Radar,
  RefreshCw,
  Save,
  ShieldCheck,
  Sparkles,
  Target,
  Wrench,
} from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import { searchChannelLabels } from "./Ui";
import type { AppRoute, ChannelHealth, InternshipProfile, SearchCapabilities, SearchSetupPlan } from "../types";

const emptyProfile: InternshipProfile = {
  schemaVersion: 1,
  targetRoles: "",
  industries: "",
  regions: "",
  workMode: "",
  startDate: "",
  duration: "",
  workAuthorization: "",
  enrollmentStatus: "",
  constraints: "",
  rssFeeds: [],
};

const searchTracks = ["方向 A", "方向 B", "方向 C", "方向 D", "方向 E", "方向 F", "方向 G"];

const contributions = [
  { id: "项目 01", label: "已核验贡献条目" },
  { id: "项目 02", label: "已核验贡献条目" },
  { id: "项目 03", label: "已核验贡献条目" },
  { id: "项目 04", label: "已核验贡献条目" },
];

const improvementTracks = [
  { index: "01", title: "岗位方向 A", state: "待设置", detail: "补充岗位要求对应的可复现实验。" },
  { index: "02", title: "岗位方向 B", state: "待设置", detail: "用一个可核验项目建立第一条能力证据。" },
  { index: "03", title: "岗位方向 C", state: "待设置", detail: "围绕目标岗位做一个可合并的工程贡献。" },
];

const textFields: Array<{
  key: "targetRoles" | "industries" | "regions" | "workMode" | "startDate" | "duration" | "workAuthorization" | "enrollmentStatus" | "constraints";
  label: string;
  placeholder: string;
  multiline?: boolean;
}> = [
  { key: "targetRoles", label: "目标岗位 / 技能", placeholder: "例如 ML Engineer Intern、Research Engineer；可用逗号或换行分隔" },
  { key: "industries", label: "目标行业", placeholder: "例如 AI、金融科技、医疗、开发者工具" },
  { key: "regions", label: "目标地区", placeholder: "国家、城市、时区，或 remote / hybrid / 不限" },
  { key: "workMode", label: "工作方式", placeholder: "例如 remote、hybrid、onsite，或不限" },
  { key: "startDate", label: "开始时间", placeholder: "例如 2027-05、暑期、可协商" },
  { key: "duration", label: "实习时长", placeholder: "例如 12 周、3–6 个月、可协商" },
  { key: "workAuthorization", label: "工作许可", placeholder: "例如需要 sponsorship、已有工作许可、待确认" },
  { key: "enrollmentStatus", label: "在读状态", placeholder: "例如在读本科、硕士、博士或应届毕业" },
  { key: "constraints", label: "限制条件", placeholder: "填写必须尊重的时间、签证、薪资、课程或其他边界", multiline: true },
];

export function InternshipPlanningSummary({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  const [profile, setProfile] = useState<InternshipProfile>();

  useEffect(() => {
    let active = true;
    void api.internshipProfile().then((value) => {
      if (active) setProfile(value);
    }).catch(() => undefined);
    return () => { active = false; };
  }, []);

  const preferences = profilePreferences(profile);
  return (
    <section className="internship-context-strip" aria-label="当前 Internship 求职主线">
      <div className="internship-context-title">
        <Radar size={19} />
        <div><span>当前求职主线</span><strong>{profile?.targetRoles || "待设置"}</strong></div>
      </div>
      <div className="internship-context-profile">
        {preferences.map((item) => (
          <div className="internship-context-item" key={item.label}>
            <span>{item.label}</span>
            <strong>{item.value}</strong>
          </div>
        ))}
      </div>
      <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status: "all", view: "strategy" })}>
        查看求职策略 <ArrowRight size={15} />
      </button>
    </section>
  );
}

export function InternshipPlanningPanel({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  const [profile, setProfile] = useState<InternshipProfile>(emptyProfile);
  const [capabilities, setCapabilities] = useState<SearchCapabilities>();
  const [setupPlan, setSetupPlan] = useState<SearchSetupPlan>();
  const [authGuide, setAuthGuide] = useState<{ title: string; url?: string; instructions: string[] }>();
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");

  const load = async () => {
    setBusy(true);
    setNotice("");
    try {
      const [nextProfile, nextCapabilities] = await Promise.all([api.internshipProfile(), api.searchCapabilities()]);
      setProfile(nextProfile);
      setCapabilities(nextCapabilities);
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => { void load(); }, []);

  const patch = (value: Partial<InternshipProfile>) => setProfile((current) => ({ ...current, ...value }));

  const saveProfile = async () => {
    setBusy(true);
    setNotice("");
    try {
      const saved = await api.saveInternshipProfile(profile);
      setProfile(saved);
      setNotice("Internship 画像已保存；它与 Postdoc 资料独立。下次搜索会使用最新画像。")
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  const previewSetup = async () => {
    setBusy(true);
    setNotice("");
    try {
      setSetupPlan(await api.previewSearchSetup());
      setNotice("已生成安装 dry-run；确认后才会执行用户级安装或配置。")
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  const executeSetup = async () => {
    if (!setupPlan) return previewSetup();
    const summary = [...setupPlan.commands, ...setupPlan.manualSteps].join("\n");
    if (!window.confirm(`即将执行以下用户级搜索渠道设置：\n\n${summary || "无需安装；仅刷新渠道状态。"}\n\n不使用 sudo，也不会写入项目目录。继续吗？`)) return;
    setBusy(true);
    setNotice("");
    try {
      const result = await api.setupSearchCapabilities(setupPlan.channels, true);
      setCapabilities(result.capabilities);
      setNotice(result.messages.join("\n"));
      setSetupPlan(undefined);
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  const beginAuth = async (channel: ChannelHealth["channel"]) => {
    setBusy(true);
    setNotice("");
    try {
      const guide = await api.beginSearchChannelAuth(channel);
      setAuthGuide(guide);
      if (guide.url) await openUrl(guide.url);
      setNotice(`${searchChannelLabels[channel]} 登录引导已打开；完成后回到这里刷新状态。`);
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  const profilePreferencesValue = profilePreferences(profile);
  return (
    <section className="internship-planning" aria-label="Internship 求职策略">
      <header className="internship-planning-header">
        <div>
          <div className="eyebrow">CURRENT TRACK</div>
          <h2>求职策略</h2>
          <p>先定义独立的 Internship 筛选画像，再并行探索官方 Web / ATS、Exa、RSS、LinkedIn、Facebook 和 Twitter / X。</p>
        </div>
        <div className="internship-search-profile" aria-label="Internship 求职筛选画像">
          {profilePreferencesValue.map((item) => (
            <div className="internship-search-profile-item" key={item.label}>
              <span>{item.label}</span>
              <strong>{item.value}</strong>
            </div>
          ))}
        </div>
      </header>

      <section className="internship-profile-editor" aria-label="Internship 搜索画像设置">
        <div className="planning-section-heading">
          <div><span className="planning-index">SEARCH PROFILE</span><h3>独立的 Internship 画像</h3></div>
          <span className="planning-section-note">资料不完整也可以搜索；缺失字段只会让资格判断变为 uncertain。</span>
        </div>
        <div className="internship-profile-fields">
          {textFields.map((field) => (
            <label className={field.multiline ? "profile-field profile-field-wide" : "profile-field"} key={field.key}>
              <span>{field.label}</span>
              {field.multiline ? (
                <textarea value={profile[field.key]} onChange={(event) => patch({ [field.key]: event.target.value } as Partial<InternshipProfile>)} placeholder={field.placeholder} />
              ) : (
                <input value={profile[field.key]} onChange={(event) => patch({ [field.key]: event.target.value } as Partial<InternshipProfile>)} placeholder={field.placeholder} />
              )}
            </label>
          ))}
          <label className="profile-field">
            <span>可选 CV（profile 内相对路径）</span>
            <input value={profile.cvPath ?? ""} onChange={(event) => patch({ cvPath: event.target.value || undefined })} placeholder="例如 internship-cv.pdf；不会读取 Postdoc master_profile" />
          </label>
          <label className="profile-field profile-field-wide">
            <span>RSS / Atom 地址（每行一个）</span>
            <textarea value={profile.rssFeeds.join("\n")} onChange={(event) => patch({ rssFeeds: event.target.value.split(/\r?\n/).map((value) => value.trim()).filter(Boolean) })} placeholder="https://example.com/internships.xml" />
          </label>
        </div>
        <div className="planning-editor-actions">
          <button className="button primary" disabled={busy} onClick={() => void saveProfile()}><Save size={16} /> 保存 Internship 画像</button>
          <span className="planning-section-note">最后更新：{profile.updatedAt ? new Date(profile.updatedAt).toLocaleString() : "尚未保存"}</span>
        </div>
      </section>

      <ChannelHealthPanel
        capabilities={capabilities}
        setupPlan={setupPlan}
        busy={busy}
        authGuide={authGuide}
        onRefresh={() => void load()}
        onPreview={() => void previewSetup()}
        onSetup={() => void executeSetup()}
        onAuth={(channel) => void beginAuth(channel)}
      />

      {notice && <div className="planning-notice"><CircleAlert size={17} /><span>{notice}</span></div>}

      <div className="internship-planning-grid">
        <article className="planning-card planning-radar-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><Radar size={21} /></div>
            <div><span className="planning-index">01 · 机会雷达</span><h3>覆盖目标岗位</h3></div>
          </div>
          <div className="planning-summary"><span>检索配置</span><strong>{profile.targetRoles || "待设置"}</strong></div>
          <div className="planning-source-row"><span>官方 Web / ATS</span><span>Exa</span><span>RSS</span><span>社交渠道</span></div>
          <div className="planning-track-list">
            {(profile.targetRoles ? profile.targetRoles.split(/[\n,，]/).map((value) => value.trim()).filter(Boolean) : searchTracks).slice(0, 7).map((track, index) => <span key={track} className={index < 3 ? "priority" : ""}>{track}</span>)}
          </div>
          <button className="button primary wide" onClick={() => onNavigate({ page: "automation" })}><Radar size={16} /> 在 Agent 中启动全面扫描 <ArrowRight size={16} /></button>
        </article>

        <article className="planning-card planning-resume-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><GitBranch size={21} /></div>
            <div><span className="planning-index">02 · 主简历证据</span><h3>一份母版，持续积累</h3></div>
          </div>
          <div className="evidence-count"><strong>—</strong><span>等待导入已核验贡献</span></div>
          <div className="evidence-list">
            {contributions.map((item) => <div className="evidence-row" key={item.id}><CheckCircle2 size={15} /><strong>{item.id}</strong><span>{item.label}</span></div>)}
          </div>
          <div className="planning-note"><ShieldCheck size={16} /> 只把你确认的贡献写入主简历；不会把 fork 的全部代码当成个人经历。</div>
        </article>

        <article className="planning-card planning-improvement-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><Target size={21} /></div>
            <div><span className="planning-index">03 · 岗位针对性提升</span><h3>把缺口变成贡献</h3></div>
          </div>
          <div className="improvement-list">
            {improvementTracks.map((item) => <div className="improvement-row" key={item.index}>
              <span className="improvement-index">{item.index}</span>
              <div><div className="improvement-title"><strong>{item.title}</strong><span>{item.state}</span></div><p>{item.detail}</p></div>
            </div>)}
          </div>
          <button className="text-button planning-action" onClick={() => onNavigate({ page: "automation" })}><Sparkles size={15} /> 让 Agent 拆解下一步 <ArrowRight size={15} /></button>
        </article>
      </div>
    </section>
  );
}

function ChannelHealthPanel({
  capabilities,
  setupPlan,
  busy,
  authGuide,
  onRefresh,
  onPreview,
  onSetup,
  onAuth,
}: {
  capabilities?: SearchCapabilities;
  setupPlan?: SearchSetupPlan;
  busy: boolean;
  authGuide?: { title: string; url?: string; instructions: string[] };
  onRefresh: () => void;
  onPreview: () => void;
  onSetup: () => void;
  onAuth: (channel: ChannelHealth["channel"]) => void;
}) {
  return (
    <section className="channel-health-panel" aria-label="搜索渠道健康状态">
      <div className="planning-section-heading">
        <div><span className="planning-index">CHANNEL DOCTOR</span><h3>渠道健康与登录</h3></div>
        <div className="channel-health-actions">
          <button className="button ghost" disabled={busy} onClick={onRefresh}><RefreshCw size={15} /> 刷新状态</button>
          <button className="button secondary" disabled={busy} onClick={onPreview}><Wrench size={15} /> 预览安装 dry-run</button>
          {setupPlan && <button className="button primary" disabled={busy} onClick={onSetup}><Check size={15} /> 确认执行设置</button>}
        </div>
      </div>
      <p className="planning-section-note">渠道独立检查、并行执行；单个渠道不可用不会阻断其他来源。安装只写入当前用户工具与配置目录，不使用 sudo。</p>
      <div className="channel-health-grid">
        {(capabilities?.channels ?? []).map((health) => {
          const needsAuth = ["facebook", "linkedin", "twitter"].includes(health.channel);
          const state = !health.available ? "需安装" : needsAuth && !health.authenticated ? "需登录" : "可用";
          return (
            <article className={`channel-health-item channel-${health.available ? "available" : "missing"}`} key={health.channel}>
              <div className="channel-health-topline"><strong>{searchChannelLabels[health.channel]}</strong><span className={`channel-state channel-state-${health.available ? "ready" : "missing"}`}>{state}</span></div>
              <span className="channel-backend">{health.backend}</span>
              <p>{health.message}</p>
              <div className="channel-health-footer"><small>检查于 {formatDate(health.checkedAt)}</small>{needsAuth && <button className="text-button" disabled={busy} onClick={() => onAuth(health.channel)}><KeyRound size={14} /> 登录引导</button>}</div>
            </article>
          );
        })}
      </div>
      {capabilities?.channels.length === 0 && <div className="planning-section-note">正在读取渠道状态…</div>}
      {setupPlan && <div className="setup-plan">
        <strong>安装预览</strong>
        {setupPlan.commands.length > 0 ? <div className="setup-command-list">{setupPlan.commands.map((command) => <code key={command}>{command}</code>)}</div> : <span>没有需要执行的安装命令。</span>}
        {setupPlan.manualSteps.map((step) => <span key={step}>· {step}</span>)}
      </div>}
      {authGuide && <div className="auth-guide"><div><strong>{authGuide.title}</strong>{authGuide.instructions.map((instruction) => <span key={instruction}>· {instruction}</span>)}</div>{authGuide.url && <button className="button ghost" onClick={() => void openUrl(authGuide.url!)}><ExternalLink size={14} /> 再次打开</button>}</div>}
    </section>
  );
}

function profilePreferences(profile?: InternshipProfile) {
  return [
    { label: "地点", value: profile?.regions || "待设置" },
    { label: "工作方式", value: profile?.workMode || "待设置" },
    { label: "实习时长", value: profile?.duration || "待设置" },
    { label: "开始时间", value: profile?.startDate || "待设置" },
  ];
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
