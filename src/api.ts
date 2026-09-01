import { invoke } from "@tauri-apps/api/core";
import type {
  DashboardData,
  CvGenerationResult,
  CoverLetterGenerationResult,
  EnqueueRequest,
  JobGroups,
  GmailDraftInfo,
  GmailOAuthStart,
  GmailStatus,
  InboundReplyRequest,
  ManualRevisionRequest,
  MigrationReport,
  ProviderInfo,
  StatusFilter,
  TargetCard,
  TargetDetail,
  TaskModelDefault,
  ReplyItem,
  RevisionResult,
} from "./types";

export const api = {
  migration: () => invoke<MigrationReport>("get_migration_report"),
  dashboard: () => invoke<DashboardData>("get_dashboard"),
  targets: (status: StatusFilter, search = "", offset = 0, limit = 20) =>
    invoke<TargetCard[]>("get_contact_targets", {
      status,
      search: search || null,
      offset,
      limit,
    }),
  target: (targetId: string) =>
    invoke<TargetDetail>("get_contact_target", { targetId }),
  setStatus: (targetId: string, status: string) =>
    invoke<void>("set_contact_status", { targetId, status }),
  setSubmissionStatus: (targetId: string, status: string) =>
    invoke<void>("set_submission_status", { targetId, status }),
  readMaterial: (artifactPath: string) =>
    invoke<string>("read_material_text", { artifactPath }),
  pdfPreview: (artifactPath: string) =>
    invoke<string>("read_pdf_preview", { artifactPath }),
  saveManualMaterial: (request: ManualRevisionRequest) =>
    invoke<RevisionResult>("save_manual_material", { request }),
  saveInboundReply: (request: InboundReplyRequest) =>
    invoke<ReplyItem>("save_inbound_reply", { request }),
  generateTypstCv: (targetId: string) =>
    invoke<CvGenerationResult>("generate_typst_cv", { targetId }),
  generateCoverLetter: (targetId: string) =>
    invoke<CoverLetterGenerationResult>("generate_cover_letter", { targetId }),
  providers: () => invoke<ProviderInfo[]>("get_model_providers"),
  taskDefaults: () =>
    invoke<TaskModelDefault[]>("get_task_model_defaults"),
  saveTaskDefault: (value: TaskModelDefault) =>
    invoke<void>("save_task_model_default", { value }),
  jobs: (pageSize = 5) => invoke<JobGroups>("get_jobs", { pageSize }),
  enqueue: (request: EnqueueRequest) =>
    invoke<string>("enqueue_job", { request }),
  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),
  retryJob: (jobId: string) => invoke<void>("retry_job", { jobId }),
  approveJob: (jobId: string) => invoke<void>("approve_job", { jobId }),
  codexAccount: () => invoke<Record<string, unknown>>("get_codex_account"),
  codexModels: () => invoke<Record<string, unknown>>("get_codex_models"),
  connectChatGpt: () =>
    invoke<Record<string, unknown>>("connect_chatgpt"),
  waitForChatGptLogin: (loginId: string) =>
    invoke<Record<string, unknown>>("wait_for_chatgpt_login", { loginId }),
  saveOpenAiKey: (apiKey: string) =>
    invoke<Record<string, unknown>>("save_openai_api_key", { apiKey }),
  hasOpenAiKey: () => invoke<boolean>("has_openai_api_key"),
  removeOpenAiKey: () => invoke<void>("remove_openai_api_key"),
  gmailStatus: () => invoke<GmailStatus>("get_gmail_status"),
  importGmailClient: (path: string) =>
    invoke<void>("import_gmail_client", { path }),
  importLegacyGmail: () => invoke<boolean>("import_legacy_gmail"),
  startGmailOAuth: () => invoke<GmailOAuthStart>("start_gmail_oauth"),
  approveCv: (targetId: string) =>
    invoke<string>("approve_cv_for_gmail", { targetId }),
  cvApproval: (targetId: string) =>
    invoke<boolean>("get_cv_approval", { targetId }),
  createGmailDraft: (
    targetId: string,
    recipient: string,
    subject: string,
    body: string,
  ) => invoke<GmailDraftInfo>("create_gmail_draft", {
    targetId,
    recipient,
    subject,
    body,
  }),
  gmailDrafts: (targetId: string) =>
    invoke<GmailDraftInfo[]>("list_gmail_drafts", { targetId }),
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "发生未知错误";
}
