import { ArrowRight, FlaskConical, MapPinned } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import { ErrorState, LoadingState, StatusBadge, SubmissionBadge } from "../components/Ui";
import type { ApplicationFilter, AppRoute, CareerSystem, DashboardData } from "../types";

export function DashboardPage({ careerSystem, onNavigate }: { careerSystem: CareerSystem; onNavigate: (route: AppRoute) => void }) {
  const [data, setData] = useState<DashboardData>();
  const [error, setError] = useState("");
  const load = () => {
    setError("");
    api.dashboard(careerSystem).then(setData).catch((value) => setError(errorMessage(value)));
  };
  useEffect(() => { setData(undefined); load(); }, [careerSystem]);

  if (error) return <Page><ErrorState message={error} retry={load} /></Page>;
  if (!data) return <Page><LoadingState /></Page>;
  const internship = careerSystem === "internship";
  const maxRegion = Math.max(1, ...data.regions.map((item) => item.count));

  return (
    <Page>
      <section className="hero-panel">
        <div className="hero-rings" />
        <div className="eyebrow">{internship ? "2027 INTERNSHIP SEASON" : "2027 申请季"} · 本机工作区</div>
        <h1>{internship ? <>把散落的 Internship 机会，<br />变成可执行的申请清单。</> : <>更安静、更严谨地找到<br />真正合适的实验室。</>}</h1>
        <p>{internship
          ? "从官方职位检索、硬性资格核验和匹配评分，到申请清单与投递跟踪，集中在独立的 InternOS 工作区。"
          : "从研究检索与证据匹配，到材料、联系目标和回复处理，保存在同一份可追溯的本地数据中。"}</p>
        <div className="hero-meta"><span className="pulse-dot" /> 数据已迁入本机 SQLite · 外部操作始终需要确认</div>
      </section>

      <section className="metric-grid" aria-label="申请指标">
        {data.metrics.filter((metric) => metric.key !== "replied").map((metric) => (
          <button
            className="metric-card"
            key={metric.key}
            onClick={() => {
              const status = metric.key === "high_fit" ? "all" : metric.key;
              onNavigate({ page: "applications", status: status as ApplicationFilter });
            }}
          >
            <span className="metric-label">{metric.label}</span>
            <strong>{metric.value}</strong>
            <span className="metric-helper">{metric.helper}</span>
            <ArrowRight size={16} className="metric-arrow" />
          </button>
        ))}
      </section>

      <section className="dashboard-grid">
        <div className="panel region-panel">
          <div className="section-heading">
            <div><span className="section-index">01</span><h2>全球地区分布</h2></div>
            <MapPinned size={20} />
          </div>
          <div className="region-chart">
            {data.regions.map((item) => (
              <div className="region-row" key={item.region}>
                <span>{item.region}</span>
                <div className="region-track"><i style={{ width: `${(item.count / maxRegion) * 100}%` }} /></div>
                <strong>{item.count}</strong>
              </div>
            ))}
          </div>
        </div>

        <div className="panel priority-panel">
          <div className="section-heading">
            <div><span className="section-index">02</span><h2>优先待处理工作区</h2></div>
            <button className="text-button" onClick={() => onNavigate({ page: "applications", status: internship ? "portal_pending" : "ready_to_contact" })}>
              查看全部 <ArrowRight size={15} />
            </button>
          </div>
          <div className="priority-list">
            {data.priorityTargets.map((target) => (
              <button className="priority-item" key={target.id} onClick={() => onNavigate({ page: "application", targetId: target.id })}>
                <div className="score-orbit"><strong>{Math.round(target.fitScore ?? 0)}</strong><span>匹配</span></div>
                <div className="priority-copy">
                  <h3>{target.organization}</h3>
                  <p>{target.title}</p>
                  <span>{target.name}{target.country ? ` · ${target.country}` : ""}</span>
                </div>
                {internship ? <SubmissionBadge status={target.submissionStatus} /> : <StatusBadge status={target.status} />}
                <ArrowRight size={18} />
              </button>
            ))}
          </div>
        </div>
      </section>

      <button className="dashboard-agent-cta" onClick={() => onNavigate({ page: "automation" })}>
        <FlaskConical size={21} />
        <span><strong>{internship ? "开始新的 Internship 搜索" : "开始新的研究任务"}</strong><small>{internship ? "描述岗位、地点与硬性条件，只核验官方职位来源" : "完整检索、按姓名核验或材料修订"}</small></span>
        <ArrowRight size={18} />
      </button>
    </Page>
  );
}

function Page({ children }: { children: React.ReactNode }) {
  return <div className="page dashboard-page">{children}</div>;
}
