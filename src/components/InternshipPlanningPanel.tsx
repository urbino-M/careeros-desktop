import { t } from "../i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowRight,
  CheckCircle2,
  CircleAlert,
  FileUp,
  GitBranch,
  Radar,
  Save,
  ShieldCheck,
  Sparkles,
  Target,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import type { AppRoute, InternshipProfile } from "../types";

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
    <section className="internship-context-strip" aria-label={t("当前 Internship 求职主线")}>
      <div className="internship-context-title">
        <Radar size={19} />
        <div><span>{t("当前求职主线")}</span><strong>{profile?.targetRoles || t("待设置")}</strong></div>
      </div>
      <div className="internship-context-profile">
        {preferences.map((item) => (
          <div className="internship-context-item" key={item.label}>
            <span>{t(item.label)}</span>
            <strong>{item.value}</strong>
          </div>
        ))}
      </div>
      <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status: "all", view: "strategy" })}>
        {t("查看求职策略")}<ArrowRight size={15} />
      </button>
    </section>
  );
}

export function InternshipPlanningPanel({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  const [profile, setProfile] = useState<InternshipProfile>(emptyProfile);
  const [cvFileName, setCvFileName] = useState("");
  const [cvDragActive, setCvDragActive] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const cvDropZone = useRef<HTMLDivElement>(null);

  const load = async () => {
    setBusy(true);
    setNotice("");
    try {
      const nextProfile = await api.internshipProfile();
      setProfile(nextProfile);
      setCvFileName(nextProfile.cvPath ? "已导入 CV" : "");
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => { void load(); }, []);

  const importCv = useCallback(async (path: string, displayName?: string) => {
    if (!path) return;
    setBusy(true);
    setNotice("");
    try {
      const cvPath = await api.importInternshipCv(path);
      setProfile((current) => ({ ...current, cvPath }));
      setCvFileName(displayName || "已导入 CV");
      setNotice("CV 已复制到本机资料目录；原文件未改动。点击“保存 Internship 画像”后会用于搜索。")
    } catch (value) {
      setNotice(errorMessage(value));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    const dropZone = cvDropZone.current;
    let unlisten: (() => void) | undefined;
    let active = true;
    const register = async () => {
      try {
        unlisten = await getCurrentWindow().onDragDropEvent((event) => {
          if (!active) return;
          const payload = event.payload;
          if (payload.type === "leave") {
            setCvDragActive(false);
            return;
          }
          if (payload.type === "enter" || payload.type === "over") {
            setCvDragActive(Boolean(dropZone && isDropInside(payload.position, dropZone.getBoundingClientRect())));
            return;
          }
          const inside = Boolean(dropZone && isDropInside(payload.position, dropZone.getBoundingClientRect()));
          setCvDragActive(false);
          if (inside && payload.paths.length > 0) {
            void importCv(payload.paths[0], fileNameFromPath(payload.paths[0]));
          }
        });
        if (!active) unlisten();
      } catch {
        // The browser preview has no Tauri file-drop event; its HTML5 fallback remains available.
      }
    };
    void register();
    return () => { active = false; unlisten?.(); };
  }, [importCv]);

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

  const chooseCv = async () => {
    const selected = await openDialog({ multiple: false, directory: false, filters: [{ name: "CV", extensions: ["pdf", "docx", "md", "txt"] }] });
    if (!selected || Array.isArray(selected)) return;
    void importCv(selected, fileNameFromPath(selected));
  };

  const removeCv = () => {
    setProfile((current) => ({ ...current, cvPath: undefined }));
    setCvFileName("");
    setNotice("CV 已从当前画像移除；保存后生效，本机副本会保留。")
  };

  const handleCvDrop = (event: React.DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setCvDragActive(false);
    const file = event.dataTransfer.files[0];
    const path = (file as (File & { path?: string }) | undefined)?.path;
    if (path) void importCv(path, file.name);
    else if (file) setNotice("当前环境无法读取拖入文件的本地路径，请点击“选择 CV 文件”完成导入。")
  };

  const profilePreferencesValue = profilePreferences(profile);
  return (
    <section className="internship-planning" aria-label={t("Internship 求职策略")}>
      <header className="internship-planning-header">
        <div>
          <div className="eyebrow">{t("当前轨道")}</div>
          <h2>{t("求职策略")}</h2>
          <p>{t("先定义统一的求职筛选画像，再用多来源检索保证机会覆盖，把岗位要求映射到你的主简历和下一项工程贡献。")}</p>
        </div>
        <div className="internship-search-profile" aria-label={t("Internship 求职筛选画像")}>
          {profilePreferencesValue.map((item) => (
            <div className="internship-search-profile-item" key={item.label}>
              <span>{t(item.label)}</span>
              <strong>{item.value}</strong>
            </div>
          ))}
        </div>
      </header>

      <section className="internship-profile-editor" aria-label={t("Internship 搜索画像设置")}>
        <div className="planning-section-heading">
          <div><span className="planning-index">SEARCH PROFILE</span><h3>{t("独立的 Internship 画像")}</h3></div>
          <span className="planning-section-note">{t("资料不完整也可以搜索；缺失字段只会让资格判断变为 uncertain。")}</span>
        </div>
        <div className="internship-profile-fields">
          {textFields.map((field) => (
            <label className={field.multiline ? "profile-field profile-field-wide" : "profile-field"} key={field.key}>
              <span>{t(field.label)}</span>
              {field.multiline ? (
                <textarea value={profile[field.key]} onChange={(event) => patch({ [field.key]: event.target.value } as Partial<InternshipProfile>)} disabled={busy} placeholder={t(field.placeholder)} />
              ) : (
                <input value={profile[field.key]} onChange={(event) => patch({ [field.key]: event.target.value } as Partial<InternshipProfile>)} disabled={busy} placeholder={t(field.placeholder)} />
              )}
            </label>
          ))}
          <div className="profile-field profile-field-wide">
            <span>{t("可选 CV")}</span>
            <div
              ref={cvDropZone}
              className={`internship-cv-dropzone${cvDragActive ? " is-dragging" : ""}`}
              onDragOver={(event) => { event.preventDefault(); setCvDragActive(true); }}
              onDragLeave={() => setCvDragActive(false)}
              onDrop={handleCvDrop}
              role="group"
              aria-label={t("Internship CV 上传区域")}
            >
              <div className="internship-cv-drop-icon"><FileUp size={22} /></div>
              <div className="internship-cv-drop-copy">
                <strong>{profile.cvPath ? t("已添加 Internship CV") : t("拖入 CV 文件，或点击选择")}</strong>
                <span>{profile.cvPath ? cvFileName || t("已导入 CV") : t("支持 PDF、DOCX、Markdown、TXT，最大 25 MB")}</span>
                {profile.cvPath && <small>{t("已复制到本机资料目录；原文件未改动。保存画像后用于匹配。")}</small>}
              </div>
              <div className="internship-cv-actions">
                <button type="button" className="button secondary" disabled={busy} onClick={() => void chooseCv()}>{profile.cvPath ? t("更换 CV") : t("选择 CV 文件")}</button>
                {profile.cvPath && <button type="button" className="text-button danger" disabled={busy} onClick={removeCv}>{t("移除")}</button>}
              </div>
            </div>
          </div>
        </div>
        <div className="planning-editor-actions">
          <button className="button primary" disabled={busy} onClick={() => void saveProfile()}><Save size={16} /> {t(" 保存 Internship 画像")}</button>
          <span className="planning-section-note">{t("最后更新：")}{profile.updatedAt ? new Date(profile.updatedAt).toLocaleString() : t("尚未保存")}</span>
        </div>
      </section>

      {notice && <div className="planning-notice"><CircleAlert size={17} /><span>{t(notice)}</span></div>}

      <div className="internship-planning-grid">
        <article className="planning-card planning-radar-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><Radar size={21} /></div>
            <div><span className="planning-index">{t("01 · 机会雷达")}</span><h3>{t("覆盖目标岗位")}</h3></div>
          </div>
          <div className="planning-summary"><span>{t("检索配置")}</span><strong>GPT / Codex</strong></div>
          <div className="planning-source-row"><span>{t("官方职位页")}</span><span>{t("ATS 记录")}</span><span>LinkedIn / X</span><span>{t("地区条件")}</span><span>{t("工作方式")}</span></div>
          <div className="planning-track-list">
            {searchTracks.map((track, index) => <span key={track} className={index < 3 ? "priority" : ""}>{t(track)}</span>)}
          </div>
          <button className="button primary wide" onClick={() => onNavigate({ page: "automation", composer: "internship_search" })}><Radar size={16} /> {t(" 在 Agent 中启动全面扫描 ")}<ArrowRight size={16} /></button>
        </article>

        <article className="planning-card planning-resume-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><GitBranch size={21} /></div>
            <div><span className="planning-index">{t("02 · 主简历证据")}</span><h3>{t("一份母版，持续积累")}</h3></div>
          </div>
          <div className="evidence-count"><strong>—</strong><span>{t("等待导入已核验贡献")}</span></div>
          <div className="evidence-list">
            {contributions.map((item) => <div className="evidence-row" key={item.id}><CheckCircle2 size={15} /><strong>{t(item.id)}</strong><span>{t(item.label)}</span></div>)}
          </div>
          <div className="planning-note"><ShieldCheck size={16} /> {t(" 只把你确认的贡献写入主简历；不会把 fork 的全部代码当成个人经历。")}</div>
        </article>

        <article className="planning-card planning-improvement-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><Target size={21} /></div>
            <div><span className="planning-index">{t("03 · 岗位针对性提升")}</span><h3>{t("把缺口变成贡献")}</h3></div>
          </div>
          <div className="improvement-list">
            {improvementTracks.map((item) => <div className="improvement-row" key={item.index}>
              <span className="improvement-index">{item.index}</span>
              <div><div className="improvement-title"><strong>{t(item.title)}</strong><span>{t(item.state)}</span></div><p>{t(item.detail)}</p></div>
            </div>)}
          </div>
          <button className="text-button planning-action" onClick={() => onNavigate({ page: "automation", composer: "internship_search" })}><Sparkles size={15} /> {t(" 让 Agent 拆解下一步 ")}<ArrowRight size={15} /></button>
        </article>
      </div>
    </section>
  );
}

function profilePreferences(profile?: InternshipProfile) {
  return [
    { label: "地点", value: profile?.regions || t("待设置") },
    { label: "工作方式", value: profile?.workMode || t("待设置") },
    { label: "实习时长", value: profile?.duration || t("待设置") },
    { label: "开始时间", value: profile?.startDate || t("待设置") },
  ];
}

function isDropInside(position: { x: number; y: number }, rect: DOMRect) {
  const ratio = window.devicePixelRatio || 1;
  const x = position.x / ratio;
  const y = position.y / ratio;
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || "已导入 CV";
}
