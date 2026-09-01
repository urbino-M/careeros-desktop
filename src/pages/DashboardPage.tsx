import { ArrowRight, FlaskConical, MapPinned } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import { ErrorState, LoadingState, StatusBadge } from "../components/Ui";
import type { AppRoute, DashboardData, StatusFilter } from "../types";

export function DashboardPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  const [data, setData] = useState<DashboardData>();
  const [error, setError] = useState("");
  const load = () => {
    setError("");
    api.dashboard().then(setData).catch((value) => setError(errorMessage(value)));
  };
  useEffect(load, []);

  if (error) return <Page><ErrorState message={error} retry={load} /></Page>;
  if (!data) return <Page><LoadingState /></Page>;
  const maxRegion = Math.max(1, ...data.regions.map((item) => item.count));

  return (
    <Page>
      <section className="hero-panel">
        <div className="hero-rings" />
        <div className="eyebrow">2027 申请季 · 本机工作区</div>
        <h1>更安静、更严谨地找到<br />真正合适的实验室。</h1>
        <p>从研究检索与证据匹配，到材料、联系目标和回复处理，保存在同一份可追溯的本地数据中。</p>
        <div className="hero-meta"><span className="pulse-dot" /> 数据已迁入本机 SQLite · 外部操作始终需要确认</div>
      </section>

      <section className="metric-grid" aria-label="申请指标">
        {data.metrics.filter((metric) => metric.key !== "replied").map((metric) => (
          <button
            className="metric-card"
            key={metric.key}
            onClick={() => {
              const status = metric.key === "high_fit" ? "all" : metric.key;
              onNavigate({ page: "applications", status: status as StatusFilter });
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
            <button className="text-button" onClick={() => onNavigate({ page: "applications", status: "ready_to_contact" })}>
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
                <StatusBadge status={target.status} />
                <ArrowRight size={18} />
              </button>
            ))}
          </div>
        </div>
      </section>

      <button className="dashboard-agent-cta" onClick={() => onNavigate({ page: "automation" })}>
        <FlaskConical size={21} />
        <span><strong>开始新的研究任务</strong><small>完整检索、按姓名核验或材料修订</small></span>
        <ArrowRight size={18} />
      </button>
    </Page>
  );
}

function Page({ children }: { children: React.ReactNode }) {
  return <div className="page dashboard-page">{children}</div>;
}
