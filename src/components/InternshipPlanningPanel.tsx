import { t } from "../i18n";
import {
  ArrowRight,
  CheckCircle2,
  GitBranch,
  Radar,
  ShieldCheck,
  Sparkles,
  Target,
} from "lucide-react";
import type { AppRoute } from "../types";

const searchTracks = [
  "方向 A",
  "方向 B",
  "方向 C",
  "方向 D",
  "方向 E",
  "方向 F",
  "方向 G",
];

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

const internshipPreferences = [
  { label: "地点", value: "待设置" },
  { label: "工作方式", value: "待设置" },
  { label: "实习时长", value: "待设置" },
  { label: "开始时间", value: "待设置" },
];

export function InternshipPlanningSummary({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  return (
    <section className="internship-context-strip" aria-label={t("当前 Internship 求职主线")}>
      <div className="internship-context-title">
        <Radar size={19} />
        <div><span>{t("当前求职主线")}</span><strong>{t("待设置")}</strong></div>
      </div>
      <div className="internship-context-profile">
        {internshipPreferences.map((item) => (
          <div className="internship-context-item" key={item.label}>
            <span>{t(item.label)}</span>
            <strong>{t(item.value)}</strong>
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
  return (
    <section className="internship-planning" aria-label={t("Internship 求职策略")}>
      <header className="internship-planning-header">
        <div>
          <div className="eyebrow">{t("当前轨道")}</div>
          <h2>{t("求职策略")}</h2>
          <p>{t("先定义统一的求职筛选画像，再用多来源检索保证机会覆盖，把岗位要求映射到你的主简历和下一项工程贡献。")}</p>
        </div>
        <div className="internship-search-profile" aria-label={t("Internship 求职筛选画像")}>
          {internshipPreferences.map((item) => (
            <div className="internship-search-profile-item" key={item.label}>
              <span>{t(item.label)}</span>
              <strong>{t(item.value)}</strong>
            </div>
          ))}
        </div>
      </header>

      <div className="internship-planning-grid">
        <article className="planning-card planning-radar-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><Radar size={21} /></div>
            <div><span className="planning-index">{t("01 · 机会雷达")}</span><h3>{t("覆盖目标岗位")}</h3></div>
          </div>
          <div className="planning-summary"><span>{t("检索配置")}</span><strong>{t("待设置")}</strong></div>
          <div className="planning-source-row"><span>{t("官方职位页")}</span><span>{t("ATS 记录")}</span><span>{t("地区条件")}</span><span>{t("工作方式")}</span></div>
          <div className="planning-track-list">
            {searchTracks.map((track, index) => <span key={track} className={index < 3 ? "priority" : ""}>{t(track)}</span>)}
          </div>
          <button className="button primary wide" onClick={() => onNavigate({ page: "automation" })}><Radar size={16} /> {t(" 在 Agent 中启动全面扫描 ")}<ArrowRight size={16} /></button>
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
          <button className="text-button planning-action" onClick={() => onNavigate({ page: "automation" })}><Sparkles size={15} /> {t(" 让 Agent 拆解下一步 ")}<ArrowRight size={15} /></button>
        </article>
      </div>
    </section>
  );
}
