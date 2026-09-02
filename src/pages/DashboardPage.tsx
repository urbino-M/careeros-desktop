import { ArrowRight, BriefcaseBusiness, FlaskConical, MapPinned } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import { ErrorState, LoadingState, StatusBadge, SubmissionBadge } from "../components/Ui";
import type { ApplicationFilter, AppRoute, CareerSystem, DashboardData, DashboardMetric, RegionCount, TargetCard } from "../types";

type DashboardPair = {
  postdoc: DashboardData;
  internship: DashboardData;
};

export function DashboardPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  const [data, setData] = useState<DashboardPair>();
  const [error, setError] = useState("");
  const load = () => {
    setError("");
    Promise.all([api.dashboard("postdoc"), api.dashboard("internship")])
      .then(([postdoc, internship]) => setData({ postdoc, internship }))
      .catch((value) => setError(errorMessage(value)));
  };
  useEffect(() => { setData(undefined); load(); }, []);

  if (error) return <Page><ErrorState message={error} retry={load} /></Page>;
  if (!data) return <Page><LoadingState /></Page>;

  const regions = combineRegions([data.postdoc, data.internship]);
  const maxRegion = Math.max(1, ...regions.map((item) => item.count));
  const priorityTargets = [...data.postdoc.priorityTargets, ...data.internship.priorityTargets]
    .sort((left, right) => (right.fitScore ?? 0) - (left.fitScore ?? 0))
    .slice(0, 6);

  return (
    <Page>
      <section className="hero-panel">
        <div className="hero-rings" />
        <div className="eyebrow">2027 申请季 · 本机工作区</div>
        <h1>把研究与行业机会，<br />变成可执行的申请清单。</h1>
        <p>CareerOS 统一管理 Postdoc 与 Internship 两条申请轨道。检索、证据、申请状态和 Agent 任务集中在同一个可追溯的本地工作区中。</p>
        <div className="hero-meta"><span className="pulse-dot" /> 数据已迁入本机 SQLite · 外部操作始终需要确认</div>
      </section>

      <section className="career-overview-grid" aria-label="两条申请轨道概览">
        <TrackOverview system="postdoc" data={data.postdoc} onNavigate={onNavigate} />
        <TrackOverview system="internship" data={data.internship} onNavigate={onNavigate} />
      </section>

      <section className="dashboard-grid">
        <div className="panel region-panel">
          <div className="section-heading">
            <div><span className="section-index">01</span><h2>全球地区分布</h2></div>
            <MapPinned size={20} />
          </div>
          {regions.length ? (
            <div className="region-chart">
              {regions.map((item) => (
                <div className="region-row" key={item.region}>
                  <span>{item.region}</span>
                  <div className="region-track"><i style={{ width: `${(item.count / maxRegion) * 100}%` }} /></div>
                  <strong>{item.count}</strong>
                </div>
              ))}
            </div>
          ) : <p className="muted-copy">还没有可展示的地区数据。</p>}
        </div>

        <div className="panel priority-panel">
          <div className="section-heading">
            <div><span className="section-index">02</span><h2>优先待处理工作区</h2></div>
            <div className="priority-actions">
              <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "postdoc", status: "all" })}>Postdoc <ArrowRight size={15} /></button>
              <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status: "all" })}>Internship <ArrowRight size={15} /></button>
            </div>
          </div>
          <div className="priority-list">
            {priorityTargets.length ? priorityTargets.map((target) => <PriorityItem target={target} key={`${target.careerTrack}-${target.id}`} onNavigate={onNavigate} />) : <p className="muted-copy">还没有需要优先处理的申请目标。</p>}
          </div>
        </div>
      </section>

      <button className="dashboard-agent-cta" onClick={() => onNavigate({ page: "automation" })}>
        <FlaskConical size={21} />
        <span><strong>打开 Agent 运行中心</strong><small>同时检索研究机会与 Internship，结果分别进入对应申请清单</small></span>
        <ArrowRight size={18} />
      </button>
    </Page>
  );
}

function TrackOverview({ system, data, onNavigate }: { system: CareerSystem; data: DashboardData; onNavigate: (route: AppRoute) => void }) {
  const internship = system === "internship";
  const metrics = (internship ? ["all", "high_fit", "portal_pending"] : ["all", "high_fit", "ready_to_contact"])
    .map((key) => getMetric(data, key));
  const Icon = internship ? BriefcaseBusiness : FlaskConical;
  const title = internship ? "Internship 申请" : "Postdoc 申请";
  const description = internship
    ? "官方职位核验、资格判断与官网投递跟踪。"
    : "PI / 实验室检索、材料准备与联系跟进。";
  return (
    <section className={`career-overview-panel ${internship ? "internship" : "postdoc"}`}>
      <div className="career-overview-heading">
        <div className="career-overview-title">
          <Icon size={21} />
          <div><span className="eyebrow">{internship ? "INDUSTRY TRACK" : "ACADEMIC TRACK"}</span><h2>{title}</h2></div>
        </div>
        <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: system, status: "all" })}>打开 <ArrowRight size={15} /></button>
      </div>
      <p className="career-overview-copy">{description}</p>
      <div className="career-overview-metrics">
        {metrics.map((metric) => (
          <button className="career-overview-metric" key={metric.key} onClick={() => onNavigate({ page: "applications", careerSystem: system, status: metricDestination(metric.key) })}>
            <span>{metric.label}</span>
            <strong>{metric.value}</strong>
            <small>{metric.helper}</small>
          </button>
        ))}
      </div>
    </section>
  );
}

function PriorityItem({ target, onNavigate }: { target: TargetCard; onNavigate: (route: AppRoute) => void }) {
  const internship = target.careerTrack === "internship";
  return (
    <button className="priority-item" onClick={() => onNavigate({ page: "application", targetId: target.id, careerSystem: target.careerTrack })}>
      <div className="score-orbit"><strong>{Math.round(target.fitScore ?? 0)}</strong><span>匹配</span></div>
      <div className="priority-copy">
        <div className="priority-title-row"><h3>{target.organization}</h3><span className="track-pill">{internship ? "Internship" : "Postdoc"}</span></div>
        <p>{target.title}</p>
        <span>{target.name}{target.country ? ` · ${target.country}` : ""}</span>
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

function combineRegions(dashboards: DashboardData[]): RegionCount[] {
  const counts = new Map<string, number>();
  dashboards.flatMap((dashboard) => dashboard.regions).forEach((item) => counts.set(item.region, (counts.get(item.region) ?? 0) + item.count));
  return [...counts.entries()]
    .map(([region, count]) => ({ region, count }))
    .sort((left, right) => right.count - left.count || left.region.localeCompare(right.region))
    .slice(0, 8);
}

function Page({ children }: { children: React.ReactNode }) {
  return <div className="page dashboard-page">{children}</div>;
}
