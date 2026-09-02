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
import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "../api";
import { ModelControls, type ModelSelection } from "../components/ModelControls";
import { ErrorState, LoadingState, StatusBadge, formatLocalTime, jobLabels } from "../components/Ui";
import type { ApplicationTab, AppRoute, JobGroups, JobSummary } from "../types";

type ComposerType = "internship_search" | "full_search" | "research_pi" | "opportunity_health" | "follow_up_scan" | null;

export function AutomationPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  const [jobs, setJobs] = useState<JobGroups>();
  const [error, setError] = useState("");
  const [historySize, setHistorySize] = useState(5);
  const [composer, setComposer] = useState<ComposerType>(null);
  const [notice, setNotice] = useState("");
  const load = () => api.jobs(historySize).then(setJobs).catch((value) => setError(errorMessage(value)));

  useEffect(() => {
    load();
    const timer = window.setInterval(load, 2000);
    let unlisten: (() => void) | undefined;
    listen("careeros://jobs-changed", load).then((value) => (unlisten = value));
    return () => { window.clearInterval(timer); unlisten?.(); };
  }, [historySize]);

  const taskHistory = useMemo(() => {
    if (!jobs) return [];
    return buildTaskHistory(jobs, historySize);
  }, [jobs, historySize]);

  const visibleJobs = jobs;

  return (
    <div className="page automation-page">
      <button className="back-button" onClick={() => onNavigate({ page: "dashboard" })}>
        <ArrowLeft size={17} /> 返回仪表盘
      </button>
      <header className="page-header automation-header">
        <div className="eyebrow">NATIVE AGENT CONTROL</div>
        <h1>Agent 运行中心</h1>
        <p>统一启动研究机会与 Internship 检索，查看实时进度，并处理需要你确认的结果。最多 5 个任务并行，第 6 个自动排队。</p>
        <div className="quick-actions">
          <button className="button primary" onClick={() => setComposer("full_search")}><Plus size={17} /> 寻找 Postdoc 机会</button>
          <button className="button secondary" onClick={() => setComposer("internship_search")}><BriefcaseBusiness size={17} /> 寻找 Internship</button>
          <button className="button secondary" onClick={() => setComposer("research_pi")}><UserSearch size={17} /> 按姓名找机会</button>
          <button className="button secondary" onClick={() => setComposer("opportunity_health")}><SearchCheck size={17} /> 检查机会</button>
          <button className="button secondary" onClick={() => onNavigate({ page: "applications", careerSystem: "postdoc", status: "all" })}><ListRestart size={17} /> 查看 Postdoc 申请</button>
          <button className="button secondary" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status: "all" })}><ListRestart size={17} /> 查看 Internship 申请</button>
          <button className="button secondary" onClick={() => setComposer("follow_up_scan")}><Clock3 size={17} /> 扫描跟进</button>
          <button className="button secondary" onClick={async () => {
            try {
              const id = await api.enqueue({ jobType: "preference_rebuild", targetType: "preferences", payload: { source: "recorded_revisions" } });
              setNotice(`偏好学习已加入：${id}`); load();
            } catch (value) { setNotice(errorMessage(value)); }
          }}><Bot size={17} /> 更新偏好</button>
        </div>
      </header>

      {notice && <div className="inline-notice">{notice}</div>}

      {composer && <TaskComposer type={composer} onClose={() => setComposer(null)} onCreated={() => { setComposer(null); load(); }} />}

      {error && <ErrorState message={error} retry={load} />}
      {!error && !jobs && <LoadingState label="正在读取任务调度器" />}
      {visibleJobs && (
        <>
          <div className="worker-strip">
            <span><i className="pulse-dot" /> Worker 在线</span>
            <span>并发 {visibleJobs.running.length} / {visibleJobs.capacity}</span>
            <span>排队 {visibleJobs.queued.length}</span>
            <span>待审核 {visibleJobs.needsReviewTotal}</span>
          </div>

          <JobSection title={`当前运行 · ${visibleJobs.running.length}`} icon={Activity} open>
            {visibleJobs.running.length ? visibleJobs.running.map((job) => <JobCard job={job} key={job.id} onReload={load} onNavigate={onNavigate} />) : <CompactEmpty text="当前没有任务在执行；调度器会自动领取队列中的下一项。" />}
          </JobSection>

          <JobSection title={`等待队列 · ${visibleJobs.queued.length}`} icon={Clock3}>
            {visibleJobs.queued.length ? visibleJobs.queued.map((job, index) => <QueueRow job={job} index={index} key={job.id} onReload={load} />) : <CompactEmpty text="队列为空。" />}
          </JobSection>

          <JobSection title={`任务记录 · ${visibleJobs.recentTotal}（待处理 ${visibleJobs.needsReviewTotal}）`} icon={History} open>
            {taskHistory.length ? taskHistory.map((job) => <JobCard job={job} key={job.id} onReload={load} onNavigate={onNavigate} />) : <CompactEmpty text="尚无任务记录。" />}
            {visibleJobs.recentTotal > historySize && <button className="load-more" onClick={() => setHistorySize((value) => value + 5)}>再展开 5 条</button>}
          </JobSection>
        </>
      )}
    </div>
  );
}

function JobSection({ title, icon: Icon, open = false, children }: { title: string; icon: typeof Activity; open?: boolean; children: React.ReactNode }) {
  return (
    <details className="job-section" open={open}>
      <summary><span><Icon size={18} /> {title}</span><ChevronDown size={18} /></summary>
      <div className="job-section-body">{children}</div>
    </details>
  );
}

function JobCard({ job, onReload, onNavigate }: { job: JobSummary; onReload: () => void; onNavigate: (route: AppRoute) => void }) {
  const failed = job.status === "failed";
  const isNative = job.providerId !== "legacy";
  const careerSystem = job.jobType === "internship_search" ? "internship" : "postdoc";
  const exactTarget = job.targetId?.startsWith("target") ? job.targetId : undefined;
  const resultTargets = [...new Set([...(exactTarget ? [exactTarget] : []), ...(job.resultTargetIds || [])])];
  const destination = resultDestination(job.jobType);
  const finished = ["needs_review", "completed"].includes(job.status);
  return (
    <article className={`job-card ${failed ? "job-failed" : ""}`}>
      <div className="job-card-title"><div className="job-card-heading"><h3>{jobLabels[job.jobType] || job.jobType}</h3><span className="track-pill">{job.jobType === "internship_search" ? "Internship" : "Postdoc"}</span></div><StatusBadge status={job.status} /></div>
      {job.status === "running" ? (
        <div className="job-live-status" role="status" aria-live="polite">
          <span><i /> 实时活动</span>
          <strong>{jobStatusMessage(job)}</strong>
        </div>
      ) : <p>{jobStatusMessage(job)}</p>}
      <div className="job-meta"><span>{formatLocalTime(job.createdAt)}</span><span>{job.id}</span>{job.modelId && <span>{job.providerId} · {job.accountId || "默认账号"} · {job.modelId} · {job.reasoning}</span>}{job.threadId && <span>会话 {job.threadId}</span>}</div>
      <div className="job-actions">
        {resultTargets.map((targetId, index) => (
          <button className="button ghost" key={targetId} onClick={() => onNavigate({ page: "application", targetId, careerSystem, tab: destination.tab, returnPage: "automation", jobId: job.id })}>
            {resultTargets.length > 1 ? `${destination.label} ${index + 1}` : destination.label}
          </button>
        ))}
        {finished && resultTargets.length === 0 && job.jobType === "preference_rebuild" && <button className="button ghost" onClick={() => onNavigate({ page: "settings" })}>查看模型与偏好设置</button>}
        {finished && resultTargets.length === 0 && job.jobType !== "preference_rebuild" && <span className="job-result-note">已完成，未产生申请卡片</span>}
        {isNative && job.status === "needs_review" && <button className="button primary" onClick={() => api.approveJob(job.id).then(onReload)}><Check size={16} /> 确认已审核</button>}
        {isNative && job.status === "running" && <button className="button ghost" onClick={() => api.cancelJob(job.id).then(onReload)}><CircleStop size={16} /> 取消</button>}
        {isNative && ["failed", "cancelled"].includes(job.status) && <button className="button ghost" onClick={() => api.retryJob(job.id).then(onReload)}><RotateCcw size={16} /> 重新运行</button>}
      </div>
      <details className="technical-details"><summary>技术详情 <ChevronDown size={15} /></summary><pre>{JSON.stringify(job, null, 2)}</pre></details>
    </article>
  );
}

export function buildTaskHistory(jobs: Pick<JobGroups, "needsReview" | "recent">, limit: number): JobSummary[] {
  const unique = new Map<string, JobSummary>();
  for (const job of [...jobs.needsReview, ...jobs.recent]) unique.set(job.id, job);
  return [...unique.values()]
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt) || right.id.localeCompare(left.id))
    .slice(0, limit);
}

export function jobStatusMessage(job: JobSummary): string {
  if (job.status !== "failed") return job.message || "任务正在处理。";
  if (job.error?.includes("Agent 没有生成") && job.error.includes("output/search-results.json")) {
    return "模型线程已结束，但没有生成可导入的完整检索结果。点击“重新运行”会恢复原线程继续完成。";
  }
  return job.error || "任务失败，展开技术详情查看原因。";
}

function resultDestination(jobType: string): { tab: ApplicationTab; label: string } {
  switch (jobType) {
    case "internship_search": return { tab: "fit", label: "查看 Internship 机会" };
    case "material_revision": return { tab: "revision", label: "查看修订差异" };
    case "checklist_refresh": return { tab: "checklist", label: "查看申请清单" };
    case "reply_followup":
    case "follow_up_scan": return { tab: "reply", label: "查看回复处理" };
    case "pi_verification": return { tab: "pi", label: "查看联系人简报" };
    case "opportunity_health": return { tab: "fit", label: "查看核验结果" };
    default: return { tab: "cv", label: "查看申请结果" };
  }
}

function QueueRow({ job, index, onReload }: { job: JobSummary; index: number; onReload: () => void }) {
  return <div className="queue-row"><strong>{index + 1}</strong><span>{jobLabels[job.jobType] || job.jobType}</span><small>{formatLocalTime(job.createdAt)}</small><button onClick={() => api.cancelJob(job.id).then(onReload)}>取消</button></div>;
}

function CompactEmpty({ text }: { text: string }) { return <div className="compact-empty"><Play size={17} /> {text}</div>; }

function TaskComposer({ type, onClose, onCreated }: { type: Exclude<ComposerType, null>; onClose: () => void; onCreated: () => void }) {
  const [query, setQuery] = useState("");
  const [threshold, setThreshold] = useState(75);
  const [model, setModel] = useState<ModelSelection>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const isPi = type === "research_pi";
  const isInternship = type === "internship_search";
  const isHealth = type === "opportunity_health";
  const isScan = type === "follow_up_scan";
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
        payload: { query, ...(["full_search", "internship_search"].includes(type) ? { threshold } : {}) },
        prompt: isInternship
          ? `Search for current industry internships matching this request: ${query}. Use official company career pages or official ATS records as primary evidence. Exclude postdoctoral, doctoral, faculty and regular full-time roles. Check hard eligibility requirements against available profile evidence; mark unknowns as uncertain. Return review-only structured opportunities and application checklists. Do not create a CV, contact anyone or submit an application.`
          : isPi
          ? `Research this named contact or researcher for current opportunities compatible with the candidate profile: ${query}. Use primary sources, verify identity, contact route, current direction and availability, deduplicate against existing opportunities, and return structured evidence. Do not contact anyone.`
          : isHealth
            ? `Verify whether these opportunity URLs or records remain active: ${query}. Use primary sources, record the check time and evidence, and return a review-only verification result. Do not archive records or change contact status.`
            : isScan
              ? `Review all contacted, replied, and follow-up contact targets in input/targets.json. Recommend only evidence-based next actions using exact target IDs. Additional instruction: ${query || "Identify which contacts are actually due for follow-up."} Do not change status, create drafts, or send email.`
            : `Run a complete evidence-based opportunity search for the candidate described in the imported local profile. Search request: ${query}. Verify current primary sources, apply the candidate's stated career-stage and constraint gates, score fit conservatively, deduplicate by source opportunity and independent contact target, then prepare no more than five complete reviewable material packages. Do not send email or submit applications.`,
      });
      onCreated();
    } catch (value) { setError(errorMessage(value)); }
    finally { setBusy(false); }
  };
  return (
    <div className="composer-backdrop" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose(); }}>
      <section className="task-composer">
        <div className="composer-heading"><div><span className="section-index">NEW</span><h2>{isInternship ? "寻找 Internship" : isPi ? "按姓名找机会" : isHealth ? "检查机会" : isScan ? "扫描跟进" : "寻找 Postdoc 机会"}</h2></div><button onClick={onClose}>关闭</button></div>
        <label className="field"><span>{isInternship ? "目标岗位、地点和硬性条件" : isPi ? "PI / 研究者姓名与线索" : isHealth ? "要核验的机会、URL 或范围" : isScan ? "补充要求（可选）" : "本次检索要求"}</span><textarea className="tall" autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder={isInternship ? "例如：目标岗位、地区、时间和其他硬性条件。" : isPi ? "例如：研究者姓名、机构或研究方向。" : isHealth ? "粘贴机会 URL，或说明要检查的机构与职位。" : isScan ? "例如：优先检查超过 14 天没有回复的联系人。" : "例如：目标地区、研究方向或机构范围。"} /></label>
        {["full_search", "internship_search"].includes(type) && <label className="field"><span>严格匹配阈值（只保留大于该分数）</span><input type="number" min={0} max={99} value={threshold} onChange={(event) => setThreshold(Math.min(99, Math.max(0, Number(event.target.value) || 0)))} /></label>}
        <ModelControls taskType={isPi ? "research_pi" : isHealth || isScan ? "maintenance" : "full_search"} value={model} onChange={setModel} />
        <div className="composer-safety">任务会建立独立 Codex 线程；重试恢复原线程。任何邮件发送和申请提交仍需你手动确认。</div>
        {error && <div className="inline-notice error">{error}</div>}
        <button className="button primary wide" disabled={busy || (!isScan && !query.trim())} onClick={submit}><Play size={17} /> {busy ? "正在加入队列…" : "加入任务队列"}</button>
      </section>
    </div>
  );
}
