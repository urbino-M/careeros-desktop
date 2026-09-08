import { t } from "../i18n";
import {
  Activity,
  ArrowLeft,
  Bot,
  BriefcaseBusiness,
  Check,
  ChevronDown,
  CircleStop,
  Clock3,
  History,
  ListRestart,
  Play,
  Plus,
  RotateCcw,
  SearchCheck,
  UserSearch,
} from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "../api";
import { ModelControls, type ModelSelection } from "../components/ModelControls";
import { ErrorState, LoadingState, StatusBadge, formatLocalTime, jobLabels } from "../components/Ui";
import type { ApplicationTab, AppRoute, JobGroups, JobSummary, RetryJobRequest, AutomationComposer, InternshipProfile } from "../types";

type ComposerType = "internship_search" | "full_search" | "research_pi" | "opportunity_health" | "follow_up_scan" | null;
const DEFAULT_RESULT_LIMIT = 5;
const MAX_RESULT_LIMIT = 5;

export function AutomationPage({ initialComposer, onNavigate }: { initialComposer?: AutomationComposer; onNavigate: (route: AppRoute) => void }) {
  const [jobs, setJobs] = useState<JobGroups>();
  const [error, setError] = useState("");
  const [historySize, setHistorySize] = useState(5);
  const [composer, setComposer] = useState<ComposerType>(initialComposer ?? null);
  const [notice, setNotice] = useState("");
  const load = () => api.jobs(historySize).then(setJobs).catch((value) => setError(errorMessage(value)));

  useEffect(() => {
    load();
    const timer = window.setInterval(load, 2000);
    let unlisten: (() => void) | undefined;
    listen("careeros://jobs-changed", load).then((value) => (unlisten = value));
    return () => { window.clearInterval(timer); unlisten?.(); };
  }, [historySize]);

  useEffect(() => {
    if (initialComposer) setComposer(initialComposer);
  }, [initialComposer]);

  const taskHistory = useMemo(() => {
    if (!jobs) return [];
    return buildTaskHistory(jobs, historySize);
  }, [jobs, historySize]);

  const visibleJobs = jobs;

  return (
    <div className="page automation-page">
      <button className="back-button" onClick={() => onNavigate({ page: "dashboard" })}>
        <ArrowLeft size={17} /> {t("返回仪表盘")}
      </button>
      <header className="page-header automation-header">
        <div className="eyebrow">{t("本地 Agent 控制")}</div>
        <h1>{t("Agent 运行中心")}</h1>
        <p>{t("统一启动研究机会与 Internship 检索，查看实时进度，并处理需要你确认的结果。最多 5 个任务并行，第 6 个自动排队。")}</p>
        <div className="quick-actions">
          <button className="button primary" onClick={() => setComposer("full_search")}><Plus size={17} /> {t("寻找 Postdoc 机会")}</button>
          <button className="button secondary" onClick={() => setComposer("internship_search")}><BriefcaseBusiness size={17} /> {t("寻找 Internship")}</button>
          <button className="button secondary" onClick={() => setComposer("research_pi")}><UserSearch size={17} /> {t("按姓名找机会")}</button>
          <button className="button secondary" onClick={() => setComposer("opportunity_health")}><SearchCheck size={17} /> {t("检查机会")}</button>
          <button className="button secondary" onClick={() => onNavigate({ page: "applications", careerSystem: "postdoc", status: "all" })}><ListRestart size={17} /> {t("查看 Postdoc 申请")}</button>
          <button className="button secondary" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status: "all" })}><ListRestart size={17} /> {t("查看 Internship 申请")}</button>
          <button className="button secondary" onClick={() => setComposer("follow_up_scan")}><Clock3 size={17} /> {t("扫描跟进")}</button>
          <button className="button secondary" onClick={async () => {
            try {
              const id = await api.enqueue({ jobType: "preference_rebuild", targetType: "preferences", payload: { source: "recorded_revisions" } });
              setNotice(t("偏好学习已加入：{0}", id)); load();
            } catch (value) { setNotice(errorMessage(value)); }
          }}><Bot size={17} /> {t("更新偏好")}</button>
        </div>
      </header>

      {notice && <div className="inline-notice">{notice}</div>}

      {composer && <TaskComposer type={composer} onClose={() => setComposer(null)} onCreated={() => { setComposer(null); load(); }} />}

      {error && <ErrorState message={error} retry={load} />}
      {!error && !jobs && <LoadingState label={t("正在读取任务调度器")} />}
      {visibleJobs && (
        <>
          <div className="worker-strip">
            <span><i className="pulse-dot" /> {t("Worker 在线")}</span>
            <span>{t("并发 {0} / {1}", visibleJobs.running.length, visibleJobs.capacity)}</span>
            <span>{t("排队 {0}", visibleJobs.queued.length)}</span>
            <span>{t("待审核 {0}", visibleJobs.needsReviewTotal)}</span>
          </div>

          <JobSection sectionId="running" title={t("当前运行")} count={visibleJobs.running.length} hint={t("并发 {0} / {1} · 收起不影响执行", visibleJobs.running.length, visibleJobs.capacity)} icon={Activity} defaultOpen>
            {visibleJobs.running.length ? visibleJobs.running.map((job) => <JobCard job={job} key={job.id} onReload={load} onNavigate={onNavigate} />) : <CompactEmpty text={t("当前没有任务在执行；调度器会自动领取队列中的下一项。")} />}
          </JobSection>

          <JobSection sectionId="queued" title={t("等待队列")} count={visibleJobs.queued.length} hint={visibleJobs.queued.length ? t("按队列顺序等待执行") : t("暂无排队任务")} icon={Clock3}>
            {visibleJobs.queued.length ? visibleJobs.queued.map((job, index) => <QueueRow job={job} index={index} key={job.id} onReload={load} />) : <CompactEmpty text={t("队列为空。")} />}
          </JobSection>

          <JobSection sectionId="history" title={t("任务记录")} count={visibleJobs.recentTotal} hint={t("待处理 {0} · 查看结果与重试", visibleJobs.needsReviewTotal)} icon={History}>
            {taskHistory.length ? taskHistory.map((job) => <JobCard job={job} key={job.id} onReload={load} onNavigate={onNavigate} />) : <CompactEmpty text={t("尚无任务记录。")} />}
            {visibleJobs.recentTotal > historySize && <button className="load-more" onClick={() => setHistorySize((value) => value + 5)}>{t("再展开 5 条")}</button>}
          </JobSection>
        </>
      )}
    </div>
  );
}

export function JobSection({ sectionId, title, count, hint, icon: Icon, defaultOpen = false, children }: {
  sectionId: "running" | "queued" | "history"; title: string; count: number; hint: string;
  icon: typeof Activity; defaultOpen?: boolean; children: React.ReactNode;
}) {
  const bodyId = useId();
  const toggleId = `${bodyId}-toggle`;
  const storageKey = `careeros:automation:section:${sectionId}`;
  const [expanded, setExpanded] = useState(() => {
    try {
      const saved = window.sessionStorage.getItem(storageKey);
      return saved === "open" ? true : saved === "closed" ? false : defaultOpen;
    } catch { return defaultOpen; }
  });
  const toggle = () => {
    const next = !expanded;
    setExpanded(next);
    try { window.sessionStorage.setItem(storageKey, next ? "open" : "closed"); } catch { /* Storage is optional. */ }
  };
  return (
    <section className="job-section" data-section={sectionId}>
      <h2 className="job-section-heading">
        <button type="button" className="job-section-toggle" id={toggleId} aria-expanded={expanded} aria-controls={bodyId} onClick={toggle}>
          <span className="job-section-heading-copy">
            <span className="job-section-label"><Icon size={19} aria-hidden="true" />{title}<span className="job-section-count">{count}</span></span>
            <span className="job-section-hint">{hint}</span>
          </span>
          <span className="job-section-toggle-action">{expanded ? t("收起") : t("展开")}<ChevronDown size={18} aria-hidden="true" /></span>
        </button>
      </h2>
      <div className="job-section-body" id={bodyId} role="region" aria-labelledby={toggleId} hidden={!expanded}>{children}</div>
    </section>
  );
}

export function JobCard({ job, onReload, onNavigate }: { job: JobSummary; onReload: () => void; onNavigate: (route: AppRoute) => void }) {
  const failed = job.status === "failed";
  const isNative = job.providerId !== "legacy";
  const careerSystem = job.jobType === "internship_search" ? "internship" : "postdoc";
  const exactTarget = job.targetId?.startsWith("target") ? job.targetId : undefined;
  const resultTargets = [...new Set([...(exactTarget ? [exactTarget] : []), ...(job.resultTargetIds || [])])];
  const destination = resultDestination(job.jobType);
  const finished = ["needs_review", "completed"].includes(job.status);
  const events = (job.events || []).filter((event) => event.message?.trim());
  const phase = jobPhase(job);
  const latestActivityAt = events.at(-1)?.createdAt;
  const [retryEditorOpen, setRetryEditorOpen] = useState(false);
  const [actionError, setActionError] = useState("");
  const retryWithOriginalSettings = async () => {
    setActionError("");
    try {
      await api.retryJob({ jobId: job.id });
      onReload();
    } catch (value) {
      setActionError(errorMessage(value));
    }
  };
  return (
    <>
      <article className={`job-card ${failed ? "job-failed" : ""}`}>
        <div className="job-card-title">
          <div className="job-card-heading">
            <div className="job-card-heading-copy">
              <h3>{jobCardTitle(job)}</h3>
              <div><span className="job-type-label">{t(jobLabels[job.jobType] || job.jobType)}</span><span className="track-pill">{job.jobType === "internship_search" ? "Internship" : "Postdoc"}</span></div>
            </div>
          </div>
          <StatusBadge status={job.status} />
        </div>
        {job.status === "running" ? (
          <div className="job-live-status" role="status" aria-live="polite">
            <span><i /> {t("实时活动")}</span>
            <strong>{jobStatusMessage(job)}</strong>
          </div>
        ) : <p>{jobStatusMessage(job)}</p>}
        <div className="job-run-facts">
          <div><span>{t("当前阶段")}</span><strong>{phase.label}</strong><small>{phase.detail}</small></div>
          <div><span>{job.status === "running" ? t("已运行") : t("任务用时")}</span><strong>{jobDuration(job)}</strong><small>{job.startedAt ? t("开始于 {0}", formatLocalTime(job.startedAt)) : t("加入队列 {0}", formatLocalTime(job.createdAt))}</small></div>
          <div><span>{t("最近状态变更")}</span><strong>{relativeTime(latestActivityAt)}</strong><small>{latestActivityAt ? formatLocalTime(latestActivityAt) : t("尚无可显示的执行轨迹")}</small></div>
        </div>
        <JobRequestDetails job={job} open={failed} />
        {events.length > 0 && <JobActivityTimeline events={events} status={job.status} />}
        <div className="job-meta">
          <span>{t("创建于 {0}", formatLocalTime(job.createdAt))}</span>
          {isResultLimitedSearch(job.jobType) && <span>{t("结果上限 · {0} 条", resultLimitForJob(job))}</span>}
          {job.modelId && <span>{t("运行设置 · {0} · {1} · {2}", job.providerId, job.modelId, job.reasoning || t("默认推理"))}</span>}
        </div>
        {actionError && <div className="inline-notice error">{actionError}</div>}
        <div className="job-actions">
          {resultTargets.map((targetId, index) => (
            <button className="button ghost" key={targetId} onClick={() => onNavigate({ page: "application", targetId, careerSystem, tab: destination.tab, returnPage: "automation", jobId: job.id })}>
              {resultTargets.length > 1 ? `${destination.label} ${index + 1}` : destination.label}
            </button>
          ))}
          {finished && resultTargets.length === 0 && job.jobType === "preference_rebuild" && <button className="button ghost" onClick={() => onNavigate({ page: "settings" })}>{t("查看模型与偏好设置")}</button>}
          {finished && resultTargets.length === 0 && job.jobType !== "preference_rebuild" && <span className="job-result-note">{t("已完成，未产生申请卡片")}</span>}
          {isNative && job.status === "needs_review" && <button className="button primary" onClick={() => api.approveJob(job.id).then(onReload)}><Check size={16} /> {t("确认已审核")}</button>}
          {isNative && job.status === "running" && <button className="button ghost" onClick={() => api.cancelJob(job.id).then(onReload)}><CircleStop size={16} /> {t("取消")}</button>}
          {isNative && ["failed", "cancelled", "needs_review"].includes(job.status) && <>
            <button className="button ghost" onClick={() => void retryWithOriginalSettings()}><RotateCcw size={16} /> {t("按原设置重试")}</button>
            <button className="button primary" onClick={() => { setActionError(""); setRetryEditorOpen(true); }}><RotateCcw size={16} /> {t("调整后重新运行")}</button>
          </>}
        </div>
        <details className="technical-details"><summary>{t("技术详情")} <ChevronDown size={15} /></summary><pre>{JSON.stringify(job, null, 2)}</pre></details>
      </article>
      {retryEditorOpen && <RetryJobComposer job={job} onClose={() => setRetryEditorOpen(false)} onRetried={() => { setRetryEditorOpen(false); onReload(); }} />}
    </>
  );
}

function JobRequestDetails({ job, open }: { job: JobSummary; open: boolean }) {
  const hasRequest = Boolean(job.requestSummary?.trim());
  const hasPrompt = Boolean(job.prompt?.trim());
  return (
    <details className="job-request-details" open={open}>
      <summary>
        <span><Bot size={15} /> {t("本次任务")}</span>
        <small>{hasPrompt ? t("查看实际发送给 Agent 的指令") : t("查看本次保存的任务信息")}</small>
        <ChevronDown size={15} />
      </summary>
      <div className="job-request-body">
        <div>
          <span>{t("你的需求")}</span>
          <p>{hasRequest ? job.requestSummary : t("此任务没有单独记录文本需求。")}</p>
        </div>
        <div>
          <span>{t("实际 Agent 指令")}</span>
          {hasPrompt ? <pre>{job.prompt}</pre> : <p>{t("此任务没有单独的 Agent 指令。")}</p>}
        </div>
      </div>
    </details>
  );
}

function RetryJobComposer({ job, onClose, onRetried }: { job: JobSummary; onClose: () => void; onRetried: () => void }) {
  const [prompt, setPrompt] = useState(job.prompt || "");
  const [model, setModel] = useState<ModelSelection | undefined>(() => initialJobModel(job));
  const supportsResultLimit = isResultLimitedSearch(job.jobType);
  const [maxResults, setMaxResults] = useState(() => resultLimitForJob(job));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const request = buildRetryRequest(job, prompt, model, supportsResultLimit ? maxResults : undefined);
  const changedPrompt = request.prompt !== undefined;
  const changedModel = Boolean(request.providerId || request.modelId || request.reasoning);
  const changedResultLimit = request.maxResults !== undefined;
  const changedProvider = request.providerId !== undefined;
  const submit = async () => {
    if (job.prompt?.trim() && !prompt.trim()) {
      setError(t("实际 Agent 指令不能清空；如需恢复原设置，请关闭此窗口后选择“按原设置重试”。"));
      return;
    }
    setBusy(true);
    setError("");
    try {
      await api.retryJob(request);
      onRetried();
    } catch (value) {
      setError(errorMessage(value));
    } finally {
      setBusy(false);
    }
  };
  const changeNotice = job.jobType === "internship_search"
      ? t("实习任务未交付结果时会使用新线程重试；原画像快照保留。")
      : changedProvider
      ? t("已切换 Agent 服务：不同服务商无法访问原线程，会创建新的 Agent 线程。")
      : changedPrompt || changedModel || changedResultLimit
      ? t("已修改任务设置：会续用原 Agent 线程，并在已有证据基础上应用新设置。")
      : t("未修改任务设置：会按原设置继续，可在可用时恢复原 Agent 线程。");
  return (
    <div className="composer-backdrop" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose(); }}>
      <section className="task-composer retry-task-composer" role="dialog" aria-modal="true" aria-label={t("调整任务后重新运行")}>
        <div className="composer-heading"><div><span className="section-index">{t("重试")}</span><h2>{t("调整后重新运行")}</h2></div><button onClick={onClose}>{t("关闭")}</button></div>
        <p className="retry-task-title">{jobCardTitle(job)}</p>
        <label className="field">
          <span>{t("实际发送给 Agent 的指令")}</span>
          <textarea className="tall" value={prompt} onChange={(event) => setPrompt(event.target.value)} placeholder={t("为本次重试补充或调整 Agent 指令。")} />
        </label>
        {supportsResultLimit && <label className="field">
          <span>{t("本次最多返回并导入的机会数（1–5）")}</span>
          <input type="number" min={1} max={MAX_RESULT_LIMIT} value={maxResults} onChange={(event) => setMaxResults(clampResultLimit(Number(event.target.value)))} />
        </label>}
        <ModelControls taskType={modelTaskType(job.jobType)} value={model} onChange={setModel} />
        <div className="composer-safety retry-task-safety">{changeNotice}</div>
        {error && <div className="inline-notice error">{error}</div>}
        <div className="retry-task-actions">
          <button className="button ghost" disabled={busy} onClick={onClose}>{t("取消")}</button>
          <button className="button primary" disabled={busy} onClick={() => void submit()}><RotateCcw size={17} /> {busy ? t("正在重新加入队列…") : t("用这些设置重新运行")}</button>
        </div>
      </section>
    </div>
  );
}

function JobActivityTimeline({ events, status }: { events: JobSummary["events"]; status: string }) {
  const open = status === "running" || status === "failed";
  return (
    <details className="job-activity-timeline" open={open}>
      <summary>
        <span><Activity size={15} /> {t("执行轨迹")}</span>
        <small>{t("最近 {0} 条真实状态", events.length)}</small>
        <ChevronDown size={15} />
      </summary>
      <ol>
        {events.map((event, index) => {
          const current = status === "running" && index === events.length - 1;
          return (
            <li className={current ? "current" : ""} key={`${event.createdAt}-${index}`}>
              <span className="job-event-dot" />
              <div>
                <small>{jobEventLabel(event.eventType)} · {formatLocalTime(event.createdAt)}</small>
                <strong>{event.message}</strong>
              </div>
            </li>
          );
        })}
      </ol>
    </details>
  );
}

export function buildTaskHistory(jobs: Pick<JobGroups, "needsReview" | "recent">, limit: number): JobSummary[] {
  const unique = new Map<string, JobSummary>();
  for (const job of [...jobs.needsReview, ...jobs.recent]) unique.set(job.id, job);
  return [...unique.values()]
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt) || right.id.localeCompare(left.id))
    .slice(0, limit);
}

export function jobCardTitle(job: Pick<JobSummary, "jobType" | "requestSummary">): string {
  const request = compactTaskText(job.requestSummary);
  switch (job.jobType) {
    case "full_run":
    case "full_search":
      return request ? t("检索：{0}", request) : t("寻找匹配的 Postdoc 机会");
    case "internship_search":
      return request ? t("寻找 Internship：{0}", request) : t("寻找匹配的 Internship");
    case "research_pi":
      return request ? t("研究：{0}", request) : t("研究指定联系人或研究方向");
    case "pi_verification":
      return request ? t("核验联系人：{0}", request) : t("重新核验联系人");
    case "opportunity_health":
      return request ? t("核验：{0}", request) : t("检查机会是否仍然有效");
    case "follow_up_scan":
      return request ? t("扫描跟进：{0}", request) : t("扫描需要跟进的联系人");
    case "reply_followup":
      return request ? t("处理回复：{0}", request) : t("判断回复后的下一步");
    case "checklist_refresh":
      return t("更新申请清单");
    case "revision_request":
    case "material_revision":
      return request ? t("修订：{0}", request) : t("修订申请材料");
    case "prepare_application":
      return t("准备申请材料");
    case "preference_rebuild":
      return t("学习你的修改偏好");
    case "test_delay":
      return t("并发验证任务");
    default:
      return request ? t("任务：{0}", request) : t(jobLabels[job.jobType] || job.jobType);
  }
}

export function buildRetryRequest(
  job: Pick<JobSummary, "id" | "jobType" | "prompt" | "providerId" | "modelId" | "reasoning" | "maxResults">,
  prompt: string,
  model?: ModelSelection,
  maxResults?: number,
): RetryJobRequest {
  const request: RetryJobRequest = { jobId: job.id };
  const originalPrompt = job.prompt?.trim() || "";
  const nextPrompt = prompt.trim();
  if (nextPrompt !== originalPrompt) request.prompt = nextPrompt;
  if (model?.providerId && model.providerId !== job.providerId) request.providerId = model.providerId;
  if (model?.modelId && model.modelId !== job.modelId) request.modelId = model.modelId;
  if (model?.reasoning && model.reasoning !== job.reasoning) request.reasoning = model.reasoning;
  if (maxResults !== undefined && maxResults !== resultLimitForJob(job)) request.maxResults = maxResults;
  return request;
}

function isResultLimitedSearch(jobType: string): boolean {
  return jobType === "full_search" || jobType === "internship_search";
}

function clampResultLimit(value: number): number {
  return Math.min(MAX_RESULT_LIMIT, Math.max(1, Number.isFinite(value) ? Math.round(value) : DEFAULT_RESULT_LIMIT));
}

function resultLimitForJob(job: Pick<JobSummary, "maxResults">): number {
  return clampResultLimit(job.maxResults ?? DEFAULT_RESULT_LIMIT);
}

function compactTaskText(value?: string): string | undefined {
  const normalized = value?.replace(/\s+/g, " ").trim();
  if (!normalized) return undefined;
  return normalized.length > 96 ? `${normalized.slice(0, 95)}…` : normalized;
}

function initialJobModel(job: Pick<JobSummary, "providerId" | "modelId" | "reasoning">): ModelSelection | undefined {
  if (!job.providerId || !job.modelId || !job.reasoning) return undefined;
  return { providerId: job.providerId, modelId: job.modelId, reasoning: job.reasoning };
}

function modelTaskType(jobType: string): string {
  if (jobType === "internship_search" || jobType === "full_search" || jobType === "full_run") return "full_search";
  if (jobType === "research_pi") return "research_pi";
  if (jobType === "material_revision" || jobType === "revision_request") return "material_revision";
  if (jobType === "reply_followup") return "reply_followup";
  return "maintenance";
}

export function jobStatusMessage(job: JobSummary): string {
  if (job.status !== "failed") return job.message || t("任务正在处理。");
  if (job.error?.includes("Agent 没有生成") && job.error.includes("output/search-results.json")) {
    return t("模型线程已结束，但没有生成可导入的结果。按原设置重试会恢复原线程；也可以调整提示词或模型后重新运行。");
  }
  return job.error || t("任务失败。请查看本次任务和执行轨迹，再按原设置重试或调整后重新运行。");
}

export function jobPhase(job: Pick<JobSummary, "status" | "progress" | "events">): { label: string; detail: string } {
  if (job.status === "queued") return { label: t("等待调度器"), detail: t("正在队列中，空闲 Worker 会自动开始。") };
  if (job.status === "needs_review") return { label: t("等待你审核"), detail: t("结果已导入，确认后才会标记完成。") };
  if (job.status === "completed") return { label: t("已完成"), detail: t("任务结果已完成审核。") };
  if (job.status === "failed") return { label: t("需要处理"), detail: t("任务未完成；可查看失败原因或重新运行。") };
  if (job.status === "cancelled") return { label: t("已取消"), detail: t("任务已停止，不会继续占用 Worker。") };
  const hasAgentActivity = (job.events || []).some((event) => event.eventType === "activity");
  if (job.progress >= 92) return { label: t("校验并导入结果"), detail: t("正在检查输出并保存为可审核的结果。") };
  if (hasAgentActivity || job.progress >= 8) return { label: t("Agent 正在执行"), detail: t("模型正在检索、推理或整理任务结果。") };
  return { label: t("准备任务"), detail: t("正在建立独立工作区并启动 Agent。") };
}

function jobEventLabel(eventType: string): string {
  return ({
    queued: t("已入队"),
    running: t("开始执行"),
    progress: t("执行步骤"),
    activity: t("Agent 动态"),
    needs_review: t("等待审核"),
    completed: t("已完成"),
    failed: t("执行失败"),
    cancelled: t("已取消"),
    cancel_requested: t("请求取消"),
    retried: t("重新加入队列"),
  }[eventType] ?? t("状态更新"));
}

function parseJobTime(value?: string): number | undefined {
  if (!value) return undefined;
  const parsed = new Date(value.endsWith("Z") || value.includes("+") ? value : `${value}Z`).getTime();
  return Number.isNaN(parsed) ? undefined : parsed;
}

function relativeTime(value?: string, now = Date.now()): string {
  const timestamp = parseJobTime(value);
  if (!timestamp) return "—";
  const seconds = Math.max(0, Math.floor((now - timestamp) / 1000));
  if (seconds < 15) return t("刚刚");
  if (seconds < 60) return t("{0} 秒前", seconds);
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("{0} 分钟前", minutes);
  const hours = Math.floor(minutes / 60);
  return t("{0} 小时前", hours);
}

function jobDuration(job: Pick<JobSummary, "startedAt" | "finishedAt">, now = Date.now()): string {
  const startedAt = parseJobTime(job.startedAt);
  if (!startedAt) return t("尚未开始");
  const endedAt = parseJobTime(job.finishedAt) ?? now;
  const seconds = Math.max(0, Math.floor((endedAt - startedAt) / 1000));
  if (seconds < 60) return t("不足 1 分钟");
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("{0} 分钟", minutes);
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  return remainder ? t("{0} 小时 {1} 分钟", hours, remainder) : t("{0} 小时", hours);
}

function resultDestination(jobType: string): { tab: ApplicationTab; label: string } {
  switch (jobType) {
    case "internship_search": return { tab: "fit", label: t("查看 Internship 机会") };
    case "material_revision": return { tab: "revision", label: t("查看修订差异") };
    case "checklist_refresh": return { tab: "checklist", label: t("查看申请清单") };
    case "reply_followup":
    case "follow_up_scan": return { tab: "reply", label: t("查看回复处理") };
    case "pi_verification": return { tab: "pi", label: t("查看联系人简报") };
    case "opportunity_health": return { tab: "fit", label: t("查看核验结果") };
    default: return { tab: "cv", label: t("查看申请结果") };
  }
}

function QueueRow({ job, index, onReload }: { job: JobSummary; index: number; onReload: () => void }) {
  return <div className="queue-row"><strong>{index + 1}</strong><span className="queue-task"><b>{jobCardTitle(job)}</b><small>{t(jobLabels[job.jobType] || job.jobType)}</small></span><small>{formatLocalTime(job.createdAt)}</small><button onClick={() => api.cancelJob(job.id).then(onReload)}>{t("取消")}</button></div>;
}

function CompactEmpty({ text }: { text: string }) { return <div className="compact-empty"><Play size={17} /> {text}</div>; }

function TaskComposer({ type, onClose, onCreated }: { type: Exclude<ComposerType, null>; onClose: () => void; onCreated: () => void }) {
  const [query, setQuery] = useState("");
  const [threshold, setThreshold] = useState(75);
  const [maxResults, setMaxResults] = useState(DEFAULT_RESULT_LIMIT);
  const [model, setModel] = useState<ModelSelection>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (type !== "full_search") return;
    let active = true;
    api.onboardingProfile().then((profile) => {
      if (active) setQuery((current) => current || profile.goals || [profile.targetRoles, profile.targetRegions].filter(Boolean).join(" · "));
    }).catch(() => { /* An optional prefill must not prevent creating a task. */ });
    return () => { active = false; };
  }, [type]);
  const isPi = type === "research_pi";
  const isInternship = type === "internship_search";
  const supportsResultLimit = isResultLimitedSearch(type);
  const isHealth = type === "opportunity_health";
  const isScan = type === "follow_up_scan";

  useEffect(() => {
    if (!isInternship) return;
    let active = true;
    void api.internshipProfile()
      .then((profile) => {
        if (active) setQuery((current) => current.trim() ? current : buildInternshipSearchQuery(profile));
      })
      .catch(() => undefined);
    return () => { active = false; };
  }, [isInternship]);
  const submit = async () => {
    if (!isScan && !query.trim()) return;
    setBusy(true); setError("");
    try {
      await api.enqueue({
        jobType: type,
        targetType: isPi ? "person" : isHealth ? "verification" : isScan ? "contact_targets" : isInternship ? "internship" : "search",
        targetId: undefined,
        providerId: model?.providerId,
        modelId: model?.modelId,
        reasoning: model?.reasoning,
        payload: { query, ...(["full_search", "internship_search"].includes(type) ? { threshold, maxResults } : {}) },
        prompt: isInternship
          ? `Search for current industry internships matching this request: ${query}. Use official company career pages or official ATS records as primary evidence. Exclude postdoctoral, doctoral, faculty and regular full-time roles. Check hard eligibility requirements against available profile evidence; mark unknowns as uncertain. Return no more than ${maxResults} review-only structured opportunities and application checklists. Do not create a CV, contact anyone or submit an application.`
          : isPi
          ? `Research this named contact or researcher for current opportunities compatible with the candidate profile: ${query}. Use primary sources, verify identity, contact route, current direction and availability, deduplicate against existing opportunities, and return structured evidence. Do not contact anyone.`
          : isHealth
            ? `Verify whether these opportunity URLs or records remain active: ${query}. Use primary sources, record the check time and evidence, and return a review-only verification result. Do not archive records or change contact status.`
            : isScan
              ? `Review all contacted, replied, and follow-up contact targets in input/targets.json. Recommend only evidence-based next actions using exact target IDs. Additional instruction: ${query || "Identify which contacts are actually due for follow-up."} Do not change status, create drafts, or send email.`
            : `Run a complete evidence-based opportunity search for the candidate described in the imported local profile. Search request: ${query}. Verify current primary sources, apply the candidate's stated career-stage and constraint gates, score fit conservatively, deduplicate by source opportunity and independent contact target, save up to ${maxResults} matching contact targets first, then complete their materials in the next phase. Do not send email or submit applications.`,
      });
      onCreated();
    } catch (value) { setError(errorMessage(value)); }
    finally { setBusy(false); }
  };
  return (
    <div className="composer-backdrop" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose(); }}>
      <section className="task-composer">
        <div className="composer-heading"><div><span className="section-index">{t("新建")}</span><h2>{isInternship ? t("寻找 Internship") : isPi ? t("按姓名找机会") : isHealth ? t("检查机会") : isScan ? t("扫描跟进") : t("寻找 Postdoc 机会")}</h2></div><button onClick={onClose}>{t("关闭")}</button></div>
        <label className="field"><span>{isInternship ? t("目标岗位、地点和硬性条件") : isPi ? t("PI / 研究者姓名与线索") : isHealth ? t("要核验的机会、URL 或范围") : isScan ? t("补充要求（可选）") : t("本次检索要求")}</span><textarea className="tall" autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder={isInternship ? t("例如：目标岗位、地区、时间和其他硬性条件。") : isPi ? t("例如：研究者姓名、机构或研究方向。") : isHealth ? t("粘贴机会 URL，或说明要检查的机构与职位。") : isScan ? t("例如：优先检查超过 14 天没有回复的联系人。") : t("例如：目标地区、研究方向或机构范围。")} /></label>
        {["full_search", "internship_search"].includes(type) && <label className="field"><span>{t("严格匹配阈值（只保留大于该分数）")}</span><input type="number" min={0} max={99} value={threshold} onChange={(event) => setThreshold(Math.min(99, Math.max(0, Number(event.target.value) || 0)))} /></label>}
        {supportsResultLimit && <label className="field"><span>{t("本次最多返回并导入的机会数（1–5）")}</span><input type="number" min={1} max={MAX_RESULT_LIMIT} value={maxResults} onChange={(event) => setMaxResults(clampResultLimit(Number(event.target.value)))} /></label>}
        <ModelControls taskType={isPi ? "research_pi" : isHealth || isScan ? "maintenance" : "full_search"} value={model} onChange={setModel} />
        <div className="composer-safety">{t("任务会建立独立 Codex 线程。Postdoc 重试优先恢复原线程；实习未交付结果时新开线程。邮件发送和申请提交仍需手动确认。")}</div>
        {error && <div className="inline-notice error">{error}</div>}
        <button className="button primary wide" disabled={busy || (!isScan && !query.trim())} onClick={submit}><Play size={17} /> {busy ? t("正在加入队列…") : t("加入任务队列")}</button>
      </section>
    </div>
  );
}

export function buildInternshipSearchQuery(profile: InternshipProfile): string {
  const fields = [
    ["目标岗位 / 技能", profile.targetRoles],
    ["目标行业", profile.industries],
    ["目标地区", profile.regions],
    ["工作方式", profile.workMode],
    ["开始时间", profile.startDate],
    ["实习时长", profile.duration],
    ["工作许可", profile.workAuthorization],
    ["在读状态", profile.enrollmentStatus],
    ["限制条件", profile.constraints],
  ].filter(([, value]) => value.trim());
  if (fields.length === 0) return "寻找符合当前 Internship 画像的行业实习机会。";
  return `请寻找符合以下条件的行业 Internship：\n${fields.map(([label, value]) => `${label}：${value}`).join("\n")}`;
}
