use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub imported: bool,
    pub legacy_root: Option<String>,
    pub source_sha256: Option<String>,
    pub source_sha256_after: Option<String>,
    pub backup_path: Option<String>,
    pub applications: i64,
    pub opportunities: i64,
    pub legacy_jobs: i64,
    pub revisions: i64,
    pub gmail_drafts: i64,
    pub active_targets: i64,
    pub hidden_tombstones: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ContactStatus {
    ReadyToContact,
    Contacted,
    Replied,
    FollowUp,
    Shelved,
}

#[allow(dead_code)]
impl ContactStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadyToContact => "ready_to_contact",
            Self::Contacted => "contacted",
            Self::Replied => "replied",
            Self::FollowUp => "follow_up",
            Self::Shelved => "shelved",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardMetric {
    pub key: String,
    pub label: String,
    pub value: i64,
    pub helper: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionCount {
    pub region: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetCard {
    pub id: String,
    pub application_id: String,
    pub opportunity_id: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub organization: String,
    pub title: String,
    pub country: Option<String>,
    pub region: Option<String>,
    pub fit_score: Option<f64>,
    pub priority: i64,
    pub status: String,
    pub submission_status: String,
    pub deadline: Option<String>,
    pub source_url: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    pub metrics: Vec<DashboardMetric>,
    pub regions: Vec<RegionCount>,
    pub priority_targets: Vec<TargetCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactItem {
    pub artifact_type: String,
    pub language: String,
    pub path: String,
    pub exists: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub id: String,
    pub item_type: String,
    pub required: bool,
    pub status: String,
    pub evidence: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyItem {
    pub id: String,
    pub sender: Option<String>,
    pub subject: Option<String>,
    pub body: String,
    pub received_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundReplyRequest {
    pub target_id: String,
    pub sender: Option<String>,
    pub subject: Option<String>,
    pub body: String,
    pub received_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionItem {
    pub id: String,
    pub artifact_type: String,
    pub language: String,
    pub artifact_path: String,
    pub backup_path: Option<String>,
    pub job_id: Option<String>,
    pub editor: String,
    pub note: Option<String>,
    pub summary: Option<String>,
    pub locations_json: Option<String>,
    pub diff_json: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub reasoning: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDetail {
    pub target: TargetCard,
    pub summary: Option<String>,
    pub department: Option<String>,
    pub pi_research_summary: Option<String>,
    pub application_notes: Option<String>,
    pub artifacts: Vec<ArtifactItem>,
    pub checklist: Vec<ChecklistItem>,
    pub replies: Vec<ReplyItem>,
    pub revisions: Vec<RevisionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub adapter_kind: String,
    pub connection_mode: String,
    pub enabled: bool,
    pub base_url: Option<String>,
    pub configured: bool,
    pub last_validated_at: Option<String>,
    pub validation_message: Option<String>,
    pub models: Vec<ProviderModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelInfo {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub enabled: bool,
    pub supports_reasoning: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub reasoning_levels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderRuntimeConfig {
    pub id: String,
    pub display_name: String,
    pub adapter_kind: String,
    pub base_url: String,
    pub secret_reference: String,
    pub models: Vec<ProviderRuntimeModel>,
}

#[derive(Debug, Clone)]
pub struct ProviderRuntimeModel {
    pub slug: String,
    pub display_name: String,
    pub supports_vision: bool,
    pub reasoning_levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskModelDefault {
    pub task_type: String,
    pub provider_id: String,
    pub model_id: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub id: String,
    pub job_type: String,
    pub target_id: Option<String>,
    pub result_target_ids: Vec<String>,
    pub status: String,
    pub progress: i64,
    pub message: Option<String>,
    pub provider_id: String,
    pub account_id: Option<String>,
    pub model_id: Option<String>,
    pub reasoning: Option<String>,
    pub thread_id: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobGroups {
    pub running: Vec<JobSummary>,
    pub queued: Vec<JobSummary>,
    pub needs_review: Vec<JobSummary>,
    pub needs_review_total: i64,
    pub recent: Vec<JobSummary>,
    pub recent_total: i64,
    pub capacity: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailStatus {
    pub configured: bool,
    pub connected: bool,
    pub account_email: Option<String>,
    pub connection_ok: bool,
    pub oauth_status: String,
    pub oauth_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailOAuthStart {
    pub authorization_url: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailDraftInfo {
    pub id: String,
    pub target_id: String,
    pub gmail_draft_id: String,
    pub gmail_message_id: Option<String>,
    pub recipient: String,
    pub subject: String,
    pub cv_path: String,
    pub remote_verified: bool,
    pub created_at: String,
    pub gmail_url: String,
}
