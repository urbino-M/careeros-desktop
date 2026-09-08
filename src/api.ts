import { invoke } from "@tauri-apps/api/core";
import type {
  DashboardData,
  InternshipProfile,
  CvGenerationResult,
  CoverLetterGenerationResult,
  CareerSystem,
  ApplicationFilter,
  CvCustomizationSettings,
  EnqueueRequest,
  JobGroups,
  GmailDraftInfo,
  GmailOAuthStart,
  GmailStatus,
  InboundReplyRequest,
  ManualRevisionRequest,
  MigrationReport,
  OnboardingProfile,
  ProviderInfo,
  ProviderConnectionRequest,
  TargetCard,
  TargetDetail,
  TaskModelDefault,
  ReplyItem,
  RevisionResult,
} from "./types";

export const api = {
  migration: () => invoke<MigrationReport>("get_migration_report"),
  dashboard: (careerTrack: CareerSystem) => invoke<DashboardData>("get_dashboard", { careerTrack }),
  targets: (careerTrack: CareerSystem, status: ApplicationFilter, search = "", offset = 0, limit = 20) =>
    invoke<TargetCard[]>("get_contact_targets", {
      careerTrack,
      status: careerTrack === "postdoc" ? status : null,
      submissionStatus: careerTrack === "internship" && ["not_set", "portal_pending", "submitted", "not_required"].includes(status) ? status : null,
      verificationStatus: careerTrack === "internship"
        ? status === "unverified" ? "unverified" : status === "all" ? null : "verified"
        : null,
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
  connectResponsesProvider: (request: ProviderConnectionRequest) =>
    invoke<ProviderInfo>("connect_responses_provider", { request }),
  disconnectResponsesProvider: (providerId: string) =>
    invoke<void>("disconnect_responses_provider", { providerId }),
  taskDefaults: () =>
    invoke<TaskModelDefault[]>("get_task_model_defaults"),
  saveTaskDefault: (value: TaskModelDefault) =>
    invoke<void>("save_task_model_default", { value }),
  cvCustomization: () =>
    invoke<CvCustomizationSettings>("get_cv_customization"),
  saveCvCustomization: (value: CvCustomizationSettings) =>
    invoke<CvCustomizationSettings>("save_cv_customization", { value }),
  onboardingProfile: () =>
    invoke<OnboardingProfile>("get_onboarding_profile"),
  saveOnboardingProfile: (value: OnboardingProfile) =>
    invoke<OnboardingProfile>("save_onboarding_profile", { value }),
  importOnboardingCv: (path: string) =>
    invoke<string>("import_onboarding_cv", { path }),
  importInternshipCv: (path: string) =>
    invoke<string>("import_internship_cv", { path }),
  internshipProfile: () => invoke<InternshipProfile>("get_internship_profile"),
  saveInternshipProfile: (value: InternshipProfile) =>
    invoke<InternshipProfile>("save_internship_profile", { value }),
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
  gmailStatus: () => invoke<GmailStatus>("get_gmail_status"),
  importGmailClient: (path: string) =>
    invoke<void>("import_gmail_client", { path }),
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
