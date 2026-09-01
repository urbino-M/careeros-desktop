import { ArrowLeft, ArrowRight, Filter, Mail, Search, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import { EmptyState, ErrorState, LoadingState, StatusBadge, SubmissionBadge, statusLabels } from "../components/Ui";
import type { AppRoute, DashboardData, StatusFilter, TargetCard } from "../types";

const filters: StatusFilter[] = [
  "ready_to_contact",
  "contacted",
  "replied",
  "follow_up",
  "shelved",
  "all",
];

const filterLabels: Record<StatusFilter, string> = {
  ...statusLabels,
  all: "全部",
};

export function ApplicationsPage({
  status,
  onNavigate,
}: {
  status: StatusFilter;
  onNavigate: (route: AppRoute) => void;
}) {
  const [targets, setTargets] = useState<TargetCard[]>();
  const [dashboard, setDashboard] = useState<DashboardData>();
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [error, setError] = useState("");
  const pageSize = 10;

  const load = () => {
    setError("");
    Promise.all([
      api.targets(status, query, page * pageSize, pageSize + 1),
      api.dashboard(),
    ])
      .then(([targetData, dashboardData]) => {
        setTargets(targetData);
        setDashboard(dashboardData);
      })
      .catch((value) => setError(errorMessage(value)));
  };

  useEffect(() => {
    setPage(0);
    setTargets(undefined);
  }, [status]);
  useEffect(load, [status, query, page]);

  const counts = useMemo(() => {
    const result: Record<string, number> = {};
    dashboard?.metrics.forEach((metric) => (result[metric.key] = metric.value));
    return result;
  }, [dashboard]);
  const visible = targets?.slice(0, pageSize) ?? [];
  const hasNext = (targets?.length ?? 0) > pageSize;

  return (
    <div className="page applications-page">
      <button className="back-button" onClick={() => onNavigate({ page: "dashboard" })}>
        <ArrowLeft size={17} /> 返回仪表盘
      </button>
      <header className="page-header">
        <div className="eyebrow">APPLICATION WORKSPACE</div>
        <h1>申请中心</h1>
        <p>Postdoc 联系目标和 industry internship 机会保存在同一个可追溯工作区；投递与联系状态彼此独立。</p>
      </header>

      <div className="status-tabs" role="tablist" aria-label="申请状态">
        {filters.map((filter) => (
          <button
            role="tab"
            aria-selected={status === filter}
            className={status === filter ? "selected" : ""}
            key={filter}
            onClick={() => onNavigate({ page: "applications", status: filter })}
          >
            {filterLabels[filter]}
            <span>{filter === "all" ? counts.all ?? 0 : counts[filter] ?? 0}</span>
          </button>
        ))}
      </div>

      <div className="status-explainer">
        <Mail size={18} />
        Internship 搜索只保存已核验机会和申请清单，不会生成简历、联系公司或自动投递。
      </div>

      <div className="search-row">
        <label className="search-box">
          <Search size={18} />
          <input
            value={search}
            placeholder="搜索 PI、公司、机构、职位或研究主题…"
            onChange={(event) => setSearch(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") { setPage(0); setQuery(search); }
            }}
          />
          {search !== query && <button onClick={() => { setPage(0); setQuery(search); }}>搜索</button>}
        </label>
        <button className="button secondary"><SlidersHorizontal size={17} /> 筛选</button>
      </div>

      <div className="list-heading">
        <div><span className="section-index">01</span><h2>选择机会 / 申请</h2></div>
        <span><Filter size={14} /> 按匹配分从高到低</span>
      </div>

      {error && <ErrorState message={error} retry={load} />}
      {!error && !targets && <LoadingState label="正在整理独立联系目标" />}
      {!error && targets && visible.length === 0 && (
        <EmptyState title="这个分组还没有记录" body="状态变化后会自动出现在对应分组；隐藏墓碑不会进入任何工作列表。" />
      )}
      {!error && visible.length > 0 && (
        <div className="target-grid">
          {visible.map((target) => {
            const internship = target.careerTrack === "internship";
            return <article className="target-card" key={target.id}>
              <div className="target-card-top">
                <div className="score"><strong>{Math.round(target.fitScore ?? 0)}</strong><span>/ 100</span></div>
                <div className="target-card-badges">
                  {internship && <span className="career-track-badge">Internship</span>}
                  <SubmissionBadge status={target.submissionStatus} />
                  {!internship && <StatusBadge status={target.status} />}
                </div>
              </div>
              <h3>{target.organization}</h3>
              <p className="target-role">{target.title}</p>
              <dl>
                <div><dt>{internship ? "申请方式" : "PI / 联系目标"}</dt><dd>{target.name}</dd></div>
                <div><dt>地区</dt><dd>{[target.region, target.country].filter(Boolean).join(" · ") || "待确认"}</dd></div>
                {target.email && <div><dt>邮箱</dt><dd className="email-value">{target.email}</dd></div>}
                <div><dt>截止</dt><dd>{target.deadline || "待确认"}</dd></div>
              </dl>
              <button className="card-action" onClick={() => onNavigate({ page: "application", targetId: target.id })}>
                {internship ? "查看机会与申请清单" : "查看材料与联系记录"} <ArrowRight size={17} />
              </button>
            </article>;
          })}
        </div>
      )}

      {(page > 0 || hasNext) && (
        <div className="pagination">
          <button disabled={page === 0} onClick={() => setPage((value) => value - 1)}>上一页</button>
          <span>第 {page + 1} 页</span>
          <button disabled={!hasNext} onClick={() => setPage((value) => value + 1)}>下一页</button>
        </div>
      )}
    </div>
  );
}
