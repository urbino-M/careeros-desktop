export type ContactStatus =
  | "ready_to_contact"
  | "contacted"
  | "replied"
  | "follow_up"
  | "shelved";

export type SubmissionStatus =
  | "not_set"
  | "portal_pending"
  | "submitted"
  | "not_required";

export type StatusFilter = ContactStatus | "all";
export type CareerSystem = "postdoc" | "internship";
export type SearchChannel = "web_ats" | "exa" | "rss" | "linkedin" | "facebook" | "twitter";
export type VerificationStatus = "verified" | "unverified";
export type ApplicationFilter = StatusFilter | SubmissionStatus | VerificationStatus;

export interface MigrationReport {
  imported: boolean;
  legacyRoot?: string;
  sourceSha256?: string;
  sourceSha256After?: string;
  backupPath?: string;
  applications: number;
  opportunities: number;
  legacyJobs: number;
  revisions: number;
  gmailDrafts: number;
  activeTargets: number;
  hiddenTombstones: number;
}

export interface DashboardMetric {
  key: string;
  label: string;
  value: number;
  helper: string;
}

export interface RegionCount {
  region: string;
  count: number;
}

export interface TargetCard {
  id: string;
  applicationId: string;
  opportunityId?: string;
  name: string;
  email?: string;
  organization: string;
  title: string;
  country?: string;
  region?: string;
  fitScore?: number;
  priority: number;
  status: ContactStatus;
  submissionStatus: SubmissionStatus;
  deadline?: string;
  sourceUrl?: string;
  updatedAt: string;
  careerTrack: "postdoc" | "internship";
  verificationStatus: VerificationStatus;
  sourceChannel: SearchChannel;
  sourceBackend: string;
}

export interface DashboardData {
  metrics: DashboardMetric[];
  regions: RegionCount[];
  priorityTargets: TargetCard[];
}

export interface ArtifactItem {
  artifactType: string;
  language: string;
  path: string;
  exists: boolean;
  updatedAt: string;
}

export interface ChecklistItem {
  id: string;
  itemType: string;
  required: boolean;
  status: string;
  evidence?: string;
  note?: string;
}

export interface ReplyItem {
  id: string;
  sender?: string;
  subject?: string;
  body: string;
  receivedAt?: string;
  createdAt: string;
}

export interface InboundReplyRequest {
  targetId: string;
  sender?: string;
  subject?: string;
  body: string;
  receivedAt?: string;
}

export interface DiffEntry {
  line: number;
  before: string;
  after: string;
}

export interface RevisionResult {
  revisionId: string;
  artifactPath: string;
  backupPath: string;
  summary: string;
  locations: string[];
  diff: DiffEntry[];
}

export interface ManualRevisionRequest {
  targetId: string;
  artifactType: string;
  language: string;
  content: string;
  note?: string;
}

export interface CvGenerationResult {
  pdfPath: string;
  backupPath?: string;
  pageCount: number;
  previousPageCount?: number;
  sourceSha256: string;
  fontPolicy: string;
  revision: RevisionResult;
}

export interface CoverLetterGenerationResult {
  pdfPath: string;
  sourcePath: string;
  textPath: string;
  backupPath?: string;
  pageCount: number;
  sourceSha256: string;
  revision: RevisionResult;
}

export interface RevisionItem {
  id: string;
  artifactType: string;
  language: string;
  artifactPath: string;
  backupPath?: string;
  jobId?: string;
  editor: string;
  note?: string;
  summary?: string;
  locationsJson?: string;
  diffJson?: string;
  providerId?: string;
  modelId?: string;
  reasoning?: string;
  createdAt: string;
}

export interface TargetDetail {
  target: TargetCard;
  summary?: string;
  department?: string;
  piResearchSummary?: string;
  applicationNotes?: string;
  artifacts: ArtifactItem[];
  checklist: ChecklistItem[];
  replies: ReplyItem[];
  revisions: RevisionItem[];
  sources: SourceEvidence[];
}

export interface SourceEvidence {
  title: string;
  url: string;
  checkedAt: string;
  evidenceType: "primary" | "secondary" | "inferred" | string;
  channel: SearchChannel;
  backend: string;
}

export interface ChannelHealth {
  channel: SearchChannel;
  backend: string;
  available: boolean;
  authenticated: boolean;
  status: string;
  message: string;
  checkedAt: string;
}

export interface SearchCapabilities {
  checkedAt: string;
  channels: ChannelHealth[];
  warnings: string[];
}

export interface SearchSetupPlan {
  checkedAt: string;
  channels: SearchChannel[];
  commands: string[];
  manualSteps: string[];
}

export interface SearchSetupResult {
  completed: boolean;
  messages: string[];
  capabilities: SearchCapabilities;
}

export interface AuthGuide {
  channel: SearchChannel;
  title: string;
  url?: string;
  instructions: string[];
}

export interface ProviderModelInfo {
  id: string;
  slug: string;
  displayName: string;
  enabled: boolean;
  supportsReasoning: boolean;
  supportsTools: boolean;
  supportsVision: boolean;
  reasoningLevels: string[];
}

export interface ProviderInfo {
  id: string;
  displayName: string;
  adapterKind: string;
  connectionMode: string;
  enabled: boolean;
  baseUrl?: string;
  configured: boolean;
  lastValidatedAt?: string;
  validationMessage?: string;
  models: ProviderModelInfo[];
}

export interface ProviderConnectionRequest {
  baseUrl: string;
  apiKey: string;
}

export interface TaskModelDefault {
  taskType: string;
  providerId: string;
  modelId: string;
  reasoning: string;
}

export interface CvCustomizationSettings {
  schemaVersion: number;
  enabled: boolean;
  emphasize: string;
  exclude: string;
  instructions: string;
  updatedAt?: string;
}

export interface OnboardingProfile {
  schemaVersion: number;
  completed: boolean;
  currentStep: number;
  fullName: string;
  publicationName: string;
  careerStage: string;
  discipline: string;
  currentSituation: string;
  targetRoles: string;
  targetRegions: string;
  goals: string;
  constraints: string;
  preferredLanguage: "zh" | "en" | "bilingual";
  cvSourceFile?: string;
  updatedAt?: string;
}

export interface InternshipProfile {
  schemaVersion: number;
  targetRoles: string;
  industries: string;
  regions: string;
  workMode: string;
  startDate: string;
  duration: string;
  workAuthorization: string;
  enrollmentStatus: string;
  constraints: string;
  cvPath?: string;
  rssFeeds: string[];
  updatedAt?: string;
}

export interface JobSummary {
  id: string;
  jobType: string;
  targetId?: string;
  resultTargetIds: string[];
  status: string;
  progress: number;
  message?: string;
  providerId: string;
  accountId?: string;
  modelId?: string;
  reasoning?: string;
  threadId?: string;
  error?: string;
  createdAt: string;
  startedAt?: string;
  finishedAt?: string;
}

export interface JobGroups {
  running: JobSummary[];
  queued: JobSummary[];
  needsReview: JobSummary[];
  needsReviewTotal: number;
  recent: JobSummary[];
  recentTotal: number;
  capacity: number;
}

export interface GmailStatus {
  configured: boolean;
  connected: boolean;
  accountEmail?: string;
  connectionOk: boolean;
  oauthStatus: "idle" | "pending" | "connected" | "failed";
  oauthMessage?: string;
}

export interface GmailOAuthStart {
  authorizationUrl: string;
  redirectUri: string;
}

export interface GmailDraftInfo {
  id: string;
  targetId: string;
  gmailDraftId: string;
  gmailMessageId?: string;
  recipient: string;
  subject: string;
  cvPath: string;
  remoteVerified: boolean;
  createdAt: string;
  gmailUrl: string;
}

export interface EnqueueRequest {
  jobType: string;
  targetType?: string;
  targetId?: string;
  prompt?: string;
  payload?: Record<string, unknown>;
  providerId?: string;
  accountId?: string;
  modelId?: string;
  reasoning?: string;
  threadId?: string;
}

export type ApplicationTab =
  | "cv"
  | "cover_letter"
  | "checklist"
  | "email_en"
  | "email_zh"
  | "fit"
  | "pi"
  | "revision"
  | "reply"
  | "other";

export type ApplicationView = "opportunities" | "strategy";

export type AppRoute =
  | { page: "dashboard" }
  | { page: "automation" }
  | { page: "applications"; careerSystem: CareerSystem; status: ApplicationFilter; view?: ApplicationView }
  | { page: "application"; targetId: string; careerSystem: CareerSystem; tab?: ApplicationTab; returnPage?: "automation"; jobId?: string }
  | { page: "settings" };

export type Locale = "zh" | "en";
