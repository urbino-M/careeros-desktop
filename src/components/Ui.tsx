import { dateLocale, t } from "../i18n";
import { AlertTriangle, LoaderCircle } from "lucide-react";
import { useState } from "react";
import type { ContactStatus, SubmissionStatus, VerificationStatus, SearchChannel } from "../types";

export type UiNotice = string | (() => string);

export function uiNotice(source: string, ...values: Array<string | number | (() => string)>): UiNotice {
  return () => t(source, ...values.map(value => typeof value === "function" ? value() : value));
}

export function resolveUiNotice(notice: UiNotice): string {
  return typeof notice === "function" ? notice() : t(notice);
}

export function useUiNotice() {
  const [notice, setNotice] = useState<UiNotice>("");
  return [resolveUiNotice(notice), (next: UiNotice) => setNotice(() => next)] as const;
}

export const statusLabels: Record<ContactStatus, string> = {
  ready_to_contact: "待处理",
  contacted: "已联系",
  replied: "已回复",
  follow_up: "跟进",
  shelved: "搁置",
};

export const submissionStatusLabels: Record<SubmissionStatus, string> = {
  not_set: "未开始",
  portal_pending: "官网待投递",
  submitted: "已投递",
  not_required: "无需投递",
};

export const verificationStatusLabels: Record<VerificationStatus, string> = {
  verified: "已核验",
  unverified: "待核验",
};

export const searchChannelLabels: Record<SearchChannel, string> = {
  web_ats: "官方 Web / ATS",
  exa: "Exa",
  rss: "RSS",
  linkedin: "LinkedIn",
  facebook: "Facebook",
  twitter: "Twitter / X",
};

export const jobLabels: Record<string, string> = {
  internship_search: "Internship 机会检索",
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
  return <span className={`status-badge status-${status}`}>{t(label)}</span>;
}

export function SubmissionBadge({ status }: { status: SubmissionStatus }) {
  return <span className={`submission-badge submission-${status}`}>{t(submissionStatusLabels[status])}</span>;
}

export function VerificationBadge({ status }: { status: VerificationStatus }) {
  return <span className={`verification-badge verification-${status}`}>{t(verificationStatusLabels[status])}</span>;
}

export function translateJobStatus(status: string) {
  return t(
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
      <span>{t(label)}</span>
    </div>
  );
}

export function ErrorState({ message, retry }: { message: string; retry?: () => void }) {
  return (
    <div className="state-panel error-state">
      <AlertTriangle size={22} />
      <div><strong>{t("暂时无法读取")}</strong><div>{t(message)}</div></div>
      {retry && <button className="button secondary" onClick={retry}>{t("重试")}</button>}
    </div>
  );
}

export function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="empty-state">
      <div className="empty-orbit" />
      <h3>{t(title)}</h3>
      <p>{t(body)}</p>
    </div>
  );
}

export function formatLocalTime(value?: string) {
  if (!value) return "—";
  const date = new Date(value.endsWith("Z") || value.includes("+") ? value : `${value}Z`);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(dateLocale(), {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}
