import { t } from "../i18n";
import { ArrowRight, BriefcaseBusiness, FlaskConical, MapPinned } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import { ErrorState, LoadingState, StatusBadge, SubmissionBadge } from "../components/Ui";
import type { ApplicationFilter, AppRoute, CareerSystem, DashboardData, DashboardMetric, DiscoveredOpportunityPage, OpportunityCategory, RegionCount, TargetCard } from "../types";
import { DiscoveredOpportunityCard, deadlineLabel, opportunityCategoryLabels } from "./ApplicationsPage";

type DashboardPair = {
  postdoc: DashboardData;
  internship: DashboardData;
  opportunities: Record<OpportunityCategory, DiscoveredOpportunityPage>;
};
type DashboardView = "overview" | CareerSystem;

export function DashboardPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  const [view, setView] = useState<DashboardView>("overview");
  const [data, setData] = useState<DashboardPair>();
  const [error, setError] = useState("");
  const requestSequence = useRef(0);
  const load = () => {
    const request = ++requestSequence.current;
    setError("");
    Promise.all([api.dashboard("postdoc"), api.dashboard("internship"),
      api.discoveredOpportunities("", 0, 3, false, "advertised"),
      api.discoveredOpportunities("", 0, 3, false, "prospective"),
      api.discoveredOpportunities("", 0, 1, false, "uncertain"),
    ])
      .then(([postdoc, internship, advertised, prospective, uncertain]) => {
        if (request !== requestSequence.current) return;
        setData({ postdoc, internship, opportunities: { advertised, prospective, uncertain } });
      })
      .catch((value) => { if (request === requestSequence.current) setError(errorMessage(value)); });
  };
  useEffect(() => {
    load();
    const timer = window.setInterval(load, 30000);
    return () => { requestSequence.current += 1; window.clearInterval(timer); };
  }, []);

  if (error) return <Page><ErrorState message={error} retry={load} /></Page>;
  if (!data) return <Page><LoadingState /></Page>;

  return <DashboardContent data={data} view={view} onViewChange={setView} onNavigate={onNavigate} onChanged={load} />;
}

export function DashboardContent({ data, view, onViewChange, onNavigate, onChanged }: {
  data: DashboardPair;
  view: DashboardView;
  onViewChange: (view: DashboardView) => void;
  onChanged?: () => void;
  onNavigate: (route: AppRoute) => void;
}) {
  return (
    <Page>
      <section className="hero-panel dashboard-hero">
        <div className="hero-rings" />
        <div className="eyebrow">{t("CAREER WORKSPACE · 本机工作区")}</div>
        <h1>{t("申请进展，一眼看清。")}</h1>
        <p>{t("总览查看两条轨道的摘要，进入各自工作区处理机会与进度。Postdoc 和 Internship 独立展示，不混排申请。")}</p>
        <div className="hero-meta"><span className="pulse-dot" /> {t("数据已迁入本机 SQLite · 外部操作始终需要确认")}</div>
      </section>

      <div className="dashboard-view-tabs" role="tablist" aria-label={t("仪表盘轨道")}>
        {(["overview", "postdoc", "internship"] as const).map((value) => <button key={value} id={`dashboard-tab-${value}`} role="tab" aria-selected={view === value} aria-controls="dashboard-track-panel" className={view === value ? "selected" : ""} onClick={() => onViewChange(value)}>
          {value === "overview" ? t("总览") : value === "postdoc" ? "Postdoc" : "Internship"}
        </button>)}
      </div>

      <div id="dashboard-track-panel" role="tabpanel" aria-labelledby={`dashboard-tab-${view}`}>
      {view === "overview" && <>
        <p className="dashboard-scope-note">{t("这里只显示轨道摘要；具体岗位、材料和待办请进入对应轨道。")}</p>
        <section className="career-overview-grid" aria-label={t("两条申请轨道概览")}>
          <TrackOverview system="postdoc" data={data.postdoc} opportunityTotal={data.opportunities.advertised.overallTotal} deadlineHint={dashboardDeadlineHint(data.opportunities.advertised.items.filter((item) => item.status !== "closed").map((item) => item.deadline))} onNavigate={onNavigate} onOpenTrack={() => onViewChange("postdoc")} />
          <TrackOverview system="internship" data={data.internship} deadlineHint={dashboardDeadlineHint(data.internship.priorityTargets.filter((item) => item.opportunityStatus !== "closed").map((item) => item.deadline))} onNavigate={onNavigate} onOpenTrack={() => onViewChange("internship")} />
        </section>
      </>}

      {view === "postdoc" && <>
      <div className="dashboard-track-heading"><FlaskConical size={22} /><div><h2>{t("Postdoc 工作区")}</h2><p>{t("公开岗位、套磁、材料与联系人进度，只属于这条轨道。")}</p></div></div>
      <TrackOverview system="postdoc" data={data.postdoc} opportunityTotal={data.opportunities.advertised.overallTotal} onNavigate={onNavigate} />
      <div className="dashboard-attention" aria-label={t("Postdoc 需要处理")}>
        <span><strong>{data.opportunities.advertised.pendingTotal}</strong> {t("个机会材料待补齐")}</span>
        <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "postdoc", status: "replied" })}><strong>{getMetric(data.postdoc, "replied").value}</strong> {t("位联系人有回复待处理")} <ArrowRight size={14} /></button>
        <span>{t("机会数与联系人数分开统计")}</span>
      </div>
      <DashboardOpportunitySections opportunities={data.opportunities} onNavigate={onNavigate} onChanged={onChanged} />
      <RegionPanel regions={data.postdoc.regions} title={t("Postdoc 联系人地区分布")} />
      </>}

      {view === "internship" && <>
        <div className="dashboard-track-heading internship"><BriefcaseBusiness size={22} /><div><h2>{t("Internship 工作区")}</h2><p>{t("实习岗位与官网投递独立管理，不包含 Postdoc 套磁或材料待办。")}</p></div></div>
        <TrackOverview system="internship" data={data.internship} onNavigate={onNavigate} />
        <div className="dashboard-attention" aria-label={t("Internship 投递进度")}>
          {(["not_set", "portal_pending", "submitted", "not_required"] as const).map((status) => <button key={status} className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status })}>
            {t(getMetric(data.internship, status).label)} <strong>{getMetric(data.internship, status).value}</strong> {t("条")}
          </button>)}
        </div>
      <section className="dashboard-grid dashboard-track-detail">
        <div className="panel priority-panel">
          <div className="section-heading">
            <h2>{t("实习岗位 · 截止优先")}</h2>
            <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status: "all" })}>{t("查看全部 ")}<ArrowRight size={15} /></button>
          </div>
          <p className="muted-copy">{t("优先展示前 4 条；截止日期未知的随后，已过期或关闭的置底。投递状态见每条记录右侧。")}</p>
          <div className="priority-list">
            {data.internship.priorityTargets.length ? data.internship.priorityTargets.map((target) => <PriorityItem target={target} key={target.id} onNavigate={onNavigate} />) : <p className="muted-copy">{t("还没有实习岗位。新增 Internship 检索后，结果会保存在这条轨道。")}</p>}
          </div>
        </div>
        <RegionPanel regions={data.internship.regions} title={t("Internship 申请地区分布")} />
      </section>
      </>}
      </div>

      <button className="dashboard-agent-cta" onClick={() => onNavigate({ page: "automation" })}>
        <FlaskConical size={21} />
        <span><strong>{t("打开 Agent 运行中心")}</strong><small>{t("创建任务时选择申请轨道，结果保存在对应工作区")}</small></span>
        <ArrowRight size={18} />
      </button>
    </Page>
  );
}

export function DashboardOpportunitySections({ opportunities, onNavigate, onChanged }: {
  opportunities: Record<OpportunityCategory, DiscoveredOpportunityPage>;
  onChanged?: () => void;
  onNavigate: (route: AppRoute) => void;
}) {
  return <>
    <section className="dashboard-opportunity-grid" aria-label={t("Postdoc 岗位与套磁")}>
      {(["advertised", "prospective"] as const).map((category) => <section className="panel" key={category}>
        <div className="section-heading">
          <h2>{t(opportunityCategoryLabels[category])} <small>{opportunities[category].total}</small></h2>
          <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "postdoc", status: "all", category })}>{t("查看全部")} <ArrowRight size={15} /></button>
        </div>
        <p className="muted-copy">{t(category === "advertised" ? "按截止时间优先展示前 3 条 · 未知日期随后 · 已过期/关闭置底" : "按匹配度优先展示前 3 条 · 不等同于已有公开岗位")}</p>
        {opportunities[category].items.length ? opportunities[category].items.map((opportunity) => <DiscoveredOpportunityCard key={opportunity.id} opportunity={opportunity} category={category} onNavigate={onNavigate} onChanged={onChanged} />) : <p className="muted-copy">{t("暂无这类机会，检索保存后会出现在这里。")}</p>}
      </section>)}
    </section>
    {opportunities.uncertain.total > 0 && <div className="dashboard-unclassified">
      <p>{opportunities.uncertain.total} {t("条机会的类型待核实，暂不归入公开岗位或套磁。")}</p>
      <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "postdoc", status: "all", category: "uncertain" })}>{t("查看待核实记录 ")}<ArrowRight size={15} /></button>
    </div>}
  </>;
}

export function dashboardDeadlineHint(deadlines: (string | null | undefined)[], now = new Date()): string {
  const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
  const dates = deadlines.map((value) => value?.trim()).filter((value): value is string => {
    if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value) || value < today) return false;
    const date = new Date(`${value}T00:00:00Z`);
    return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
  }).sort();
  return dates.length ? deadlineLabel(dates[0], now) : t("暂无已知未截止日期");
}

function TrackOverview({ system, data, opportunityTotal, deadlineHint, onNavigate, onOpenTrack }: { system: CareerSystem; data: DashboardData; opportunityTotal?: number; deadlineHint?: string; onNavigate: (route: AppRoute) => void; onOpenTrack?: () => void }) {
  const internship = system === "internship";
  const metrics = (internship ? ["all", "high_fit", "portal_pending"] : ["all", "high_fit", "ready_to_contact"])
    .map((key) => !internship && key === "all" && opportunityTotal !== undefined
      ? { key: "all", label: "全部机会", value: opportunityTotal, helper: "包含材料未齐与暂无联系人的机会" }
      : !internship ? { ...getMetric(data, key), label: key === "high_fit" ? "高匹配联系人（人）" : "待联系（人）" }
      : { ...getMetric(data, key), label: key === "all" ? "申请记录（条）" : key === "high_fit" ? "高匹配记录（条）" : "待投递（条）" });
  const Icon = internship ? BriefcaseBusiness : FlaskConical;
  const title = internship ? "Internship 申请" : "Postdoc 申请";
  const description = internship
    ? "GPT / Codex 网页搜索、官方来源核验、资格判断与官网投递跟踪。"
    : "PI / 实验室检索、材料准备与联系跟进。";
  return (
    <section className={`career-overview-panel ${internship ? "internship" : "postdoc"}`}>
      <div className="career-overview-heading">
        <div className="career-overview-title">
          <Icon size={21} />
          <div><span className="eyebrow">{t(internship ? "行业轨道" : "学术轨道")}</span><h2>{t(title)}</h2></div>
        </div>
        <button className="text-button" onClick={() => onOpenTrack ? onOpenTrack() : onNavigate({ page: "applications", careerSystem: system, status: "all" })}>{onOpenTrack ? t("进入轨道") : t("打开申请列表")} <ArrowRight size={15} /></button>
      </div>
      <p className="career-overview-copy">{t(description)}</p>
      <div className="career-overview-metrics">
        {metrics.map((metric) => (
          <button className="career-overview-metric" key={metric.key} onClick={() => onNavigate({ page: "applications", careerSystem: system, status: metricDestination(metric.key) })}>
            <span>{t(metric.label)}</span>
            <strong>{metric.value}</strong>
            <small>{t(metric.helper)}</small>
          </button>
        ))}
      </div>
      {onOpenTrack && <div className="dashboard-summary-footnote"><p>{internship
        ? t("已投递 {0} 条 · 尚未开始 {1} 条", getMetric(data, "submitted").value, getMetric(data, "not_set").value)
        : t("回复待处理 {0} 人 · 跟进 {1} 人", getMetric(data, "replied").value, getMetric(data, "follow_up").value)}</p>
        <p>{t("最近已知截止：")}{deadlineHint}</p>
      </div>}
    </section>
  );
}

function PriorityItem({ target, onNavigate }: { target: TargetCard; onNavigate: (route: AppRoute) => void }) {
  const internship = target.careerTrack === "internship";
  return (
    <button className="priority-item" onClick={() => onNavigate({ page: "application", targetId: target.id, careerSystem: target.careerTrack })}>
      <div className="score-orbit"><strong>{Math.round(target.fitScore ?? 0)}</strong><span>{t("匹配")}</span></div>
      <div className="priority-copy">
        <div className="priority-title-row"><h3>{target.organization}</h3><span className="track-pill">{internship ? "Internship" : "Postdoc"}</span></div>
        <p>{target.title}</p>
        <span>{target.name}{target.country ? ` · ${target.country}` : ""}</span>
        <span className="priority-deadline">{target.opportunityStatus === "closed" ? t("已关闭 · ") : ""}{deadlineLabel(target.deadline)}</span>
      </div>
      {internship ? <SubmissionBadge status={target.submissionStatus} /> : <StatusBadge status={target.status} />}
      <ArrowRight size={18} />
    </button>
  );
}

function getMetric(data: DashboardData, key: string): DashboardMetric {
  return data.metrics.find((metric) => metric.key === key) ?? {
    key,
    label: key === "high_fit" ? "高匹配" : "申请机会",
    value: 0,
    helper: "暂无数据",
  };
}

function metricDestination(key: string): ApplicationFilter {
  return key === "high_fit" ? "all" : key as ApplicationFilter;
}

function RegionPanel({ regions, title }: { regions: RegionCount[]; title: string }) {
  const maxRegion = Math.max(1, ...regions.map((item) => item.count));
  return <section className="panel region-panel dashboard-region-panel">
    <div className="section-heading"><h2>{title}</h2><MapPinned size={20} /></div>
    {regions.length ? <div className="region-chart">{regions.map((item) => <div className="region-row" key={item.region}>
      <span>{item.region}</span><div className="region-track"><i style={{ width: `${(item.count / maxRegion) * 100}%` }} /></div><strong>{item.count}</strong>
    </div>)}</div> : <p className="muted-copy">{t("这条轨道还没有地区数据。")}</p>}
  </section>;
}

function Page({ children }: { children: React.ReactNode }) {
  return <div className="page dashboard-page">{children}</div>;
}
