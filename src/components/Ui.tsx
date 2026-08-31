import { AlertTriangle, LoaderCircle } from "lucide-react";
import type { ContactStatus } from "../types";

export const statusLabels: Record<ContactStatus, string> = {
  ready_to_contact: "待联系",
  contacted: "已联系",
  replied: "已回复",
  follow_up: "跟进",
};

export const jobLabels: Record<string, string> = {
  full_run: "完整检索与申请",
  full_search: "完整检索与申请",
  research_pi: "按姓名找机会",
  opportunity_health: "机会存活检查",
  checklist_refresh: "申请清单刷新",
  follow_up_scan: "扫描跟进",
  preference_rebuild: "偏好学习",
  revision_request: "材料修订",
  material_revision: "材料修订",
  reply_followup: "回复后续处理",
  prepare_application: "准备申请材料",
  test_delay: "并发验证",
};

export function StatusBadge({ status }: { status: string }) {
  const label = statusLabels[status as ContactStatus] ?? translateJobStatus(status);
  return <span className={`status-badge status-${status}`}>{label}</span>;
}

export function translateJobStatus(status: string) {
  return (
    {
      queued: "排队中",
      running: "运行中",
      needs_review: "待审核",
      completed: "已完成",
      failed: "失败",
      cancelled: "已取消",
    }[status] ?? status
  );
}

export function LoadingState({ label = "正在读取本机数据" }: { label?: string }) {
  return (
    <div className="state-panel">
      <LoaderCircle className="spin" size={22} />
      <span>{label}</span>
    </div>
  );
}

export function ErrorState({ message, retry }: { message: string; retry?: () => void }) {
  return (
    <div className="state-panel error-state">
      <AlertTriangle size={22} />
      <div><strong>暂时无法读取</strong><div>{message}</div></div>
      {retry && <button className="button secondary" onClick={retry}>重试</button>}
    </div>
  );
}

export function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="empty-state">
      <div className="empty-orbit" />
      <h3>{title}</h3>
      <p>{body}</p>
    </div>
  );
}

export function formatLocalTime(value?: string) {
  if (!value) return "—";
  const date = new Date(value.endsWith("Z") || value.includes("+") ? value : `${value}Z`);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}
