use crate::db;
use crate::models::JobSummary;
use crate::paths::AppPaths;
use crate::typst;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;
use uuid::Uuid;

#[path = "opportunity_identity.rs"]
mod identity;
#[cfg(test)]
use identity::{same_organization, www_alias as www_source_alias, similar_title as similar_posting_title};

fn incoming_identity(value: &FoundOpportunity) -> identity::Facts<'_> {
    identity::Facts { organization:&value.organization,title:&value.title,source:&value.source_url,
        external_id:value.external_id.as_deref(),saved_key:None,deadline:value.deadline.as_deref() }
}

struct SavedOpportunityIdentity {
    id:String,key:Option<String>,source:String,organization:String,title:String,deadline:Option<String>,visible:bool,emails:String,
}
impl SavedOpportunityIdentity {
    fn facts(&self)->identity::Facts<'_> {
        identity::Facts {organization:&self.organization,title:&self.title,source:&self.source,external_id:None,saved_key:self.key.as_deref(),deadline:self.deadline.as_deref()}
    }
    fn compare(&self,value:&FoundOpportunity)->identity::Match {
        let shared=value.contacts.iter().filter_map(|v|v.email.as_deref()).any(|email|self.emails.split('\u{1f}').any(|saved|saved.eq_ignore_ascii_case(email.trim())));
        identity::compare(&self.facts(),&incoming_identity(value),shared)
    }
}

fn saved_opportunity_identities(conn:&Connection)->Result<Vec<SavedOpportunityIdentity>> {
    let mut stmt=conn.prepare("SELECT o.id,o.identity_key,COALESCE(o.source_url,''),o.organization,o.title,o.deadline,
        NOT EXISTS(SELECT 1 FROM contact_targets_v2 t WHERE t.opportunity_id=o.id)
          OR EXISTS(SELECT 1 FROM contact_targets_v2 t WHERE t.opportunity_id=o.id AND t.archived_at IS NULL),
        COALESCE((SELECT group_concat(lower(t.email),char(31)) FROM contact_targets_v2 t WHERE t.opportunity_id=o.id),'')
        FROM opportunities o WHERE COALESCE(o.opportunity_type,'')<>'industry_internship'")?;
    Ok(stmt.query_map([],|r|Ok(SavedOpportunityIdentity {id:r.get(0)?,key:r.get(1)?,source:r.get(2)?,organization:r.get(3)?,title:r.get(4)?,deadline:r.get(5)?,visible:r.get(6)?,emails:r.get(7)?}))?
        .collect::<std::result::Result<Vec<_>,_>>()?)
}

#[derive(Debug, thiserror::Error)]
#[error("搜索结果结构不完整：{0}")]
pub(crate) struct SearchContractError(String);

#[derive(Debug, thiserror::Error)]
#[error("机会身份待确认：{0}")]
struct OpportunityIdentityConflict(String);

#[derive(Debug, thiserror::Error)]
#[error("联系人身份待确认：{0}；未合并或覆盖原联系人和材料")]
struct ContactIdentityConflict(String);

#[derive(Debug, thiserror::Error)]
#[error("已有更新的核验结果，已跳过旧结果，未改动联系人和材料")]
struct StaleOpportunityResult;

#[derive(Debug, thiserror::Error)]
#[error("机会或联系人已搁置，已跳过；请先恢复后再补齐材料")]
struct ShelvedOpportunityResult;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchOutput {
    #[serde(default = "protocol_version")]
    schema_version: u8,
    #[serde(default)]
    opportunities: Vec<FoundOpportunity>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InternshipSearchOutput {
    #[serde(default = "protocol_version")]
    schema_version: u8,
    #[serde(default)]
    opportunities: Vec<FoundInternshipOpportunity>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FoundInternshipOpportunity {
    opportunity_kind: String,
    external_id: Option<String>,
    source_url: String,
    source_title: Option<String>,
    title: String,
    organization: String,
    department: Option<String>,
    country: Option<String>,
    region: Option<String>,
    location: Option<String>,
    deadline: Option<String>,
    summary: String,
    #[serde(default)]
    keywords: Vec<String>,
    active: bool,
    eligibility_status: String,
    eligibility_summary: String,
    fit_score: f64,
    fit_analysis: String,
    fit_analysis_zh: String,
    verified_at: String,
    #[serde(default)]
    sources: Vec<SourceEvidence>,
    #[serde(default)]
    checklist: Vec<ChecklistOutput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FoundOpportunity {
    external_id: Option<String>,
    source_url: String,
    source_title: Option<String>,
    title: String,
    organization: String,
    department: Option<String>,
    country: Option<String>,
    region: Option<String>,
    opportunity_type: Option<String>,
    deadline: Option<String>,
    summary: String,
    #[serde(default)]
    keywords: Vec<String>,
    career_level_eligible: bool,
    #[serde(default)]
    availability: Option<String>,
    #[serde(default)]
    eligibility_status: Option<String>,
    verified_at: String,
    #[serde(default)]
    sources: Vec<SourceEvidence>,
    #[serde(default)]
    contacts: Vec<FoundContact>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FoundContact {
    name: String,
    email: Option<String>,
    fit_score: f64,
    #[serde(default = "default_priority")]
    priority: i64,
    homepage_url: Option<String>,
    lab_url: Option<String>,
    research_summary: Option<String>,
    #[serde(default)]
    materials: MaterialPackage,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct MaterialPackage {
    cv_data: Value,
    fit_scores: Vec<FitDimension>,
    email_en: String,
    email_zh: String,
    fit_analysis: String,
    fit_analysis_zh: String,
    pi_profile: String,
    pi_profile_zh: String,
    #[serde(default)]
    checklist: Vec<ChecklistOutput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FitDimension {
    dimension: String,
    score: f64,
    evidence: String,
    uncertainty: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceEvidence {
    title: String,
    url: String,
    checked_at: String,
    #[serde(default = "default_evidence_type")]
    evidence_type: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChecklistOutput {
    item_type: String,
    #[serde(default)]
    required: bool,
    status: String,
    #[serde(default = "default_origin")]
    origin: String,
    evidence: Option<String>,
    source_url: Option<String>,
    note: Option<String>,
    #[serde(default = "default_sort")]
    sort_order: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplyOutput {
    #[serde(default = "protocol_version")]
    schema_version: u8,
    decision: String,
    summary_zh: String,
    recommended_action: String,
    draft_reply_en: String,
    draft_reply_zh: String,
    #[serde(default)]
    sources: Vec<SourceEvidence>,
    #[serde(default)]
    recommended_opportunities: Vec<FoundOpportunity>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChecklistResult {
    #[serde(default = "protocol_version")]
    schema_version: u8,
    #[serde(default)]
    items: Vec<ChecklistOutput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FollowUpScanOutput {
    #[serde(default = "protocol_version")]
    schema_version: u8,
    #[serde(default)]
    items: Vec<FollowUpRecommendation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FollowUpRecommendation {
    target_id: String,
    action: String,
    reason: String,
    due_at: Option<String>,
    #[serde(default)]
    sources: Vec<SourceEvidence>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationOutput {
    #[serde(default = "protocol_version")]
    schema_version: u8,
    active: bool,
    summary_zh: String,
    checked_at: String,
    #[serde(default)]
    sources: Vec<SourceEvidence>,
}

fn default_priority() -> i64 { 100 }
fn default_sort() -> i64 { 100 }
fn default_origin() -> String { "verified".into() }
fn default_evidence_type() -> String { "primary".into() }
fn protocol_version() -> u8 { 1 }

fn opportunity_contract() -> Value {
    json!({
        "externalId":"optional stable source id",
        "sourceUrl":"verified primary URL",
        "sourceTitle":"source title",
        "title":"role title",
        "organization":"institution",
        "department":"optional",
        "country":"optional",
        "region":"optional",
        "opportunityType":"formal_position|fellowship|program|prospective_contact|other (legacy formal_postdoc/prospective_pi also accepted)",
        "deadline":"ISO date or null",
        "summary":"verified summary",
        "keywords":["keyword"],
        "careerLevelEligible":true,
        "eligibilityStatus":"eligible|uncertain|ineligible",
        "availability":"open|closed|prospective|uncertain",
        "verifiedAt":"UTC ISO-8601",
        "sources":[{"title":"primary source","url":"https://...","checkedAt":"UTC ISO-8601","evidenceType":"primary"}],
        "contacts":[{
            "name":"contact or responsible person","email":"verified email or null","fitScore":86,
            "priority":1,"homepageUrl":"optional","labUrl":"optional","researchSummary":"verified",
            "materials":{
                "cvData": crate::cv_schema::contract(),
                "fitScores":[
                    {"dimension":"research","score":26,"evidence":"source-backed overlap","uncertainty":"known gap or explicitly unknown"},
                    {"dimension":"methods","score":23,"evidence":"source-backed methods","uncertainty":"known gap or unknown"},
                    {"dimension":"evidence","score":13,"evidence":"source CV achievements","uncertainty":"known gap or unknown"},
                    {"dimension":"requirements","score":12,"evidence":"eligibility evidence","uncertainty":"known gap or unknown"},
                    {"dimension":"logistics","score":8,"evidence":"stated preferences","uncertainty":"unknown if not supplied"},
                    {"dimension":"hiring","score":4,"evidence":"current primary source","uncertainty":"unknown if no vacancy"}
                ],
                "emailEn":"complete reviewable Markdown",
                "emailZh":"complete Chinese translation Markdown",
                "fitAnalysis":"source-linked English analysis when requested; headings are flexible",
                "fitAnalysisZh":"source-linked Chinese analysis when requested",
                "piProfile":"source-backed English contact brief when requested; explicitly mark unknown information",
                "piProfileZh":"Chinese contact brief when requested",
                "checklist":[{"itemType":"unique stable key such as eligibility_confirmation","required":true,"status":"ready|review|missing","origin":"verified|inferred","evidence":"text","sourceUrl":"verified https URL or null for inferred-only items","note":"optional","sortOrder":10}]
            }
        }]
    })
}

pub fn result_contract(job_type: &str) -> Value {
    match job_type {
        "internship_search" => json!({
            "file": "output/internship-search-results.json",
            "schemaVersion": 1,
            "rules": [
                "Use exact camelCase keys shown below; do not emit snake_case alternatives.",
                "Return current industry internships only; exclude postdoctoral, doctoral, faculty and full-time roles.",
                "Use eligibilityStatus=uncertain when the supplied evidence cannot establish candidate eligibility.",
                "Score against the requested search brief; do not use unverified candidate facts.",
                "Do not create a CV, outreach email, Gmail draft or application submission."
            ],
            "limits": {"discovery": 20, "saved": 10},
            "required": {
                "schemaVersion": 1,
                "opportunities": [{
                    "opportunityKind": "industry_internship",
                    "externalId": "optional stable source id",
                    "sourceUrl": "verified official job URL",
                    "sourceTitle": "official source title",
                    "title": "internship title",
                    "organization": "company or organization",
                    "department": "optional team",
                    "country": "optional country",
                    "region": "optional region",
                    "location": "optional exact or remote location",
                    "deadline": "ISO date or null",
                    "summary": "verified role summary",
                    "keywords": ["keyword"],
                    "active": true,
                    "eligibilityStatus": "eligible|uncertain|ineligible",
                    "eligibilitySummary": "evidence-based explanation",
                    "fitScore": 82,
                    "fitAnalysis": "complete reviewable Markdown",
                    "fitAnalysisZh": "complete Chinese reviewable Markdown",
                    "verifiedAt": "UTC ISO-8601",
                    "sources": [{"title":"official source","url":"https://...","checkedAt":"UTC ISO-8601","evidenceType":"primary"}],
                    "checklist": [{"itemType":"eligibility_confirmation","required":true,"status":"ready|review|missing","origin":"verified|inferred","evidence":"text","sourceUrl":"https://...","note":"optional","sortOrder":10}]
                }]
            }
        }),
        "full_run" | "full_search" | "research_pi" => {
            let mut rules = vec![
                "Use exact camelCase keys. Stage 1: search and verify opportunities, then write output/search-results.json with contacts but OMIT materials; stop for the application to save discoveries. Stage 2 begins only when explicitly asked to prepare pending materials.".into(),
                "Read profile/master_profile.json and the source CV. Derive stage, discipline and background automatically. Source CV claims are user-provided evidence, not independently verified; do not require approved flags. Unknown eligibility, dates and constraints remain unknown.".into(),
                "When no named supervisor is found, search the institution and Google Scholar by name, affiliation and topic, then follow DOI/publisher links. Academic papers show research fit, not a vacancy. An official application portal or recruiting office may be a contact target with its real label; never invent a person or email.".into(),
                "For an advertised opening, use current primary recruitment evidence and tailor materials to the duties. Otherwise label availability prospective or uncertain. Do not infer open from a professor profile. Return an empty opportunities array if no matches; do not fabricate results.".into(),
                "Stage 2: fill fitScores with six unique dimensions: research 30, methods 25, evidence 15, requirements 15, logistics 10, hiring 5. Numeric earned scores must sum to fitScore. Each dimension includes source-backed evidence and explicit uncertainty. Markdown tables and headings are for readability, not a validation protocol.".into(),
                "Write substantive fit analysis, contact brief and email in the user's preferredLanguage (zh, en, or bilingual). Omit unrequested translations. Use natural discipline-appropriate headings, primary links and honest gaps; there is no minimum word count or fixed number of papers.".into(),
                "CV sections are discipline-appropriate and source-backed, not a mandatory eight-section template. Respect pageCount and enabled customization, including reference display. Section and entry counts are never locked; current user instructions guide source-backed additions, removals and ordering.".into(),
            ];
            rules.extend(crate::cv_schema::generation_rules().into_iter().map(str::to_owned));
            rules.extend(typst::layout_generation_rules());
            json!({
                "file": "output/search-results.json",
                "schemaVersion": 1,
                "rules": rules,
                "cvPolicy": crate::cv_schema::generation_policy_contract(),
                "cvLayoutPolicy": typst::layout_policy_contract(),
                "limits": {"discovery":20,"deepVerification":8,"completePackages":5},
                "required": {
                    "schemaVersion": 1,
                    "opportunities": [opportunity_contract()]
                }
            })
        },
        "reply_followup" => json!({
            "file":"output/reply-followup.json",
            "schemaVersion":1,
            "required":{
                "schemaVersion":1,
                "decision":"continue|investigate_referral|clarify|wait|stop",
                "summaryZh":"Chinese decision explanation",
                "recommendedAction":"specific next action",
                "draftReplyEn":"complete reviewable draft, never sent",
                "draftReplyZh":"complete Chinese translation",
                "sources":[{"title":"source","url":"https://...","checkedAt":"UTC ISO-8601","evidenceType":"primary"}],
                "recommendedOpportunities":[opportunity_contract()]
            }
        ,"note":"Use stop only when the sender clearly rejects or declines further contact; ambiguous or delayed outcomes must use wait or clarify. Use an empty recommendedOpportunities array when no verified referral exists."}),
        "checklist_refresh" => json!({
            "file":"output/checklist.json",
            "schemaVersion":1,
            "rules":["itemType must be a unique stable snake_case key within this target; never use the literal value item for every row."],
            "required":{"schemaVersion":1,"items":[{"itemType":"unique key such as eligibility_confirmation or cv","required":true,"status":"ready|review|missing","origin":"verified|inferred","evidence":"text","sourceUrl":"verified https URL or null for inferred-only items","note":"optional","sortOrder":10}]}
        }),
        "follow_up_scan" => json!({
            "file":"output/follow-up-scan.json",
            "schemaVersion":1,
            "required":{"schemaVersion":1,"items":[{
                "targetId":"exact contact target id from input/targets.json",
                "action":"wait|send_follow_up|investigate|close_out",
                "reason":"concise evidence-based Chinese explanation",
                "dueAt":"optional UTC ISO-8601",
                "sources":[{"title":"source","url":"https://...","checkedAt":"UTC ISO-8601","evidenceType":"primary"}]
            }]},
            "note":"Recommendations are review-only. Do not change status, send email, or create a draft."
        }),
        "opportunity_health" | "pi_verification" => json!({
            "file":"output/verification.json",
            "schemaVersion":1,
            "required":{"schemaVersion":1,"active":true,"summaryZh":"explanation","checkedAt":"UTC ISO-8601","sources":[{"title":"primary source","url":"https://...","checkedAt":"UTC ISO-8601","evidenceType":"primary"}]},
            "note":"Verification is review-only. Do not change contact status or archive records."
        }),
        _ => json!({"directory":"output","note":"Write reviewable files only."}),
    }
}

pub async fn import_job_result(
    paths: &AppPaths,
    job: &JobSummary,
    payload: &Value,
    workspace: &Path,
) -> Result<Value> {
    match job.job_type.as_str() {
        "internship_search" => {
            let output = read_json::<InternshipSearchOutput>(
                &workspace.join("output/internship-search-results.json"),
            )?;
            ensure_protocol_version(output.schema_version, "internship-search-results.json")?;
            import_internship_search_output(paths, &job.id, payload, output)
        }
        "full_run" | "full_search" | "research_pi" => {
            let bytes = fs::read(workspace.join("output/search-results.json"))?;
            let output: SearchOutput = serde_json::from_slice(&bytes)
                .map_err(|error| SearchContractError(error.to_string()))?;
            ensure_protocol_version(output.schema_version, "search-results.json")
                .map_err(|error| SearchContractError(error.to_string()))?;
            import_search_output(paths, &job.id, payload, output).await
        }
        "reply_followup" => {
            let output = read_json::<ReplyOutput>(&workspace.join("output/reply-followup.json"))?;
            ensure_protocol_version(output.schema_version, "reply-followup.json")?;
            import_reply_output(paths, job, payload, output).await
        }
        "checklist_refresh" => {
            let target_id = job.target_id.as_deref().context("刷新清单缺少联系人 ID")?;
            let output = read_json::<ChecklistResult>(&workspace.join("output/checklist.json"))?;
            ensure_protocol_version(output.schema_version, "checklist.json")?;
            replace_checklist(paths, target_id, &output.items)?;
            Ok(json!({"targetId":target_id,"items":output.items.len()}))
        }
        "follow_up_scan" => {
            let output = read_json::<FollowUpScanOutput>(&workspace.join("output/follow-up-scan.json"))?;
            ensure_protocol_version(output.schema_version, "follow-up-scan.json")?;
            import_follow_up_scan(paths, &job.id, output)
        }
        "opportunity_health" | "pi_verification" => {
            let output = read_json::<VerificationOutput>(&workspace.join("output/verification.json"))?;
            ensure_protocol_version(output.schema_version, "verification.json")?;
            import_verification(paths, job, output)
        }
        _ => Ok(json!({"outputDirectory":workspace.join("output")})),
    }
}

fn import_verification(paths: &AppPaths, job: &JobSummary, output: VerificationOutput) -> Result<Value> {
    if output.summary_zh.trim().len() < 12 || output.checked_at.trim().is_empty() {
        bail!("核验结果缺少中文摘要或核验时间")
    }
    if output.sources.is_empty() {
        bail!("核验结果没有来源，不能进入审核")
    }
    for source in &output.sources {
        validate_source(source)?;
    }
    if let Some(target_id) = job.target_id.as_deref() {
        let conn = db::connect(&paths.database)?;
        let tx = db::publication_transaction(&conn)?;
        let application_id: String = conn.query_row(
            "SELECT application_id FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL",
            [target_id],
            |row| row.get(0),
        )?;
        let artifact_type = if job.job_type == "pi_verification" { "pi_verification" } else { "opportunity_verification" };
        let filename = if job.job_type == "pi_verification" { "pi-verification-zh.md" } else { "opportunity-verification-zh.md" };
        let source_lines = output.sources.iter().map(|source| format!("- [{}]({}) · {}", source.title, source.url, source.checked_at)).collect::<Vec<_>>().join("\n");
        let body = format!(
            "# {}\n\n- 状态：{}\n- 核验时间：{}\n\n{}\n\n## 来源\n\n{}\n",
            if job.job_type == "pi_verification" { "联系人重新核验" } else { "机会重新核验" },
            if output.active { "有效" } else { "可能失效" },
            output.checked_at,
            output.summary_zh,
            source_lines,
        );
        store_text_artifact(paths, &tx, target_id, &application_id, artifact_type, "zh", filename, &body)?;
        record_sources(&tx, "contact_target", target_id, &output.sources)?;
        tx.execute(
            "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id)
             VALUES(?1,?2,?3,?3)",
            params![job.id, artifact_type, target_id],
        )?;
        tx.commit()?;
    }
    Ok(json!({"reviewOnly":true,"active":output.active,"verification":output}))
}

fn import_follow_up_scan(paths: &AppPaths, job_id: &str, output: FollowUpScanOutput) -> Result<Value> {
    let database_conn = db::connect(&paths.database)?;
    let conn = db::publication_transaction(&database_conn)?;
    let mut imported = Vec::new();
    for item in output.items.into_iter().take(20) {
        if item.reason.trim().len() < 12 {
            bail!("跟进建议 {} 缺少明确理由", item.target_id)
        }
        if !matches!(item.action.as_str(), "wait" | "send_follow_up" | "investigate" | "close_out") {
            bail!("跟进建议 {} 的动作无效", item.target_id)
        }
        let status: String = conn.query_row(
            "SELECT CASE WHEN shelved_at IS NOT NULL THEN 'shelved' ELSE status END
             FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL",
            [&item.target_id],
            |row| row.get(0),
        ).with_context(|| format!("跟进建议引用了不存在的联系人 {}", item.target_id))?;
        if !matches!(status.as_str(), "contacted" | "replied" | "follow_up") {
            bail!("联系人 {} 当前不是可跟进状态", item.target_id)
        }
        for source in &item.sources {
            validate_source(source)?;
        }
        conn.execute(
            "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id)
             VALUES(?1,'follow_up_recommendation',?2,?2)",
            params![job_id, item.target_id],
        )?;
        imported.push(json!({
            "targetId":item.target_id,
            "action":item.action,
            "reason":item.reason,
            "dueAt":item.due_at,
            "sources":item.sources,
        }));
    }
    conn.commit()?;
    Ok(json!({"reviewOnly":true,"recommendations":imported}))
}

pub fn rebuild_preferences(paths: &AppPaths) -> Result<Value> {
    let conn = db::connect(&paths.database)?;
    let mut statement = conn.prepare(
        "SELECT category,signal_json,created_at FROM preference_observations
         ORDER BY created_at DESC,id DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?))
    })?.collect::<std::result::Result<Vec<_>,_>>()?;
    let observations = rows.into_iter().map(|(category,signal,created_at)| {
        json!({"category":category,"signal":serde_json::from_str::<Value>(&signal).unwrap_or(Value::String(signal)),"createdAt":created_at})
    }).collect::<Vec<_>>();
    let value = json!({
        "schemaVersion":1,
        "rebuiltAt":Utc::now().to_rfc3339(),
        "policy":"Revision observations guide future retrieval and writing; they never change evidence fit scores or verified facts.",
        "observations":observations,
    });
    let live = paths.profile.join("learned_preferences.json");
    drop(statement);
    let _execution = db::publication_transaction(&conn)?;
    let _profile = crate::onboarding::profile_guard()?;
    crate::onboarding::recover_profile_import(paths)?;
    if live.exists() {
        let backup = paths.backups.join(format!("{}-learned_preferences.json",Uuid::new_v4().simple()));
        fs::copy(&live,backup)?;
    }
    let temporary=paths.profile.join(format!(".preferences-{}.tmp",Uuid::new_v4().simple()));
    fs::write(&temporary,serde_json::to_vec_pretty(&value)?)?;
    fs::rename(temporary,&live)?;
    Ok(json!({"observations":observations.len(),"path":live}))
}

fn import_internship_search_output(
    paths: &AppPaths,
    job_id: &str,
    payload: &Value,
    mut output: InternshipSearchOutput,
) -> Result<Value> {
    let threshold = payload.get("threshold").and_then(Value::as_f64).unwrap_or(70.0);
    let result_limit = configured_result_limit(payload, 10);
    output.opportunities.truncate(20);
    output.opportunities.sort_by(|left, right| right.fit_score.total_cmp(&left.fit_score));
    let mut imported = Vec::new();
    let mut warnings = Vec::new();
    for opportunity in output.opportunities {
        if imported.len() >= result_limit {
            break;
        }
        if opportunity.fit_score <= threshold {
            continue;
        }
        if let Err(error) = validate_internship_opportunity(&opportunity) {
            warnings.push(format!("{}：{error}", opportunity.title));
            continue;
        }
        let target = upsert_internship_target(paths, job_id, &opportunity)?;
        imported.push(json!({
            "targetId": target.target_id,
            "applicationId": target.application_id,
            "opportunityId": target.opportunity_id,
            "organization": opportunity.organization,
            "title": opportunity.title,
            "eligibilityStatus": opportunity.eligibility_status,
            "fitScore": opportunity.fit_score,
        }));
    }
    if imported.is_empty() {
        bail!(
            "没有 internship 同时通过职位类型、有效性、来源和严格匹配阈值：{}",
            warnings.join("；")
        )
    }
    Ok(json!({
        "imported": imported,
        "warnings": warnings,
        "thresholdStrictlyGreaterThan": threshold,
        "reviewOnly": true,
    }))
}

async fn import_search_output(
    paths: &AppPaths,
    job_id: &str,
    payload: &Value,
    mut output: SearchOutput,
) -> Result<Value> {
    let threshold = payload.get("threshold").and_then(Value::as_f64).unwrap_or(75.0);
    let result_limit = configured_result_limit(payload, 5);
    output.opportunities.truncate(20);
    // A card continuation must not publish other jobs discovered by the model.
    // Resolve the persisted record, not the model-provided snapshot or identifiers.
    let scoped_id = payload.get("opportunityId").map(|value| {
        value.as_str().filter(|id| !id.trim().is_empty()).context("续办任务的机会 ID 无效")
    }).transpose()?;
    let confirmed_source = payload.get("confirmedSourceUrl").and_then(Value::as_str).filter(|url| is_http_url(url));
    if let Some(id) = scoped_id {
        let conn = db::connect(&paths.database)?;
        if let Err(error) = ensure_continuation_scope(&conn, id, &output.opportunities, confirmed_source) {
            return Ok(json!({"outcome":"identity_needs_review","imported":[],"pendingMaterials":[],
                "warnings":[error.to_string()],"summary":format!("机会身份待确认，未合并或覆盖原记录：{error}")}));
        }
    }
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    let mut rejected = 0;
    let mut identity_reviews = 0;
    let mut identity_review_names = Vec::new();
    let mut stale_results = 0;
    for opportunity in output.opportunities {
        if let Err(error) = validate_opportunity(&opportunity) {
            rejected += 1;
            warnings.push(format!("{}：{error}", opportunity.title));
            continue;
        }
        // Persist source-verified opportunities even when no individual contact is known.
        let conn = db::connect(&paths.database)?;
        let tx = db::publication_transaction(&conn)?;
        let opportunity_id = match upsert_opportunity(&tx, &opportunity, scoped_id, confirmed_source) {
            Ok(id) => id,
            Err(error) if error.is::<OpportunityIdentityConflict>() => {
                identity_reviews += 1;
                identity_review_names.push(format!("{} / {}",opportunity.organization,opportunity.title));
                warnings.push(format!("{}：{error}", opportunity.title));
                continue;
            }
            Err(error) => return Err(error),
        };
        record_sources(&tx, "opportunity", &opportunity_id, &opportunity.sources)?;
        let current = opportunity_result_is_current(&tx, &opportunity_id, &opportunity.verified_at)?;
        let closed = saved_opportunity_closed(&tx,&opportunity_id)?;
        let shelved = db::opportunity_shelved(&tx, &opportunity_id)?;
        tx.commit()?;
        if shelved {
            warnings.push(format!("{}：机会已搁置，仅保留来源记录，不生成联系人或材料", opportunity.title));
            continue;
        }
        if !current {
            stale_results += 1;
            warnings.push(format!("{}：{StaleOpportunityResult}", opportunity.title));
            continue;
        }
        if closed {
            warnings.push(format!("{}：机会已关闭，仅保留来源记录，不生成申请材料", opportunity.title));
            continue;
        }
        // Detect conflicting people before importing any contact in this result.
        // Otherwise the first same-name entry can overwrite an existing target
        // before the later `seen` check discovers the collision.
        let conflicts = conflicting_contacts(&opportunity.contacts);
        for (index, contact) in opportunity.contacts.iter().cloned().enumerate() {
            if conflicts.contains(&index) {
                identity_reviews += 1;
                identity_review_names.push(format!("{} / {}", opportunity.organization, contact.name));
                warnings.push(ContactIdentityConflict(format!("同批结果中 {} 的姓名、邮箱和个人主页互相冲突，请核对身份", contact.name)).to_string());
                continue;
            }
            if let Err(error) = validate_contact_identity(&contact) {
                warnings.push(format!("{}：{error}", opportunity.title));
                continue;
            }
            if scoped_id.is_none() && contact.fit_score <= threshold { continue; }
            candidates.push((contact.fit_score, opportunity.clone(), contact, opportunity_id.clone()));
        }
    }
    // One logical vacancy can appear in several output entries. Finish discovery
    // first, discard entries superseded within this batch, then check the whole
    // contact batch before any target/PI write can select an arbitrary first match.
    let conn = db::connect(&paths.database)?;
    let mut current_candidates = Vec::new();
    for candidate in candidates {
        if opportunity_result_is_current(&conn, &candidate.3, &candidate.1.verified_at)? {
            current_candidates.push(candidate);
        } else {
            stale_results += 1;
            warnings.push(format!("{}：{StaleOpportunityResult}", candidate.1.title));
        }
    }
    drop(conn);
    let conflicts = conflicting_discovered_contacts(&current_candidates);
    let mut candidates = Vec::new();
    for (index, candidate) in current_candidates.into_iter().enumerate() {
        if conflicts.contains(&index) {
            identity_reviews += 1;
            identity_review_names.push(format!("{} / {}", candidate.1.organization, candidate.2.name));
            warnings.push(ContactIdentityConflict(format!("同批机会中的 {} 身份证据互相冲突，请核对后再继续", candidate.2.name)).to_string());
        } else {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|a,b| b.0.total_cmp(&a.0));
    let mut imported = Vec::new();
    let mut pending = Vec::new();
    let mut seen = HashSet::new();
    let mut render_paths = paths.clone();
    let snapshot = paths.workspaces.join(job_id).join("profile");
    if snapshot.join(".snapshot-complete").is_file() { render_paths.profile = snapshot; }
    for (_, opportunity, mut contact, _) in candidates {
        if imported.iter().filter(|item: &&Value| item.get("preservedExistingMaterials").and_then(Value::as_bool) != Some(true)).count() + pending.len() >= result_limit { break; }
        let target = match upsert_discovered_contact(paths, job_id, &opportunity, &contact, scoped_id, confirmed_source) {
            Ok(target) => target,
            Err(error) if error.is::<StaleOpportunityResult>() || error.is::<OpportunityIdentityConflict>() || error.is::<ContactIdentityConflict>() || error.is::<ShelvedOpportunityResult>() => {
                if error.is::<StaleOpportunityResult>() { stale_results += 1; }
                else if error.is::<OpportunityIdentityConflict>() || error.is::<ContactIdentityConflict>() {
                    identity_reviews += 1;
                    identity_review_names.push(format!("{} / {}", opportunity.organization, contact.name));
                } else { rejected += 1; }
                warnings.push(format!("{}：{error}", opportunity.title));
                continue;
            }
            Err(error) => return Err(error),
        };
        if !seen.insert(target.target_id.clone()) { continue; }
        let conn = db::connect(&paths.database)?;
        let language = crate::onboarding::load(&render_paths)?.preferred_language;
        {
            let tx = db::publication_transaction(&conn)?;
            record_material_language(&tx,paths,&target.target_id,&language,false)?;
            tx.commit()?;
        }
        let existing = saved_materials(&conn, paths, &target.target_id)?;
        if package_complete(&conn, &target.target_id, &existing, &language)? {
            // Search must never overwrite an existing package or a user's edits.
            let tx = db::publication_transaction(&conn)?;
            record_material_language(&tx,paths,&target.target_id,&language,true)?;
            reconcile_material_state(&tx,paths,&target.target_id,None)?;
            tx.commit()?;
            imported.push(json!({"targetId":target.target_id,"contact":contact.name,"preservedExistingMaterials":true}));
            continue;
        }
        drop(conn);
        let package = target_directory(paths, &target.target_id).join("versions").join(Uuid::new_v4().simple().to_string());
        let outcome = async {
            validate_contact_for_language(&contact, &render_paths)?;
            fs::create_dir_all(&package)?;
            let cv_snapshot = cv_material_fingerprint(&existing)?;
            let cv_complete = ["cv_pdf","cv_data","cv_typst"].iter().all(|kind| existing.keys().any(|(k,_)| k == kind));
            let pages = if cv_complete { 0 } else {
                if let Some(path) = existing.get(&("cv_data".into(), "und".into())) {
                    contact.materials.cv_data = read_json(path)?;
                } else if existing.keys().any(|(kind,_)| kind.starts_with("cv_")) {
                    bail!("已有部分 CV 文件但缺少结构化源数据；为保护原内容，请在 CV 页面核对并重新生成。其他原文件未改动")
                }
                let pages = typst::render_cv_package(&render_paths, &contact.materials.cv_data, &package).await?;
                pages
            };
            install_material_package(paths, &target, &contact, &package, &language, &cv_snapshot)?;
            Ok::<_, anyhow::Error>(pages)
        }.await;
        match outcome {
            Ok(pages) => imported.push(json!({"targetId":target.target_id,"contact":contact.name,"cvPages":pages})),
            Err(error) => {
                let message = format!("{error:#}");
                let conn = db::connect(&paths.database)?;
                let tx = db::publication_transaction(&conn)?;
                let ready = reconcile_material_state(&tx,paths,&target.target_id,Some(&message))?;
                tx.commit()?;
                if ready {
                    imported.push(json!({"targetId":target.target_id,"contact":contact.name,"preservedExistingMaterials":true}));
                    warnings.push(format!("{}：另一份有效材料已保存，本次候选未覆盖它",contact.name));
                } else {
                    pending.push(json!({"targetId":target.target_id,"contact":contact.name,"code":"materials_incomplete","detail":message}));
                }
                // Unpublished partial files are retained as diagnostics, never pointed to by the UI.
            }
        }
    }
    let outcome = if !pending.is_empty() { "materials_pending" }
        else if imported.is_empty() && identity_reviews > 0 { "identity_needs_review" }
        else if imported.is_empty() && rejected > 0 { "sources_need_review" }
        else if imported.is_empty() && stale_results > 0 { "stale_results_ignored" }
        else if imported.is_empty() { "no_matches" } else { "ready" };
    let summary = match outcome {
        "materials_pending" => "机会已保存，部分材料待完成；可继续原任务。",
        "identity_needs_review" => "发现疑似重复或机会／联系人身份冲突，冲突记录未合并，已有材料保持不变。",
        "sources_need_review" => "来源核验未通过，请查看具体原因；不是模型连接失败。",
        "stale_results_ignored" => "已有更新的核验记录，已跳过旧结果；现有联系人与材料保持不变。",
        "no_matches" => "本次没有找到符合条件的联系目标。可以调整地区、方向或阈值。",
        _ => "机会与材料已保存，等待审核。",
    };
    let summary = if identity_reviews>0 { format!("{summary} 另有 {identity_reviews} 条需核对身份：{}。请核对机会来源与联系人的邮箱／个人主页。",identity_review_names.into_iter().take(3).collect::<Vec<_>>().join("；")) } else {summary.into()};
    Ok(json!({
        "outcome":outcome,"imported":imported,"pendingMaterials":pending,"warnings":warnings,
        "identityReviewCount":identity_reviews,
        "thresholdStrictlyGreaterThan":threshold,
        "summary":summary
    }))
}

fn configured_result_limit(payload: &Value, fallback: usize) -> usize {
    payload
        .get("maxResults")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=5).contains(value))
        .unwrap_or(fallback)
}

async fn import_reply_output(
    paths: &AppPaths,
    job: &JobSummary,
    payload: &Value,
    output: ReplyOutput,
) -> Result<Value> {
    let target_id = job.target_id.as_deref().context("回复任务缺少联系人 ID")?;
    if !matches!(output.decision.as_str(), "continue" | "investigate_referral" | "clarify" | "wait" | "stop") {
        bail!("回复 Agent 返回了无效决策：{}", output.decision)
    }
    if output.summary_zh.trim().is_empty() || output.recommended_action.trim().is_empty() {
        bail!("回复 Agent 没有给出可审核的判断")
    }
    if output.draft_reply_en.trim().len() < 20 || output.draft_reply_zh.trim().len() < 12 {
        bail!("回复 Agent 没有给出完整的双语审核草稿")
    }
    let next_status = reply_decision_status(&output.decision)?;
    for source in &output.sources { validate_source(source)?; }
    let (drafts_applied, status_applied) = publish_reply_result(paths, job, payload, &output, next_status)?;
    let referrals = if !drafts_applied || output.recommended_opportunities.is_empty() {
        json!({"imported":[],"warnings":[]})
    } else {
        import_search_output(
            paths,
            &job.id,
            payload,
            SearchOutput { schema_version: protocol_version(), opportunities: output.recommended_opportunities },
        ).await?
    };
    Ok(json!({"targetId":target_id,"decision":output.decision,"nextStatus":next_status,"statusApplied":status_applied,"draftsApplied":drafts_applied,
        "summary":if !drafts_applied { "历史回复结果已保留在“编辑与修订”；已有更新的回复或任务，未替换当前草稿、保留当前状态，也未导入推荐机会。" }
            else if status_applied { "回复分析已保存，联系进度已更新，等待审核。" }
            else { "回复分析已保存；联系状态已被修改或旧任务缺少状态快照，保留当前状态，请手动确认建议。" },
        "recommendedAction":output.recommended_action,"referrals":referrals}))
}

fn publish_reply_result(paths: &AppPaths, job: &JobSummary, payload: &Value, output: &ReplyOutput, next_status: &str) -> Result<(bool, bool)> {
    let target_id = job.target_id.as_deref().context("回复任务缺少联系人 ID")?;
    let conn = db::connect(&paths.database)?;
    let tx = db::publication_transaction(&conn)?;
    let application_id: String = tx.query_row(
        "SELECT application_id FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL",
        [target_id], |row| row.get(0),
    )?;
    let analysis = format!(
        "# 回复处理判断\n\n- 决策：{}\n- 建议阶段（以当前联系人状态为准）：{}\n- 建议动作：{}\n\n{}\n",
        output.decision,
        if next_status == "shelved" { "搁置" } else { "跟进" },
        output.recommended_action,
        output.summary_zh
    );
    // Serialize the freshness check, artifact publication and status update. Every
    // result has its own immutable directory, including stale/legacy replays.
    let drafts_applied = reply_result_is_current(&tx, job, payload, target_id)?;
    let directory = target_directory(paths, target_id).join("reply-history").join(Uuid::new_v4().simple().to_string());
    fs::create_dir_all(&directory)?;
    let note = if drafts_applied { "回复 Agent 结果已设为当前草稿；原版本保留。" }
        else { "历史任务结果，仅留存供审核；未替换当前草稿，也未改变联系状态。" };
    for (kind, language, filename, text) in [
        ("reply_analysis", "zh", "reply-analysis.md", analysis.as_str()),
        ("followup_email", "en", "followup-email-en.md", output.draft_reply_en.as_str()),
        ("followup_email", "zh", "followup-email-zh.md", output.draft_reply_zh.as_str()),
    ] {
        let path = directory.join(filename);
        fs::write(&path, text)?;
        let relative = display_path(&paths.data_root, &path);
        let previous: Option<String> = tx.query_row(
            "SELECT path FROM contact_target_artifacts WHERE target_id=?1 AND artifact_type=?2 AND language=?3",
            params![target_id,kind,language], |row| row.get(0),
        ).optional()?;
        if drafts_applied {
            tx.execute(
                "INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id)
                 VALUES(?1,?2,?3,?4,?5) ON CONFLICT(target_id,artifact_type,language) DO UPDATE
                 SET path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                params![target_id,kind,language,relative,application_id],
            )?;
        }
        let revision = format!("reply-version-{}",Uuid::new_v4().simple());
        tx.execute(
            "INSERT INTO artifact_revisions(id,application_id,artifact_type,language,artifact_path,backup_path,editor,note)
             VALUES(?1,?2,?3,?4,?5,?6,'codex',?7)", params![revision,application_id,kind,language,relative,previous,note],
        )?;
        tx.execute("INSERT INTO revision_contact_targets(revision_id,target_id) VALUES(?1,?2)",params![revision,target_id])?;
        tx.execute(
            "INSERT INTO revision_change_sets(id,revision_id,job_id,summary,provider_id,model_id,reasoning)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![format!("changes:{revision}"),revision,job.id,note,job.provider_id,job.model_id,job.reasoning],
        )?;
    }
    if drafts_applied { record_sources(&tx,"contact_target",target_id,&output.sources)?; }
    tx.execute(
        "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id)
         VALUES(?1,'reply_analysis',?2,?2)",
        params![job.id,target_id],
    )?;
    let status_applied = drafts_applied && db::apply_agent_status_in_transaction(&tx, target_id, next_status, payload.get("_statusVersion").and_then(Value::as_i64))?;
    tx.commit()?;
    Ok((drafts_applied, status_applied))
}

fn reply_result_is_current(conn: &Connection, job: &JobSummary, payload: &Value, target_id: &str) -> Result<bool> {
    let newer_job: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_jobs newer JOIN native_jobs current ON current.id=?1
         WHERE newer.job_type='reply_followup' AND newer.target_id=?2
           AND (newer.created_at>current.created_at OR (newer.created_at=current.created_at AND newer.rowid>current.rowid)))",
        params![job.id,target_id], |row| row.get(0),
    )?;
    if newer_job { return Ok(false); }
    let latest_reply: Option<String> = conn.query_row(
        "SELECT r.id FROM application_replies r JOIN reply_contact_targets m ON m.reply_id=r.id
         WHERE m.target_id=?1 ORDER BY COALESCE(r.received_at,r.created_at) DESC,r.rowid DESC LIMIT 1",
        [target_id], |row| row.get(0),
    ).optional()?;
    Ok(match latest_reply {
        Some(id) => payload.get("replyId").and_then(Value::as_str) == Some(id.as_str()),
        // Older tasks can lack a saved reply; do not let them replace a published
        // draft when freshness cannot be established.
        None => !conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM contact_target_artifacts WHERE target_id=?1 AND artifact_type IN ('reply_analysis','followup_email'))",
            [target_id], |row| row.get::<_,bool>(0),
        )?,
    })
}

fn reply_decision_status(decision: &str) -> Result<&'static str> {
    match decision {
        "stop" => Ok("shelved"),
        "continue" | "investigate_referral" | "clarify" | "wait" => Ok("follow_up"),
        _ => bail!("回复 Agent 返回了无效决策：{decision}"),
    }
}

struct ImportedTarget {
    target_id: String,
    application_id: String,
    opportunity_id: String,
}

fn upsert_internship_target(
    paths: &AppPaths,
    job_id: &str,
    value: &FoundInternshipOpportunity,
) -> Result<ImportedTarget> {
    validate_internship_opportunity(value)?;
    let normalized_checklist = normalize_checklist_items(&value.checklist)?;
    let identity = internship_opportunity_identity(value);
    let conn = db::connect(&paths.database)?;
    let tx = db::publication_transaction(&conn)?;
    let opportunity_id: String = if let Some(id) = tx
        .query_row(
            "SELECT id FROM opportunities
             WHERE identity_key=?1 OR lower(rtrim(COALESCE(source_url,''),'/'))=?2
             LIMIT 1",
            params![identity, internship_canonical_url(&value.source_url)],
            |row| row.get(0),
        )
        .optional()?
    {
        tx.execute(
            "UPDATE opportunities
             SET last_verified_at=?2,fit_score=MAX(COALESCE(fit_score,0),?3),
                 status='open',opportunity_type='industry_internship',summary=?4,deadline=?5,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE id=?1",
            params![id, value.verified_at, value.fit_score, value.summary, value.deadline],
        )?;
        id
    } else {
        let id = format!("opportunity-internship-{}", Uuid::new_v4().simple());
        tx.execute(
            "INSERT INTO opportunities(
                id,identity_key,title,organization,department,country,region,opportunity_type,status,
                deadline,source_url,source_title,discovered_at,last_verified_at,summary,keywords_json,
                fit_score,priority,notes
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,'industry_internship','open',?8,?9,?10,?11,?11,?12,?13,?14,'review',?15)",
            params![
                id,
                identity,
                value.title,
                value.organization,
                value.department,
                value.country,
                value.region,
                value.deadline,
                value.source_url,
                value.source_title,
                value.verified_at,
                value.summary,
                serde_json::to_string(&value.keywords)?,
                value.fit_score,
                value.external_id,
            ],
        )?;
        id
    };
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT id,application_id FROM contact_targets_v2
             WHERE opportunity_id=?1 AND archived_at IS NULL AND normalized_name='applicationportal'
             LIMIT 1",
            [&opportunity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (target_id, application_id) = if let Some((target_id, application_id)) = existing {
        tx.execute(
            "UPDATE contact_targets_v2
             SET fit_score=?2,source_url=?3,submission_status=CASE
                    WHEN submission_status='not_set' THEN 'portal_pending' ELSE submission_status END,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE id=?1",
            params![target_id, value.fit_score, value.source_url],
        )?;
        (target_id, application_id)
    } else {
        let application_id = format!("app-internship-{}", Uuid::new_v4().simple());
        let target_id = format!("target-internship-{}", Uuid::new_v4().simple());
        tx.execute(
            "INSERT INTO applications(id,opportunity_id,pi_id,status,notes)
             VALUES(?1,?2,NULL,'ready_for_review',?3)",
            params![
                application_id,
                opportunity_id,
                format!(
                    "Internship discovery from native job {job_id}. Eligibility: {}. Location: {}",
                    value.eligibility_summary,
                    value.location.as_deref().unwrap_or("not specified")
                ),
            ],
        )?;
        tx.execute(
            "INSERT INTO contact_targets_v2(
                id,application_id,opportunity_id,pi_id,name,normalized_name,email,normalized_email,
                organization,title,fit_score,priority,status,submission_status,source_url,identity_key
             ) VALUES(?1,?2,?3,NULL,'Application portal','applicationportal',NULL,NULL,?4,?5,?6,100,
                      'ready_to_contact','portal_pending',?7,?8)",
            params![
                target_id,
                application_id,
                opportunity_id,
                value.organization,
                value.title,
                value.fit_score,
                value.source_url,
                format!("{opportunity_id}::applicationportal"),
            ],
        )?;
        (target_id, application_id)
    };
    tx.commit()?;
    let conn = db::connect(&paths.database)?;
    store_text_artifact(
        paths,
        &conn,
        &target_id,
        &application_id,
        "fit_analysis",
        "en",
        "fit-analysis.md",
        &value.fit_analysis,
    )?;
    store_text_artifact(
        paths,
        &conn,
        &target_id,
        &application_id,
        "fit_analysis",
        "zh",
        "fit-analysis-zh.md",
        &value.fit_analysis_zh,
    )?;
    replace_checklist_with_conn(&conn, &target_id, &normalized_checklist)?;
    record_sources(&conn, "opportunity", &opportunity_id, &value.sources)?;
    conn.execute(
        "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id)
         VALUES(?1,'contact_target',?2,?2)",
        params![job_id, target_id],
    )?;
    Ok(ImportedTarget { target_id, application_id, opportunity_id })
}

fn upsert_discovered_contact(
    paths: &AppPaths,
    job_id: &str,
    opportunity: &FoundOpportunity,
    contact: &FoundContact,
    opportunity_id: Option<&str>,
    confirmed_source: Option<&str>,
) -> Result<ImportedTarget> {
    validate_opportunity(opportunity)?;
    validate_contact_identity(contact)?;
    for source in &opportunity.sources { validate_source(source)?; }
    let conn = db::connect(&paths.database)?;
    // Reserve the writer before the dedupe reads. A deferred transaction can read a
    // stale WAL snapshot and then fail with SQLITE_BUSY_SNAPSHOT when one of the other
    // five workers commits before this import attempts its first write.
    let tx = db::publication_transaction(&conn)?;
    let opportunity_id = upsert_opportunity(&tx, opportunity, opportunity_id, confirmed_source)?;
    if db::opportunity_shelved(&tx, &opportunity_id)? { return Err(ShelvedOpportunityResult.into()) }
    if !opportunity_result_is_current(&tx, &opportunity_id, &opportunity.verified_at)? {
        return Err(StaleOpportunityResult.into());
    }
    let normalized_email = contact.email.as_deref().map(normalize_email);
    let normalized_name = normalize_person(&contact.name);
    let existing = matching_contact_target(&tx, &opportunity_id, contact)?;
    let pi_id = upsert_pi(&tx, opportunity, contact)?;
    let (target_id, application_id) = if let Some(value)=existing {
        let shelved: bool = tx.query_row("SELECT shelved_at IS NOT NULL FROM contact_targets_v2 WHERE id=?1", [&value.0], |r| r.get(0))?;
        if shelved { return Err(ShelvedOpportunityResult.into()) }
        tx.execute(
            "UPDATE contact_targets_v2 SET fit_score=?2,priority=?3,pi_id=?4,
             email=COALESCE(?5,email),normalized_email=COALESCE(?6,normalized_email),
             source_url=?7,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id=?1",
            params![value.0,contact.fit_score,contact.priority,pi_id,contact.email,normalized_email,opportunity.source_url],
        )?;
        value
    } else {
        let application_id=format!("app-native-{}",Uuid::new_v4().simple());
        let target_id=format!("target-native-{}",Uuid::new_v4().simple());
        tx.execute(
            "INSERT INTO applications(id,opportunity_id,pi_id,status,notes)
             VALUES(?1,?2,?3,'ready_for_review',?4)",
            params![application_id,opportunity_id,pi_id,format!("Imported from native job {job_id}")],
        )?;
        let identity=format!("{opportunity_id}::{}",normalized_email.clone().unwrap_or_else(||normalized_name.clone()));
        tx.execute(
            "INSERT INTO contact_targets_v2(
                id,application_id,opportunity_id,pi_id,name,normalized_name,email,normalized_email,
                organization,title,fit_score,priority,status,source_url,identity_key,material_status
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'ready_to_contact',?13,?14,'pending')",
            params![target_id,application_id,opportunity_id,pi_id,contact.name,normalized_name,
                contact.email,normalized_email,opportunity.organization,opportunity.title,
                contact.fit_score,contact.priority,opportunity.source_url,identity],
        )?;
        (target_id,application_id)
    };
    record_sources(&tx,"opportunity",&opportunity_id,&opportunity.sources)?;
    tx.execute(
        "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id) VALUES(?1,'contact_target',?2,?2)",
        params![job_id,target_id],
    )?;
    tx.commit()?;
    Ok(ImportedTarget{target_id,application_id,opportunity_id})
}

/// Files are complete before a single transaction publishes their paths.
type SavedMaterials = BTreeMap<(String, String), PathBuf>;

fn saved_materials(conn: &Connection, paths: &AppPaths, target: &str) -> Result<SavedMaterials> {
    let mut result = BTreeMap::new();
    let mut statement = conn.prepare("SELECT artifact_type,language,path FROM contact_target_artifacts WHERE target_id=?1")?;
    let root = paths.data_root.canonicalize()?;
    for row in statement.query_map([target], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))? {
        let (kind, language, stored) = row?;
        let path = paths.data_root.join(stored);
        if !path.is_file() { continue; }
        let path = path.canonicalize()?;
        if !path.starts_with(&root) { bail!("材料路径不在当前数据目录内") }
        if fs::metadata(&path)?.len() > 0 { result.insert((kind,language), path); }
    }
    Ok(result)
}

fn package_complete(conn: &Connection, target: &str, saved: &SavedMaterials, language: &str) -> Result<bool> {
    let mut required = vec![("cv_pdf","en"),("cv_data","und"),("cv_typst","en")];
    for lang in ["en","zh"] {
        if language == "bilingual" || language == lang {
            for kind in ["email","fit_analysis","pi_profile"] { required.push((kind,lang)); }
        }
    }
    Ok(required.iter().all(|(kind,lang)| saved.contains_key(&(kind.to_string(),lang.to_string())))
        && conn.query_row("SELECT EXISTS(SELECT 1 FROM contact_target_checklist WHERE target_id=?1)", [target], |row| row.get::<_,bool>(0))?)
}

fn record_material_language(conn: &Connection, paths: &AppPaths, target: &str, language: &str, replace: bool) -> Result<()> {
    if !matches!(language,"en"|"zh"|"bilingual") { bail!("材料语言设置无效") }
    let existing: Option<String> = conn.query_row("SELECT path FROM contact_target_artifacts WHERE target_id=?1 AND artifact_type='material_requirements' AND language='und'",[target],|r|r.get(0)).optional()?;
    if let Some(stored) = existing {
        let saved = fs::read(paths.data_root.join(stored)).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
        if saved.as_ref().is_some_and(|v|v.get("language").and_then(Value::as_str)==Some(language)) || (!replace && saved.is_some()) { return Ok(()) }
    }
    let directory = crate::materials::new_material_version(paths,target);
    fs::create_dir_all(&directory)?;
    let path = directory.join("requirements.json");
    fs::write(&path,serde_json::to_vec(&json!({"schemaVersion":1,"language":language}))?)?;
    conn.execute("INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id)
        SELECT id,'material_requirements','und',?2,application_id FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL
        ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![target,display_path(&paths.data_root,&path)])?;
    Ok(())
}

/// All material writers use the current package's requirements and actual files.
/// A failed attempt is diagnostic data, not permission to demote a newer package.
pub(crate) fn reconcile_material_state(conn: &Connection, paths: &AppPaths, target: &str, failure: Option<&str>) -> Result<bool> {
    let internship: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM contact_targets_v2 t JOIN opportunities o ON o.id=t.opportunity_id WHERE t.id=?1 AND o.opportunity_type='industry_internship')",[target],|r|r.get(0))?;
    if internship { return Ok(false) }
    let saved = saved_materials(conn,paths,target)?;
    let language = if let Some(path) = saved.get(&("material_requirements".into(),"und".into())) {
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        if value.get("schemaVersion").and_then(Value::as_u64)!=Some(1) { bail!("材料要求版本无效") }
        value.get("language").and_then(Value::as_str).filter(|v|matches!(*v,"en"|"zh"|"bilingual")).context("材料语言记录无效")?.to_owned()
    } else { crate::onboarding::load(paths)?.preferred_language };
    let ready = package_complete(conn,target,&saved,&language)?;
    conn.execute("UPDATE contact_targets_v2 SET material_status=?2,
        material_error=CASE WHEN ?2='ready' THEN NULL ELSE COALESCE(?3,material_error,'材料尚未补齐') END,
        updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id=?1 AND archived_at IS NULL",
        params![target,if ready {"ready"} else {"pending"},failure])?;
    Ok(ready)
}

fn saved_opportunity_closed(conn: &Connection, id: &str) -> Result<bool> {
    let (status,deadline):(Option<String>,Option<String>)=conn.query_row("SELECT status,deadline FROM opportunities WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    Ok(status.as_deref()==Some("closed") || deadline.as_deref().and_then(|v|chrono::NaiveDate::parse_from_str(v,"%Y-%m-%d").ok()).is_some_and(|d|d<Utc::now().date_naive()))
}

fn cv_material_fingerprint(saved: &SavedMaterials) -> Result<String> {
    let mut hash = Sha256::new();
    for ((kind,lang), path) in saved.iter().filter(|((kind,_),_)| kind.starts_with("cv_")) {
        hash.update(kind); hash.update(lang); hash.update(fs::read(path)?);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn install_material_package(paths: &AppPaths, target: &ImportedTarget, contact: &FoundContact, directory: &Path, language: &str, cv_snapshot: &str) -> Result<()> {
    let artifacts = [
        ("cv_typst","en","cv.typ"), ("cv_data","und","cv-data.json"), ("cv_pdf","en","cv.pdf"),
        ("email","en","email-en.md"), ("email","zh","email-zh.md"),
        ("fit_analysis","en","fit-analysis.md"), ("fit_analysis","zh","fit-analysis-zh.md"),
        ("pi_profile","en","pi-profile.md"), ("pi_profile","zh","pi-profile-zh.md"),
    ];
    for (name,text) in [
        ("email-en.md",&contact.materials.email_en), ("email-zh.md",&contact.materials.email_zh),
        ("fit-analysis.md",&contact.materials.fit_analysis), ("fit-analysis-zh.md",&contact.materials.fit_analysis_zh),
        ("pi-profile.md",&contact.materials.pi_profile), ("pi-profile-zh.md",&contact.materials.pi_profile_zh),
    ] {
        if !text.trim().is_empty() { fs::write(directory.join(name),text)?; }
    }
    fs::write(directory.join("provenance.json"), serde_json::to_vec_pretty(&json!({
        "createdAt":Utc::now().to_rfc3339(),"targetId":target.target_id,"reviewRequired":true
    }))?)?;
    let conn = db::connect(&paths.database)?;
    let tx = db::publication_transaction(&conn)?;
    let existing = saved_materials(&tx, paths, &target.target_id)?;
    if db::opportunity_shelved(&tx, &target.opportunity_id)? { bail!("机会已搁置，未发布新的申请材料") }
    let shelved: bool = tx.query_row("SELECT shelved_at IS NOT NULL FROM contact_targets_v2 WHERE id=?1", [&target.target_id], |r| r.get(0))?;
    if shelved { bail!("联系人已搁置，未发布新的申请材料") }
    if saved_opportunity_closed(&tx,&target.opportunity_id)? { bail!("机会已关闭，未发布新的申请材料") }
    if cv_material_fingerprint(&existing)? != cv_snapshot { bail!("CV 在任务执行期间发生修改，已保留你的版本；请重新确认后继续补齐") }
    for (kind, language, file) in artifacts {
        if existing.contains_key(&(kind.into(),language.into())) { continue; }
        let path = directory.join(file);
        if !path.is_file() { continue; }
        tx.execute(
            "INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![target.target_id,kind,language,display_path(&paths.data_root,&path),target.application_id],
        )?;
    }
    let checklist = normalize_checklist_items(&contact.materials.checklist)?;
    for item in checklist {
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM contact_target_checklist WHERE target_id=?1 AND item_type=?2)", params![target.target_id,item.item_type], |row|row.get(0))?;
        if exists { continue; }
        tx.execute(
            "INSERT INTO contact_target_checklist(id,target_id,item_type,required,status,origin,evidence,source_url,note,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![format!("check-native-{}",Uuid::new_v4().simple()),target.target_id,item.item_type,item.required as i64,item.status,item.origin,item.evidence,item.source_url,item.note,item.sort_order],
        )?;
    }
    if !package_complete(&tx, &target.target_id, &saved_materials(&tx, paths, &target.target_id)?, language)? {
        bail!("必需材料仍不完整，未标记完成；原材料保持不变")
    }
    record_material_language(&tx,paths,&target.target_id,language,true)?;
    reconcile_material_state(&tx,paths,&target.target_id,None)?;
    tx.commit()?;
    Ok(())
}

// Legacy fixture helper; production imports always render before publishing.
#[cfg(test)]
fn upsert_complete_contact(paths:&AppPaths,job_id:&str,opportunity:&FoundOpportunity,contact:&FoundContact)->Result<ImportedTarget> {
    validate_contact(contact)?;
    upsert_discovered_contact(paths,job_id,opportunity,contact,None,None)
}

fn ensure_continuation_scope(conn: &Connection, id: &str, opportunities: &[FoundOpportunity], confirmed_source: Option<&str>) -> Result<()> {
    let saved=saved_opportunity_identities(conn)?.into_iter().find(|row|row.id==id).context("原机会已不存在，无法继续完善")?;
    for value in opportunities {
        // An explicit identifier conflict cannot be overridden by a reused URL,
        // a matching title, or confirmation of the URL alone.
        if identity::inconsistent_id(&incoming_identity(value)) || identity::conflicts(&saved.facts(),&incoming_identity(value)) {
            return Err(OpportunityIdentityConflict(format!("{} 的官方岗位编号与原机会冲突，请作为独立机会核对", value.title)).into());
        }
        let manually_confirmed = confirmed_source.is_some_and(|url| identity::canonical_source(url) == identity::canonical_source(&value.source_url))
            && identity::same_organization(&saved.organization,&value.organization);
        if saved.compare(value)!=identity::Match::Same && !manually_confirmed {
            bail!("无法确认 {} / {} 与原机会为同一岗位。请核对原来源或同一机构官方岗位编号；同名不作为合并依据。", value.organization, value.title)
        }
    }
    Ok(())
}

fn matching_opportunity(conn: &Connection, value: &FoundOpportunity) -> Result<Option<String>> {
    if identity::inconsistent_id(&incoming_identity(value)) {
        return Err(OpportunityIdentityConflict("返回编号与来源网址不一致，未入库".into()).into());
    }
    let rows=saved_opportunity_identities(conn)?;
    let mut matches:Vec<_>=rows.into_iter().filter_map(|row| {
        let decision=row.compare(value);
        (decision!=identity::Match::Different).then_some((row,decision))
    }).collect();
    if matches.iter().any(|(r,_)|r.visible) {matches.retain(|(r,_)|r.visible);}
    if matches.len()==1 && matches[0].1==identity::Match::Same {return Ok(Some(matches.remove(0).0.id));}
    if !matches.is_empty() {
        let details=matches.iter().take(5).map(|(r,d)|format!("{} / {} [{}]：{}",r.organization,r.title,r.id,
            match d {identity::Match::Review(reason)=>*reason,_=>"匹配到多个机会"})).collect::<Vec<_>>().join("；");
        return Err(OpportunityIdentityConflict(format!("{}；请从对应已有机会继续完善并确认来源，未创建新申请卡",details)).into());
    }
    Ok(None)
}

fn opportunity_result_is_current(conn: &Connection, id: &str, verified_at: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT julianday(last_verified_at) IS NULL OR julianday(last_verified_at)<=julianday(?2)
         FROM opportunities WHERE id=?1", params![id, verified_at], |row| row.get(0),
    )?)
}

fn upsert_opportunity(conn:&Connection,value:&FoundOpportunity,scoped_id:Option<&str>,confirmed_source:Option<&str>)->Result<String>{
    let availability = opportunity_availability(value);
    let identity=opportunity_identity(value);
    let existing:Option<String>=if let Some(id) = scoped_id {
        ensure_continuation_scope(conn, id, std::slice::from_ref(value), confirmed_source)?;
        Some(id.to_owned())
    } else { matching_opportunity(conn, value)? };
    if let Some(id)=existing{
        let (old_deadline, old_source, old_title, old_organization, old_kind): (Option<String>, Option<String>, String, String, Option<String>) = conn.query_row(
            "SELECT deadline,source_url,title,organization,opportunity_type FROM opportunities WHERE id=?1",
            [&id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        )?;
        let deadline = value.deadline.as_deref().filter(|v| meaningful_metadata(v)).map(str::to_owned).or(old_deadline);
        let mut verified = value.clone();
        verified.deadline = deadline.clone();
        // An omitted/unknown kind cannot erase a previously established category.
        // Explicit newly verified formal/prospective kinds replace the old kind.
        verified.opportunity_type = value.opportunity_type.as_deref()
            .filter(|kind| meaningful_metadata(kind) && *kind != "other")
            .map(str::to_owned).or(old_kind);
        let changed = conn.execute(
            "UPDATE opportunities SET last_verified_at=?2,fit_score=MAX(COALESCE(fit_score,0),?3),
             status=?4,deadline=?5,source_url=?6,source_title=COALESCE(?7,source_title),
             summary=COALESCE(?8,summary),identity_key=COALESCE(?9,identity_key),
             opportunity_type=COALESCE(?10,opportunity_type),title=COALESCE(?11,title),
             organization=COALESCE(?12,organization),department=COALESCE(?13,department),
             country=COALESCE(?14,country),region=COALESCE(?15,region),
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id=?1
             AND (julianday(last_verified_at) IS NULL OR julianday(last_verified_at)<=julianday(?2))",
            params![id,value.verified_at,value.contacts.iter().map(|item|item.fit_score).fold(0.0,f64::max),opportunity_availability(&verified),
                deadline,value.source_url,value.source_title.as_deref().filter(|v| meaningful_metadata(v)),
                Some(value.summary.as_str()).filter(|v| meaningful_metadata(v)),
                identity.starts_with("external-v2:").then_some(identity.as_str()),verified.opportunity_type,
                Some(value.title.as_str()).filter(|v| meaningful_metadata(v)),
                Some(value.organization.as_str()).filter(|v| meaningful_metadata(v)),
                value.department.as_deref().filter(|v| meaningful_metadata(v)),
                value.country.as_deref().filter(|v| meaningful_metadata(v)),
                value.region.as_deref().filter(|v| meaningful_metadata(v))],
        )?;
        if changed > 0 {
            // Refresh copied display fields only while they still inherit the old
            // opportunity values. Do not rewrite contact-specific edits/history.
            conn.execute(
                "UPDATE contact_targets_v2 SET
                   source_url=CASE WHEN source_url IS NULL OR source_url=?3 THEN ?2 ELSE source_url END,
                   title=CASE WHEN title IS NULL OR title='' OR title=?4 THEN (SELECT title FROM opportunities WHERE id=?1) ELSE title END,
                   organization=CASE WHEN organization IS NULL OR organization='' OR organization=?5 THEN (SELECT organization FROM opportunities WHERE id=?1) ELSE organization END
                 WHERE opportunity_id=?1 AND archived_at IS NULL",
                params![id,value.source_url,old_source,old_title,old_organization],
            )?;
        }
        return Ok(id)
    }
    let id=format!("opportunity-native-{}",Uuid::new_v4().simple());
    conn.execute(
        "INSERT INTO opportunities(
            id,identity_key,title,organization,department,country,region,opportunity_type,status,
            deadline,source_url,source_title,discovered_at,last_verified_at,summary,keywords_json,fit_score,priority,notes
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?18,?9,?10,?11,?12,?12,?13,?14,?15,?16,?17)",
        params![id,identity,value.title,value.organization,value.department,value.country,value.region,
            value.opportunity_type.as_deref().unwrap_or("formal_position"),value.deadline,value.source_url,
            value.source_title,value.verified_at,value.summary,serde_json::to_string(&value.keywords)?,
            value.contacts.iter().map(|item|item.fit_score).fold(0.0,f64::max),"review",value.external_id,availability],
    )?;
    Ok(id)
}

fn meaningful_metadata(value: &str) -> bool {
    !matches!(value.trim().to_lowercase().as_str(), "" | "unknown" | "n/a" | "null" | "待确认" | "待核验" | "待核实")
}

struct ContactIdentity<'a> {
    name: &'a str,
    email: Option<&'a str>,
    homepage: Option<&'a str>,
}

impl<'a> From<&'a FoundContact> for ContactIdentity<'a> {
    fn from(contact: &'a FoundContact) -> Self {
        Self { name: &contact.name, email: contact.email.as_deref(), homepage: contact.homepage_url.as_deref() }
    }
}

fn compare_contact_identity(saved: &ContactIdentity<'_>, incoming: &ContactIdentity<'_>) -> identity::Match {
    let identity_name = |value: &str| normalize_person(&value.split_whitespace().skip_while(|part| {
        matches!(part.trim_end_matches('.').to_ascii_lowercase().as_str(), "prof" | "professor" | "dr" | "doctor" | "mr" | "mrs" | "ms")
    }).collect::<Vec<_>>().join(" "));
    let same_name = identity_name(saved.name) == identity_name(incoming.name);
    let email = |value: Option<&str>| value.filter(|v| !v.trim().is_empty()).map(normalize_email);
    let (saved_email, incoming_email) = (email(saved.email), email(incoming.email));
    let same_email = saved_email.is_some() && saved_email == incoming_email;
    let homepage = |value: Option<&str>| value.filter(|v| is_http_url(v)).map(identity::canonical_source);
    let (saved_home, incoming_home) = (homepage(saved.homepage), homepage(incoming.homepage));
    let same_home = saved_home.is_some() && saved_home == incoming_home;
    if same_email && !same_name && !same_home {
        return identity::Match::Review("相同邮箱对应不同姓名，缺少同一人的个人主页证据");
    }
    if same_name && saved_email.is_some() && incoming_email.is_some() && !same_email
        && saved_home.is_some() && incoming_home.is_some() && !same_home {
        return identity::Match::Review("同名联系人具有不同邮箱和个人主页，不能确定为同一人");
    }
    // Preserve legitimate email corrections and legacy name-only records where
    // there is no contradictory identity evidence. A lab URL is not a person ID.
    if same_name || same_email { identity::Match::Same } else { identity::Match::Different }
}

fn conflicting_contacts(contacts: &[FoundContact]) -> HashSet<usize> {
    let mut conflicts = HashSet::new();
    for (left, contact) in contacts.iter().enumerate() {
        for (right, other) in contacts.iter().enumerate().skip(left + 1) {
            if matches!(compare_contact_identity(&contact.into(), &other.into()), identity::Match::Review(_)) {
                conflicts.insert(left);
                conflicts.insert(right);
            }
        }
    }
    conflicts
}

fn conflicting_discovered_contacts(candidates: &[(f64, FoundOpportunity, FoundContact, String)]) -> HashSet<usize> {
    let mut conflicts = HashSet::new();
    for (left, (_, opportunity, contact, id)) in candidates.iter().enumerate() {
        for (right, (_, other_opportunity, other, other_id)) in candidates.iter().enumerate().skip(left + 1) {
            let same_email = contact.email.as_deref().filter(|email| !email.trim().is_empty())
                .is_some_and(|email| other.email.as_deref().is_some_and(|other| normalize_email(email) == normalize_email(other)));
            // Match the existing target and global PI registry scopes; unrelated
            // institutions may legitimately have different people with one name.
            if id != other_id && !same_email
                && !identity::same_organization(&opportunity.organization, &other_opportunity.organization) { continue; }
            if matches!(compare_contact_identity(&contact.into(), &other.into()), identity::Match::Review(_)) {
                conflicts.insert(left);
                conflicts.insert(right);
            }
        }
    }
    conflicts
}

fn matching_contact_target(conn: &Connection, opportunity_id: &str, contact: &FoundContact) -> Result<Option<(String, String)>> {
    let mut statement = conn.prepare(
        "SELECT t.id,t.application_id,t.name,t.email,p.homepage_url
         FROM contact_targets_v2 t LEFT JOIN pis p ON p.id=t.pi_id
         WHERE t.opportunity_id=?1 AND t.archived_at IS NULL",
    )?;
    let mut matches = Vec::new();
    for row in statement.query_map([opportunity_id], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,Option<String>>(4)?)))? {
        let (id, application, name, email, homepage) = row?;
        match compare_contact_identity(&ContactIdentity { name: &name, email: email.as_deref(), homepage: homepage.as_deref() }, &contact.into()) {
            identity::Match::Same => matches.push((id,application)),
            identity::Match::Review(reason) => return Err(ContactIdentityConflict(format!("{}：{reason}",contact.name)).into()),
            identity::Match::Different => {},
        }
    }
    if matches.len() > 1 {
        return Err(ContactIdentityConflict(format!("{} 同时匹配多个已有联系人，请核对后再继续",contact.name)).into());
    }
    Ok(matches.pop())
}

fn upsert_pi(conn:&Connection,opportunity:&FoundOpportunity,contact:&FoundContact)->Result<String>{
    let normalized_email=contact.email.as_deref().map(normalize_email);
    let mut statement = conn.prepare("SELECT id,name,email,homepage_url,institution FROM pis")?;
    let mut matches = Vec::new();
    for row in statement.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?)))? {
        let (id, name, email, homepage, institution) = row?;
        let same_email = normalized_email.is_some() && email.as_deref().map(normalize_email) == normalized_email;
        if !same_email && !identity::same_organization(institution.as_deref().unwrap_or(""),&opportunity.organization) { continue; }
        match compare_contact_identity(&ContactIdentity { name: &name, email: email.as_deref(), homepage: homepage.as_deref() }, &contact.into()) {
            identity::Match::Same => matches.push(id),
            identity::Match::Review(reason) => return Err(ContactIdentityConflict(format!("{}：{reason}",contact.name)).into()),
            identity::Match::Different => {},
        }
    }
    if matches.len() > 1 {
        return Err(ContactIdentityConflict(format!("{} 同时匹配多份 PI 身份记录，请核对后再继续",contact.name)).into());
    }
    if let Some(id)=matches.pop(){
        conn.execute(
            "UPDATE pis SET email=COALESCE(?2,email),homepage_url=COALESCE(?3,homepage_url),source_checked_at=?4
             WHERE id=?1 AND (julianday(source_checked_at) IS NULL OR julianday(source_checked_at)<=julianday(?4))",
            params![id,contact.email,contact.homepage_url.as_deref().filter(|v| is_http_url(v)),opportunity.verified_at],
        )?;
        return Ok(id)
    }
    let id=format!("pi-native-{}",Uuid::new_v4().simple());
    conn.execute(
        "INSERT INTO pis(id,name,institution,department,country,region,email,homepage_url,lab_url,research_summary,source_checked_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![id,contact.name,opportunity.organization,opportunity.department,opportunity.country,
            opportunity.region,contact.email,contact.homepage_url,contact.lab_url,contact.research_summary,opportunity.verified_at],
    )?;
    Ok(id)
}

fn store_text_artifact(paths:&AppPaths,conn:&Connection,target_id:&str,application_id:&str,artifact_type:&str,language:&str,filename:&str,content:&str)->Result<()> {
    if content.trim().is_empty(){bail!("材料 {artifact_type}/{language} 为空")}
    let directory=crate::materials::new_material_version(paths,target_id);
    fs::create_dir_all(&directory)?;
    let path=directory.join(filename);
    fs::write(&path,content)?;
    let relative=display_path(&paths.data_root,&path);
    conn.execute(
        "INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id)
         VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET path=excluded.path,
           updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![target_id,artifact_type,language,relative,application_id],
    )?;
    Ok(())
}

fn replace_checklist(paths:&AppPaths,target_id:&str,items:&[ChecklistOutput])->Result<()> {
    let conn=db::connect(&paths.database)?;
    replace_checklist_with_conn(&conn,target_id,items)
}

fn replace_checklist_with_conn(conn:&Connection,target_id:&str,items:&[ChecklistOutput])->Result<()> {
    let items = normalize_checklist_items(items)?;
    let tx = db::publication_transaction(conn)?;
    tx.execute("DELETE FROM contact_target_checklist WHERE target_id=?1",[target_id])?;
    for item in &items {
        tx.execute(
            "INSERT INTO contact_target_checklist(
                id,target_id,item_type,required,status,origin,evidence,source_url,note,sort_order
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![format!("check-native-{}",Uuid::new_v4().simple()),target_id,item.item_type,
                item.required as i64,item.status,item.origin,item.evidence,item.source_url,item.note,item.sort_order],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn normalize_checklist_items(items:&[ChecklistOutput])->Result<Vec<ChecklistOutput>> {
    if items.is_empty() { bail!("申请清单为空") }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(items.len());
    for (index, mut item) in items.iter().cloned().enumerate() {
        if !matches!(item.status.as_str(), "ready" | "review" | "missing") {
            bail!("清单项 {} 的状态无效：{}", index + 1, item.status)
        }
        if !matches!(item.origin.as_str(), "verified" | "inferred") {
            bail!("清单项 {} 的来源类型无效：{}", index + 1, item.origin)
        }
        item.source_url = item.source_url.take().and_then(|url| {
            let trimmed = url.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        });
        if let Some(url) = item.source_url.as_deref() {
            if !is_http_url(url) { bail!("清单项 {} 的来源 URL 无效", index + 1) }
        }
        let raw = normalize_text(&item.item_type);
        let base = if raw.is_empty() || raw == "item" {
            format!("agent-item-{:03}", if item.sort_order > 0 { item.sort_order } else { (index + 1) as i64 * 10 })
        } else { raw };
        let mut key = base.clone();
        let mut suffix = 2;
        while !seen.insert(key.clone()) {
            key = format!("{base}-{suffix}");
            suffix += 1;
        }
        normalized.push(ChecklistOutput { item_type: key.replace('-', "_"), ..item });
    }
    Ok(normalized)
}

fn record_sources(conn:&Connection,entity_type:&str,entity_id:&str,sources:&[SourceEvidence])->Result<()> {
    for source in sources {
        validate_source(source)?;
        let digest=format!("{:x}",Sha256::digest(format!("{entity_type}:{entity_id}:{}",canonical_url(&source.url)).as_bytes()));
        conn.execute(
            "INSERT OR IGNORE INTO native_source_evidence(id,entity_type,entity_id,title,url,checked_at,evidence_type)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![format!("source:{}",&digest[..24]),entity_type,entity_id,source.title,source.url,source.checked_at,source.evidence_type],
        )?;
    }
    Ok(())
}

fn opportunity_availability(value: &FoundOpportunity) -> &str {
    if value.deadline.as_deref().and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .is_some_and(|date| date < Utc::now().date_naive()) { return "closed" }
    if value.availability.as_deref() == Some("closed") { return "closed" }
    if matches!(value.opportunity_type.as_deref(),Some("prospective_contact"|"prospective_pi")) { return "prospective" }
    match value.availability.as_deref() {
        Some("open") => "open",
        _ => "uncertain", // Older output must not silently imply a live vacancy.
    }
}

fn validate_opportunity(value:&FoundOpportunity)->Result<()> {
    if value.eligibility_status.as_deref().is_some_and(|status| !matches!(status,"eligible"|"uncertain"|"ineligible")) { bail!("资格状态无效") }
    if value.eligibility_status.as_deref() == Some("ineligible")
        || (value.eligibility_status.is_none() && !value.career_level_eligible) {
        bail!("职业阶段、资格或现实约束与目标机会不匹配")
    }
    if value.title.trim().is_empty() || value.organization.trim().is_empty() { bail!("职位或机构为空") }
    if !is_http_url(&value.source_url) { bail!("缺少有效的机会来源 URL") }
    if value.verified_at.trim().is_empty() { bail!("缺少核验时间") }
    chrono::DateTime::parse_from_rfc3339(&value.verified_at).context("机会核验时间不是 ISO-8601")?;
    if let Some(kind)=value.opportunity_type.as_deref() {
        if !matches!(kind,"formal_position"|"fellowship"|"program"|"prospective_contact"|"other"|"formal_postdoc"|"prospective_pi") { bail!("机会类型无效：{kind}") }
    }
    if !value.sources.iter().any(|source| source.evidence_type == "primary") { bail!("缺少官方主来源证据；学术搜索可发现线索，但不能单独证明招聘状态") }
    if value.availability.as_deref().is_some_and(|status| !matches!(status,"open"|"closed"|"prospective"|"uncertain")) { bail!("招聘状态无效") }
    for source in &value.sources { validate_source(source)?; }
    Ok(())
}

fn validate_internship_opportunity(value: &FoundInternshipOpportunity) -> Result<()> {
    if value.opportunity_kind != "industry_internship" {
        bail!("机会类型不是 industry_internship")
    }
    if !value.active {
        bail!("官方来源未确认职位仍在开放")
    }
    if value.title.trim().is_empty() || value.organization.trim().is_empty() {
        bail!("职位或公司为空")
    }
    if !is_http_url(&value.source_url) {
        bail!("缺少有效的官方职位 URL")
    }
    if value.verified_at.trim().is_empty() {
        bail!("缺少核验时间")
    }
    chrono::DateTime::parse_from_rfc3339(&value.verified_at)
        .context("职位核验时间不是 ISO-8601")?;
    if !matches!(
        value.eligibility_status.as_str(),
        "eligible" | "uncertain" | "ineligible"
    ) {
        bail!("eligibilityStatus 无效：{}", value.eligibility_status)
    }
    if value.eligibility_status == "ineligible" {
        bail!("候选人已明确不符合硬性资格")
    }
    if !(0.0..=100.0).contains(&value.fit_score) {
        bail!("匹配分必须在 0 到 100 之间")
    }
    if value.summary.trim().len() < 20 || value.eligibility_summary.trim().len() < 12 {
        bail!("职位或资格说明不完整")
    }
    if value.fit_analysis.trim().len() < 40 || value.fit_analysis_zh.trim().len() < 20 {
        bail!("双语匹配分析不完整")
    }
    normalize_checklist_items(&value.checklist)?;
    if value.sources.is_empty() {
        bail!("缺少官方来源证据")
    }
    if !value.sources.iter().any(|source| source.evidence_type == "primary") {
        bail!("缺少官方主来源证据")
    }
    for source in &value.sources {
        validate_source(source)?;
    }
    Ok(())
}

fn validate_contact_identity(value: &FoundContact) -> Result<()> {
    if value.name.trim().is_empty() { bail!("联系人姓名或官方申请入口名称为空") }
    if !value.fit_score.is_finite() || !(0.0..=100.0).contains(&value.fit_score) { bail!("匹配分数必须在 0–100 之间") }
    if let Some(email) = value.email.as_deref() {
        if !valid_email(email) { bail!("联系人邮箱无效；未核验时应留空") }
    }
    Ok(())
}

fn validate_structured_fit(value: &FoundContact) -> Result<()> {
    let dimensions = [("research",30.0),("methods",25.0),("evidence",15.0),("requirements",15.0),("logistics",10.0),("hiring",5.0)];
    if value.materials.fit_scores.len() != dimensions.len() { bail!("fitScores 需要六个评分维度") }
    let mut total = 0.0;
    for (id, maximum) in dimensions {
        let rows = value.materials.fit_scores.iter().filter(|row| row.dimension == id).collect::<Vec<_>>();
        if rows.len() != 1 { bail!("fitScores 维度 {id} 缺失或重复") }
        let row = rows[0];
        if !row.score.is_finite() || !(0.0..=maximum).contains(&row.score) { bail!("fitScores/{id}/score 超出范围") }
        if row.evidence.trim().is_empty() || row.uncertainty.trim().is_empty() { bail!("fitScores/{id} 需要 evidence 和 uncertainty；未知可以明确写未知") }
        total += row.score;
    }
    if (total - value.fit_score).abs() > 0.01 { bail!("fitScores 合计与 fitScore 不一致") }
    Ok(())
}

fn validate_contact_for_language(value: &FoundContact, paths: &AppPaths) -> Result<()> {
    validate_contact_identity(value)?;
    if value.materials.fit_scores.is_empty() {
        // Backward-compatible reader for old jobs; new jobs use structured scores.
        return validate_contact(value)
    }
    validate_structured_fit(value)?;
    crate::cv_schema::normalize(&value.materials.cv_data)?;
    normalize_checklist_items(&value.materials.checklist)?;
    let language = crate::onboarding::load(paths)?.preferred_language;
    for (lang, email, fit, profile) in [
        ("en",&value.materials.email_en,&value.materials.fit_analysis,&value.materials.pi_profile),
        ("zh",&value.materials.email_zh,&value.materials.fit_analysis_zh,&value.materials.pi_profile_zh),
    ] {
        if language != "bilingual" && language != lang { continue; }
        for (label,text) in [("联系信",email),("匹配分析",fit),("联系人简报",profile)] {
            if text.trim().is_empty() { bail!("{lang}/{label} 未生成") }
        }
        // Evidence quality is explicit in fitScores; headings/word counts are presentation, not gates.
        for text in [fit,profile] {
            if !text.contains("https://") && !text.contains("http://") { bail!("{lang} 分析或简报缺少来源链接") }
        }
    }
    Ok(())
}

fn validate_contact(value:&FoundContact)->Result<()> {
    if value.name.trim().is_empty() { bail!("联系人姓名为空") }
    if let Some(email)=value.email.as_deref(){if !valid_email(email){bail!("联系人邮箱无效")}}
    crate::cv_schema::normalize(&value.materials.cv_data).context("缺少有效的结构化 CV")?;
    normalize_checklist_items(&value.materials.checklist)?;
    for (label,text) in [
        ("英文联系信",&value.materials.email_en),("中文联系信",&value.materials.email_zh),
    ] { if text.trim().len()<40 { bail!("{label}不完整") } }
    validate_fit_report(
        "英文匹配分析",
        &value.materials.fit_analysis,
        &[
            ("Research/topic alignment", 30.0),
            ("Methods/skills", 25.0),
            ("Candidate evidence", 15.0),
            ("Role requirements", 15.0),
            ("Logistics", 10.0),
            ("Hiring signal", 5.0),
        ],
        &["## Recommendation", "## Gaps and risks", "## Positioning", "## Sources"],
        value.fit_score,
        800,
    )?;
    validate_fit_report(
        "中文匹配分析",
        &value.materials.fit_analysis_zh,
        &[
            ("研究主题匹配", 30.0),
            ("方法与技能", 25.0),
            ("候选人证据", 15.0),
            ("岗位要求", 15.0),
            ("现实条件", 10.0),
            ("招聘信号", 5.0),
        ],
        &["## 建议", "## 缺口与风险", "## 申请定位", "## 来源"],
        value.fit_score,
        450,
    )?;
    validate_report_sections(
        "英文联系人简报",
        &value.materials.pi_profile,
        &[
            "## Identity and current role",
            "## Current research",
            "## Recent work",
            "## Active projects and opportunity context",
            "## Lab and collaboration context",
            "## Contact evidence",
            "## Uncertainties",
            "## Sources",
        ],
        800,
    )?;
    validate_report_sections(
        "中文联系人简报",
        &value.materials.pi_profile_zh,
        &[
            "## 身份与现职",
            "## 当前研究方向",
            "## 近期代表工作",
            "## 在研项目与机会背景",
            "## 团队与合作背景",
            "## 联系方式证据",
            "## 待核实事项",
            "## 来源",
        ],
        450,
    )?;
    Ok(())
}

fn validate_fit_report(
    label: &str,
    text: &str,
    dimensions: &[(&str, f64)],
    sections: &[&str],
    expected_score: f64,
    minimum_characters: usize,
) -> Result<()> {
    validate_report_sections(label, text, sections, minimum_characters)?;
    let mut total = 0.0;
    for (dimension, expected_maximum) in dimensions {
        let row_prefix = format!("| {dimension} |");
        let row = text.lines().find(|line| line.trim_start().starts_with(&row_prefix))
            .with_context(|| format!("{label}缺少评分行：{dimension}"))?;
        let cells = row.trim().trim_matches('|').split('|').map(str::trim).collect::<Vec<_>>();
        let score = cells.get(1).context(format!("{label}的 {dimension} 评分列无效"))?;
        let (earned, maximum) = score.split_once('/')
            .with_context(|| format!("{label}的 {dimension} 必须使用 得分/满分"))?;
        let earned = earned.trim().parse::<f64>()
            .with_context(|| format!("{label}的 {dimension} 得分不是数字"))?;
        let maximum = maximum.trim().parse::<f64>()
            .with_context(|| format!("{label}的 {dimension} 满分不是数字"))?;
        if (maximum - expected_maximum).abs() > 0.01 || !(0.0..=maximum).contains(&earned) {
            bail!("{label}的 {dimension} 评分超出 0/{expected_maximum} 范围")
        }
        let (evidence, gap) = match (cells.get(2).copied(), cells.get(3).copied()) {
            (Some(evidence), Some(gap)) => (evidence, gap),
            (Some(combined), None) => split_combined_fit_evidence_gap(combined)
                .with_context(|| format!("{label}的 {dimension} 必须使用独立证据和缺口列，或在合并列中明确写出 Gap/uncertainty"))?,
            _ => bail!("{label}的 {dimension} 必须同时说明证据和缺口"),
        };
        if evidence.len() < 12 || gap.len() < 8 {
            bail!("{label}的 {dimension} 必须同时说明证据和缺口")
        }
        total += earned;
    }
    if (total - expected_score).abs() > 0.01 {
        bail!("{label}六项得分合计 {total}，与 fitScore {expected_score} 不一致")
    }
    Ok(())
}

fn split_combined_fit_evidence_gap(value: &str) -> Option<(&str, &str)> {
    [
        "Gap/uncertainty:",
        "Gap / uncertainty:",
        "gap/uncertainty:",
        "gap / uncertainty:",
        "缺口或不确定性：",
        "缺口或不确定性:",
        "缺口：",
        "缺口:",
        "不确定性：",
        "不确定性:",
    ]
    .iter()
    .find_map(|marker| {
        value
            .split_once(*marker)
            .map(|(evidence, gap)| (evidence.trim(), gap.trim()))
    })
}

fn validate_report_sections(
    label: &str,
    text: &str,
    required_sections: &[&str],
    minimum_characters: usize,
) -> Result<()> {
    if text.trim().chars().count() < minimum_characters {
        bail!("{label}过短，至少需要 {minimum_characters} 个字符")
    }
    for section in required_sections {
        if !text.contains(section) {
            bail!("{label}缺少章节：{section}")
        }
    }
    if !text.contains("https://") && !text.contains("http://") {
        bail!("{label}缺少可核验来源链接")
    }
    Ok(())
}

fn ensure_protocol_version(version:u8,file:&str)->Result<()> {
    if version != protocol_version() { bail!("{file} 使用了不受支持的 schemaVersion：{version}") }
    Ok(())
}

fn validate_source(value:&SourceEvidence)->Result<()> {
    if value.title.trim().is_empty() || !is_http_url(&value.url) || value.checked_at.trim().is_empty(){
        bail!("来源缺少标题、URL 或核验时间")
    }
    chrono::DateTime::parse_from_rfc3339(&value.checked_at).context("来源核验时间不是 ISO-8601")?;
    if !matches!(value.evidence_type.as_str(),"primary"|"secondary"|"inferred") {
        bail!("来源证据类型无效：{}",value.evidence_type)
    }
    Ok(())
}

fn opportunity_identity(value:&FoundOpportunity)->String {
    identity::key(&incoming_identity(value))
}

fn internship_opportunity_identity(value: &FoundInternshipOpportunity) -> String {
    if let Some(id) = value.external_id.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
        return format!(
            "internship-external:{}:{}",
            normalize_text(&value.organization),
            normalize_text(id)
        )
    }
    if is_http_url(&value.source_url) {
        return format!("internship-url:{}", internship_canonical_url(&value.source_url))
    }
    format!(
        "internship-fingerprint:{}:{}:{}",
        normalize_text(&value.organization),
        normalize_text(&value.title),
        value.deadline.as_deref().unwrap_or("unknown")
    )
}

fn canonical_url(value:&str)->String{
    if let Ok(mut url)=Url::parse(value){
        url.set_fragment(None);
        let tracking=["utm_source","utm_medium","utm_campaign","utm_term","utm_content","fbclid","gclid"];
        let pairs=url.query_pairs().filter(|(key,_)|!tracking.contains(&key.as_ref())).map(|(k,v)|(k.into_owned(),v.into_owned())).collect::<Vec<_>>();
        url.set_query(None);
        if !pairs.is_empty(){url.query_pairs_mut().extend_pairs(pairs);}
        return url.to_string()
    }
    value.trim().to_owned()
}

// Internship identity compatibility is outside this Postdoc hardening slice.
fn internship_canonical_url(value: &str) -> String { canonical_url(value).trim_end_matches('/').to_lowercase() }

fn normalize_email(value:&str)->String{value.trim().to_lowercase()}
fn normalize_person(value:&str)->String{value.to_lowercase().chars().filter(|value|value.is_alphanumeric()).collect()}
fn normalize_text(value:&str)->String{value.to_lowercase().chars().map(|c|if c.is_alphanumeric(){c}else{' '}).collect::<String>().split_whitespace().collect::<Vec<_>>().join("-")}
fn valid_email(value:&str)->bool{let value=value.trim();let Some((left,right))=value.split_once('@')else{return false};!left.is_empty()&&right.contains('.')&&!value.chars().any(char::is_whitespace)}
fn is_http_url(value:&str)->bool{Url::parse(value).map(|url|matches!(url.scheme(),"http"|"https")).unwrap_or(false)}

fn target_directory(paths:&AppPaths,target_id:&str)->PathBuf{
    let safe=target_id.chars().map(|c|if c.is_ascii_alphanumeric()||c=='-'{c}else{'_'}).collect::<String>();
    paths.generated.join("contact-targets").join(safe)
}
fn display_path(root:&Path,path:&Path)->String{path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string()}
fn read_json<T:for<'de>Deserialize<'de>>(path:&Path)->Result<T>{serde_json::from_slice(&fs::read(path).with_context(||format!("Agent 没有生成 {}",path.display()))?).with_context(||format!("{} 不是有效的结构化结果",path.display()))}

#[cfg(test)]
mod tests{
    use super::*;
    use tempfile::TempDir;

    fn complete_fit_en() -> String { r#"# Fit analysis — 90/100

| Dimension | Score | Evidence | Gap / uncertainty |
|---|---:|---|---|
| Research/topic alignment | 27/30 | The verified role and candidate profile share the same research problem, application domain, and intended scientific outcome. | One adjacent topic appears only in the role and must be framed as a proposed extension. |
| Methods/skills | 23/25 | Verified modelling, inference, numerical analysis, and machine-learning methods directly support the advertised technical work. | One specialist software package named by the role is not present in the candidate record. |
| Candidate evidence | 14/15 | Publications, research projects, and field validation provide concrete evidence for the strongest parts of the claimed fit. | The most recent manuscript remains under review and must be labelled accurately. |
| Role requirements | 13/15 | The verified degree stage, research record, and transferable skills satisfy the principal published requirements. | Final degree timing and one administrative document still require confirmation. |
| Logistics | 9/10 | The location, stated start window, deadline, and application route are compatible with the recorded preferences. | The exact appointment start date is not stated on the primary page. |
| Hiring signal | 4/5 | An active primary-source vacancy and named responsible contact provide a strong, current hiring signal. | Funding duration beyond the advertised term is not independently verified. |

## Recommendation

Proceed with a carefully tailored application and lead with the strongest verified research overlap. Keep all unverified specialist experience out of the CV and address the remaining administrative questions before submission.

## Gaps and risks

The main risks are the unverified specialist package, final degree timing, and uncertainty around the exact start date. None should be converted into a positive claim without additional evidence.

## Positioning

Position the candidate as bringing proven physics-informed modelling, evidence-backed inference, and field-tested research practice into the target programme, while treating the adjacent topic as future work.

## Sources

- [Official role](https://example.edu/jobs/1)
- [Official contact profile](https://example.edu/people/pi)
"#.into() }

    fn complete_fit_en_with_combined_evidence_and_gap() -> String {
        complete_fit_en()
            .lines()
            .map(|line| {
                if line == "| Dimension | Score | Evidence | Gap / uncertainty |" {
                    return "| Dimension | Score / maximum | Evidence and gap/uncertainty |".into();
                }
                if line.starts_with("| ") && !line.starts_with("|---") {
                    let cells = line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect::<Vec<_>>();
                    if cells.len() == 4 {
                        return format!(
                            "| {} | {} | {} Gap/uncertainty: {} |",
                            cells[0], cells[1], cells[2], cells[3]
                        );
                    }
                }
                line.into()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn complete_fit_zh() -> String { r#"# 匹配度分析 — 90/100

| 维度 | 得分 | 支持证据 | 缺口或不确定性 |
|---|---:|---|---|
| 研究主题匹配 | 27/30 | 已核验岗位方向与候选人的核心研究问题、应用场景和预期科学目标高度重合。 | 岗位中的一个相邻主题未出现在候选人材料中，只能作为拟开展方向。 |
| 方法与技能 | 23/25 | 已核验的建模、推断、数值分析与机器学习方法能够直接支持岗位的主要技术任务。 | 岗位点名的一项专用软件尚无候选人使用证据。 |
| 候选人证据 | 14/15 | 论文、科研项目和现场验证经历为主要匹配点提供了具体证据。 | 最新成果仍在审稿，必须准确标注状态。 |
| 岗位要求 | 13/15 | 学位阶段、研究记录和可迁移技能满足官方页面列出的主要要求。 | 最终学位时间和一项行政材料仍需确认。 |
| 现实条件 | 9/10 | 地点、开始时间窗口、截止日期和申请路径与已记录偏好兼容。 | 官方页面没有给出精确入职日期。 |
| 招聘信号 | 4/5 | 当前有效的官方招聘页和明确联系人构成较强招聘信号。 | 公告期限以外的后续经费尚未独立核实。 |

## 建议

建议申请，并围绕最强的已核验研究重合点定制材料。不要把尚未核实的专用软件经验写成已有能力，提交前确认学位与行政要求。

## 缺口与风险

主要风险是专用软件经验缺失、最终学位时间与精确入职日期不确定。这些事项在获得新证据前都应明确写为待核实。

## 申请定位

将候选人定位为能够把已有的物理信息建模、证据化推断和现场研究经验迁移到目标课题的人选，同时把相邻主题写成未来研究计划。

## 来源

- [官方岗位](https://example.edu/jobs/1)
- [官方联系人主页](https://example.edu/people/pi)
"#.into() }

    fn complete_pi_en() -> String { r#"# Contact brief — Example PI

## Identity and current role

The official institutional directory identifies Example PI as the current research lead for the advertised programme and confirms the present department and title.

## Current research

The current official profile emphasizes computational modelling, evidence-aware machine learning, inverse problems, and validation in realistic scientific settings relevant to this opportunity.

## Recent work

- **2026 — Verified programme paper.** The official publication page documents the newest method and its evaluated scientific application.
- **2025 — Verified systems study.** The institutional repository records a related study combining modelling and measured data.
- **2024 — Verified review article.** The publisher page establishes the broader trajectory and terminology used by the group.

## Active projects and opportunity context

The primary vacancy page names the contact and describes an active funded project. The advertised duties overlap with the current research trajectory, but continuation beyond the stated contract is unknown.

## Lab and collaboration context

The laboratory combines methodological development with domain collaborators. A credible collaboration angle is to contribute verified modelling and inference experience without claiming unrecorded domain expertise.

## Contact evidence

The institutional directory lists `pi@example.edu`; the vacancy page identifies the same person as the scientific contact. No private or inferred address is used.

## Uncertainties

The exact team size, future funding, preferred start date, and use of one specialist software package are not established by the primary sources and require confirmation.

## Sources

- [Official profile](https://example.edu/people/pi)
- [Official vacancy](https://example.edu/jobs/1)
- [Official publications](https://example.edu/people/pi/publications)
"#.into() }

    fn complete_pi_zh() -> String { r#"# 联系人简报 — 示例联系人

## 身份与现职

学校官方目录确认该联系人现任目标项目的研究负责人，并核验了当前院系、职称以及其在招聘中的责任。

## 当前研究方向

最新官方主页重点列出计算建模、证据化机器学习、逆问题以及真实科学场景验证，这些方向与当前机会直接相关。

## 近期代表工作

- **2026 — 项目论文。** 官方论文页面记录了最新方法及其科学应用验证。
- **2025 — 系统研究。** 机构知识库收录了结合建模与实测数据的相关工作。
- **2024 — 综述论文。** 出版社页面说明了团队近年的研究轨迹与核心术语。

## 在研项目与机会背景

官方招聘页明确列出联系人和当前资助项目，岗位任务与其近期方向一致；但公告期限以后的经费延续情况尚不明确。

## 团队与合作背景

该团队同时开展方法研究和领域合作。可行合作角度是贡献已核验的建模与推断经验，同时不声称不存在于材料中的领域经历。

## 联系方式证据

机构目录公开列出 `pi@example.edu`，招聘页也把同一人列为科学联系人，没有采用推测或私人邮箱。

## 待核实事项

团队规模、后续经费、期望入职日期以及一项专用软件的使用要求尚未由主来源确认，需要联系后核实。

## 来源

- [官方主页](https://example.edu/people/pi)
- [官方招聘页](https://example.edu/jobs/1)
- [官方成果列表](https://example.edu/people/pi/publications)
"#.into() }

    #[test] fn url_dedupe_removes_tracking(){assert_eq!(canonical_url("https://Example.edu/job/?utm_source=x#top"),"https://example.edu/job/");}

    #[tokio::test]
    async fn repeated_search_reuses_org_alias_and_guarded_www_alias_without_resetting_contact() -> Result<()> {
        let temp=TempDir::new()?;
        let (paths,mut value)=pipeline_fixture(&temp)?;
        value["opportunities"][0]["organization"]=json!("Example University");
        value["opportunities"][0]["title"]=json!("Postdoctoral Fellowship in Distributed Fiber Optic Sensing and Machine Learning");
        value["opportunities"][0]["contacts"][0]["email"]=json!("research@example.edu");
        value["opportunities"][0]["sourceUrl"]=json!("https://example.edu/jobs.html");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        let conn=db::connect(&paths.database)?;
        conn.execute("UPDATE contact_targets_v2 SET status='follow_up'",[])?;
        value["opportunities"][0]["organization"]=json!("Example University, Singapore");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        value["opportunities"][0]["sourceUrl"]=json!("https://www.example.edu/jobs.html");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(conn.query_row("SELECT status FROM contact_targets_v2",[],|r|r.get::<_,String>(0))?,"follow_up");
        assert!(!www_source_alias("https://example.edu/jobs/AbC","https://www.example.edu/jobs/abc"));
        assert!(!same_organization("Example University, North Campus","Example University, South Campus"));
        assert!(!similar_posting_title("Postdoctoral Research Fellowship","Postdoctoral Research Fellowship"));
        value["opportunities"][0]["externalId"]=json!("role-1");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        value["opportunities"][0]["externalId"]=json!("role-2");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get::<_,i64>(0))?,2);
        Ok(())
    }
    #[test] fn same_role_different_people_never_share_contact_key(){assert_ne!(normalize_person("Alice A"),normalize_person("Bob B"));}

    #[tokio::test]
    async fn identity_review_preserves_existing_contact_and_legacy_ids_survive_reopen() -> Result<()> {
        let temp=TempDir::new()?;
        let (paths,mut value)=pipeline_fixture(&temp)?;
        value["opportunities"][0]["externalId"]=json!("posting-A");
        value["opportunities"][0]["contacts"][0]["email"]=json!("office@example.edu");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        {
            let conn=db::connect(&paths.database)?;
            conn.execute("UPDATE opportunities SET identity_key='external:posting-a'",[])?;
            conn.execute("UPDATE contact_targets_v2 SET status='follow_up'",[])?;
        }
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        let conn=db::connect(&paths.database)?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get::<_,i64>(0))?,1);
        value["opportunities"][0]["title"]=json!("Detailed source backed topic in signal processing and distributed sensing");
        // Same official ID on an unverified mirror must be reviewed, not automatically merged.
        value["opportunities"][0]["sourceUrl"]=json!("https://mirror.example/job");
        value["opportunities"][0]["sources"][0]["url"]=json!("https://mirror.example/job");
        value["opportunities"][0]["organization"]=json!("Example University");
        // With only a short old title there is no topic evidence, but matching cross-site ID is still suspicious.
        let result=import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        assert_eq!(result["outcome"],"identity_needs_review","{result}");
        let count:i64=conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get(0))?;
        assert_eq!(count,1);
        value["opportunities"][0]["sourceUrl"]=json!("https://third.example/job");
        value["opportunities"][0]["sources"][0]["url"]=json!("https://third.example/job");
        let reviewed=import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        assert_eq!(reviewed["outcome"],"identity_needs_review");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get::<_,i64>(0))?,count);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_targets_v2 WHERE status='follow_up'",[],|r|r.get::<_,i64>(0))?,1);
        Ok(())
    }

    #[tokio::test]
    async fn workday_search_and_continuation_share_identity_without_cross_source_id_collisions() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, mut value) = pipeline_fixture(&temp)?;
        let original_url = "https://example.wd3.myworkdayjobs.com/Careers/job/Research_R00001234";
        value["opportunities"][0]["sourceUrl"] = json!(original_url);
        value["opportunities"][0]["sources"][0]["url"] = json!(original_url);
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        let conn = db::connect(&paths.database)?;
        let original: String = conn.query_row("SELECT id FROM opportunities",[],|r|r.get(0))?;
        conn.execute("UPDATE contact_targets_v2 SET status='follow_up'",[])?;
        let translated = "https://example.wd5.myworkdayjobs.com/en-US/Careers/job/Translated-Title_R00001234";
        value["opportunities"][0]["sourceUrl"] = json!(translated);
        value["opportunities"][0]["sources"][0]["url"] = json!(translated);
        value["opportunities"][0]["organization"] = json!("Translated Institution Name");
        for payload in [json!({}), json!({"opportunityId":original})] {
            let result = import_search_output(&paths,"pipeline",&payload,serde_json::from_value(value.clone())?).await?;
            assert_ne!(result["outcome"],"identity_needs_review","{result}");
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get::<_,i64>(0))?,1);
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_targets_v2 WHERE status='follow_up'",[],|r|r.get::<_,i64>(0))?,1);
        }
        // Two independent sources may reuse the same short official ID.
        for (organization, url) in [("First Independent University","https://first.example/jobs/12"),("Second Independent University","https://second.example/jobs/12")] {
            value["opportunities"][0]["organization"] = json!(organization);
            value["opportunities"][0]["externalId"] = json!("JOB-12");
            value["opportunities"][0]["sourceUrl"] = json!(url);
            value["opportunities"][0]["sources"][0]["url"] = json!(url);
            import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        }
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get::<_,i64>(0))?,3);
        Ok(())
    }

    #[test]
    fn configured_result_limit_uses_only_the_supported_user_range() {
        assert_eq!(configured_result_limit(&json!({"maxResults": 3}), 5), 3);
        assert_eq!(configured_result_limit(&json!({"maxResults": 0}), 5), 5);
        assert_eq!(configured_result_limit(&json!({"maxResults": 6}), 5), 5);
    }

    #[test]
    fn protocol_contracts_are_versioned_and_exact() {
        let internship = result_contract("internship_search");
        assert_eq!(internship["required"]["schemaVersion"], 1);
        assert_eq!(
            internship["required"]["opportunities"][0]["eligibilityStatus"],
            "eligible|uncertain|ineligible"
        );
        assert!(internship["rules"]
            .as_array()
            .is_some_and(|rules| rules.iter().any(|rule| rule.as_str().is_some_and(|text| text.contains("Do not create a CV")))));
        let search = result_contract("research_pi");
        assert_eq!(search["required"]["schemaVersion"], 1);
        assert_eq!(search["required"]["opportunities"][0]["contacts"][0]["materials"]["cvData"]["schemaVersion"], 1);
        assert_ne!(search["required"]["opportunities"][0]["contacts"][0]["materials"]["checklist"][0]["itemType"], "item");
        assert!(search["rules"].as_array().unwrap().iter().any(|rule| {
            rule.as_str().is_some_and(|value| value.contains("enabled customization"))
        }));
        assert!(search["rules"].as_array().unwrap().iter().any(|rule| {
            rule.as_str().is_some_and(|value| {
                value.contains("fitScores") && value.contains("not a validation protocol")
            })
        }));
        assert_eq!(search["cvPolicy"]["references"]["fixedCount"], false);
        assert_eq!(
            search["cvLayoutPolicy"]["pageCount"]["default"],
            "auto"
        );
        assert!(search["cvPolicy"]["layoutQuality"].is_null());
        assert!(search["rules"].as_array().unwrap().iter().any(|rule| {
            rule.as_str().is_some_and(|value| value.contains("never locked"))
        }));
        let reply = result_contract("reply_followup");
        assert!(reply["required"]["recommendedOpportunities"].is_array());
    }

    #[test]
    fn every_versioned_output_envelope_deserializes() -> Result<()> {
        let internship: InternshipSearchOutput = serde_json::from_value(json!({
            "schemaVersion": 1,
            "opportunities": []
        }))?;
        let search:SearchOutput=serde_json::from_value(json!({"schemaVersion":1,"opportunities":[]}))?;
        let reply:ReplyOutput=serde_json::from_value(json!({
            "schemaVersion":1,"decision":"wait","summaryZh":"等待进一步消息并保留当前记录。",
            "recommendedAction":"暂不发送邮件。","draftReplyEn":"Draft reply retained for review only and never sent automatically.",
            "draftReplyZh":"仅供审核的中文回复草稿，系统绝不会自动发送。","sources":[],"recommendedOpportunities":[]
        }))?;
        let checklist:ChecklistResult=serde_json::from_value(json!({"schemaVersion":1,"items":[]}))?;
        let follow_up:FollowUpScanOutput=serde_json::from_value(json!({"schemaVersion":1,"items":[]}))?;
        let verification:VerificationOutput=serde_json::from_value(json!({"schemaVersion":1,"active":true,"summaryZh":"已核验。","checkedAt":"2026-08-31T00:00:00Z","sources":[]}))?;
        assert_eq!((internship.schema_version,search.schema_version,reply.schema_version,checklist.schema_version,follow_up.schema_version,verification.schema_version),(1,1,1,1,1,1));
        Ok(())
    }

    #[test]
    fn internship_validation_rejects_non_internship_quality_gaps() {
        let mut value = FoundInternshipOpportunity {
            opportunity_kind: "industry_internship".into(),
            external_id: Some("intern-1".into()),
            source_url: "https://company.example/jobs/intern-1".into(),
            source_title: Some("Official careers".into()),
            title: "Machine Learning Intern".into(),
            organization: "Example Company".into(),
            department: None,
            country: Some("Singapore".into()),
            region: Some("Asia".into()),
            location: Some("Singapore".into()),
            deadline: None,
            summary: "Verified machine learning internship working on production model evaluation.".into(),
            keywords: vec!["machine learning".into()],
            active: true,
            eligibility_status: "uncertain".into(),
            eligibility_summary: "Graduation-window eligibility requires candidate confirmation.".into(),
            fit_score: 82.0,
            fit_analysis: "Evidence-based fit analysis for review; no candidate facts are invented.".into(),
            fit_analysis_zh: "基于已核验职位要求的匹配分析；没有虚构候选人经历。".into(),
            verified_at: "2026-09-01T00:00:00Z".into(),
            sources: vec![SourceEvidence {
                title: "Official role".into(),
                url: "https://company.example/jobs/intern-1".into(),
                checked_at: "2026-09-01T00:00:00Z".into(),
                evidence_type: "primary".into(),
            }],
            checklist: vec![ChecklistOutput {
                item_type: "graduation_window".into(),
                required: true,
                status: "review".into(),
                origin: "verified".into(),
                evidence: Some("Official role specifies a graduation window.".into()),
                source_url: Some("https://company.example/jobs/intern-1".into()),
                note: None,
                sort_order: 10,
            }],
        };
        assert!(validate_internship_opportunity(&value).is_ok());
        value.eligibility_status = "ineligible".into();
        assert!(validate_internship_opportunity(&value).is_err());
    }

    #[test]
    fn reply_decision_routes_explicit_stop_to_shelved() -> Result<()> {
        assert_eq!(reply_decision_status("stop")?, "shelved");
        for decision in ["continue", "investigate_referral", "clarify", "wait"] {
            assert_eq!(reply_decision_status(decision)?, "follow_up");
        }
        assert!(reply_decision_status("negative-ish").is_err());
        Ok(())
    }

    #[test]
    fn detailed_fit_reports_require_weighted_rows_sections_and_matching_total() -> Result<()> {
        let dimensions = [
            ("Research/topic alignment", 30.0), ("Methods/skills", 25.0),
            ("Candidate evidence", 15.0), ("Role requirements", 15.0),
            ("Logistics", 10.0), ("Hiring signal", 5.0),
        ];
        let sections = ["## Recommendation", "## Gaps and risks", "## Positioning", "## Sources"];
        let valid = complete_fit_en();
        validate_fit_report("English fit", &valid, &dimensions, &sections, 90.0, 800)?;
        let combined = complete_fit_en_with_combined_evidence_and_gap();
        validate_fit_report("English fit", &combined, &dimensions, &sections, 90.0, 800)?;
        assert!(validate_fit_report(
            "English fit",
            &combined.replace("Gap/uncertainty:", ""),
            &dimensions,
            &sections,
            90.0,
            800,
        ).is_err());
        assert!(validate_fit_report("English fit", &valid, &dimensions, &sections, 89.0, 800).is_err());
        assert!(validate_fit_report("English fit", &valid.replace("## Sources", "## Evidence"), &dimensions, &sections, 90.0, 800).is_err());
        Ok(())
    }

    #[test]
    fn duplicate_generic_checklist_keys_are_normalized_without_data_loss() -> Result<()> {
        let items = vec![
            ChecklistOutput { item_type:"item".into(),required:true,status:"ready".into(),origin:"verified".into(),evidence:Some("one".into()),source_url:Some("https://example.edu/one".into()),note:None,sort_order:10 },
            ChecklistOutput { item_type:"item".into(),required:true,status:"review".into(),origin:"inferred".into(),evidence:Some("two".into()),source_url:Some("https://example.edu/two".into()),note:None,sort_order:20 },
        ];
        let normalized = normalize_checklist_items(&items)?;
        assert_eq!(normalized.len(), 2);
        assert_ne!(normalized[0].item_type, normalized[1].item_type);
        assert!(normalized.iter().all(|item| item.item_type != "item"));
        Ok(())
    }

    #[test]
    fn inferred_checklist_allows_a_blank_optional_source_url() -> Result<()> {
        let items = vec![ChecklistOutput {
            item_type:"career_level_gate".into(), required:true, status:"review".into(),
            origin:"inferred".into(), evidence:Some("Ph.D. completion date inferred from the profile.".into()),
            source_url:Some("  ".into()), note:None, sort_order:10,
        }];
        let normalized = normalize_checklist_items(&items)?;
        assert_eq!(normalized[0].source_url, None);
        Ok(())
    }

    #[test]
    fn one_opportunity_keeps_two_pi_statuses_independent() -> Result<()> {
        let temp=TempDir::new()?;
        let root=temp.path().to_path_buf();
        let paths=AppPaths{
            database:root.join("database/careeros.sqlite3"),generated:root.join("generated"),
            profile:root.join("profile"),workspaces:root.join("workspaces"),codex_home:root.join("codex"),
            backups:root.join("backups"),cache:root.join("cache"),logs:root.join("logs"),
            runtime:PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime"),data_root:root,
        };
        paths.ensure()?;
        let conn=db::connect(&paths.database)?;
        conn.execute_batch(include_str!("../migrations/0001_legacy_foundation.sql"))?;
        conn.execute_batch(include_str!("../migrations/0008_native_desktop.sql"))?;
        conn.execute_batch(include_str!("../migrations/0012_discovery_material_state.sql"))?;
        conn.execute_batch(include_str!("../migrations/0009_reply_routing_and_submission_status.sql"))?;
        conn.execute_batch(include_str!("../migrations/0014_opportunity_shelving.sql"))?;
        conn.execute("INSERT INTO native_jobs(id,job_type,status,provider_id,payload_json) VALUES('job-test','full_search','running','openai','{}')",[])?;
        drop(conn);
        let cv=json!({"schemaVersion":1,"name":"Alex Morgan","authorName":"Morgan, A.","tagline":"Targeted profile","contact":"candidate@example.org","affiliations":"Example Institute","sections":[{"title":"Research Profile","entries":[{"key":"Focus","body":"Verified target-relevant research."}]}]});
        let materials=MaterialPackage{
            fit_scores: vec![],
            cv_data:cv,email_en:"Complete English reviewable draft with verified facts and no invented claims for this contact.".into(),
            email_zh:"完整中文审核稿，仅使用已确认事实，不会自动发送，也不会提交任何申请。".into(),
            fit_analysis:complete_fit_en(),
            fit_analysis_zh:complete_fit_zh(),
            pi_profile:complete_pi_en(),
            pi_profile_zh:complete_pi_zh(),
            checklist:vec![ChecklistOutput{
                item_type:"cv".into(),required:true,status:"ready".into(),origin:"verified".into(),
                evidence:Some("Verified tailored CV is present.".into()),source_url:Some("https://example.edu/jobs/1".into()),
                note:None,sort_order:10,
            }],
        };
        let opportunity=FoundOpportunity{
            external_id:Some("same-role".into()),source_url:"https://example.edu/jobs/1".into(),source_title:Some("Official".into()),
            title:"Postdoctoral Fellow".into(),organization:"Example University".into(),department:None,country:Some("UK".into()),
            region:Some("Europe".into()),opportunity_type:Some("formal_postdoc".into()),deadline:None,summary:"Verified role".into(),
            keywords:vec![],career_level_eligible:true,availability:Some("open".into()),eligibility_status:None,verified_at:"2026-08-31T00:00:00Z".into(),
            sources:vec![SourceEvidence{title:"Official".into(),url:"https://example.edu/jobs/1".into(),checked_at:"2026-08-31T00:00:00Z".into(),evidence_type:"primary".into()}],contacts:vec![],
        };
        let mut invalid_materials = materials.clone();
        invalid_materials.cv_data = json!({"schemaVersion":1,"name":"Alex Morgan"});
        let invalid = FoundContact{name:"Invalid PI".into(),email:Some("invalid@example.edu".into()),fit_score:90.0,priority:1,homepage_url:None,lab_url:None,research_summary:None,materials:invalid_materials};
        assert!(upsert_complete_contact(&paths,"job-test",&opportunity,&invalid).is_err());
        let conn=db::connect(&paths.database)?;
        let partial_count:i64=conn.query_row("SELECT COUNT(*) FROM contact_targets_v2",[],|row|row.get(0))?;
        assert_eq!(partial_count,0,"CV 校验失败前不应写入半成品联系人");
        drop(conn);
        let a=FoundContact{name:"Alice PI".into(),email:Some("alice@example.edu".into()),fit_score:90.0,priority:1,homepage_url:None,lab_url:None,research_summary:None,materials:materials.clone()};
        let b=FoundContact{name:"Bob PI".into(),email:Some("bob@example.edu".into()),fit_score:90.0,priority:2,homepage_url:None,lab_url:None,research_summary:None,materials};
        let first=upsert_complete_contact(&paths,"job-test",&opportunity,&a)?;
        let second=upsert_complete_contact(&paths,"job-test",&opportunity,&b)?;
        let mut alias=a.clone();
        alias.email=Some("alice.new@example.edu".into());
        let alias_result=upsert_complete_contact(&paths,"job-test",&opportunity,&alias)?;
        assert_eq!(first.opportunity_id,second.opportunity_id);
        assert_ne!(first.target_id,second.target_id);
        assert_eq!(first.target_id,alias_result.target_id);
        let conn=db::connect(&paths.database)?;
        let opportunity_count:i64=conn.query_row("SELECT COUNT(*) FROM opportunities",[],|row|row.get(0))?;
        let target_count:i64=conn.query_row("SELECT COUNT(*) FROM contact_targets_v2",[],|row|row.get(0))?;
        assert_eq!((opportunity_count,target_count),(1,2));
        db::update_target_status(&paths.database,&first.target_id,"contacted")?;
        let statuses:(String,String)=conn.query_row(
            "SELECT a.status,b.status FROM contact_targets_v2 a,contact_targets_v2 b WHERE a.id=?1 AND b.id=?2",
            params![first.target_id,second.target_id],|row|Ok((row.get(0)?,row.get(1)?)),
        )?;
        assert_eq!(statuses,("contacted".into(),"ready_to_contact".into()));
        Ok(())
    }
    fn pipeline_fixture(temp: &TempDir) -> Result<(AppPaths, Value)> {
        let root = temp.path().to_path_buf();
        let paths = AppPaths {
            database:root.join("database/careeros.sqlite3"),generated:root.join("generated"),
            profile:root.join("profile"),workspaces:root.join("workspaces"),codex_home:root.join("codex"),
            backups:root.join("backups"),cache:root.join("cache"),logs:root.join("logs"),
            runtime:PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime"),data_root:root,
        };
        crate::migration::initialize(&paths)?;
        db::connect(&paths.database)?.execute("INSERT INTO native_jobs(id,job_type,status,provider_id,payload_json) VALUES('pipeline','full_search','running','openai','{}')", [])?;
        let value = json!({
            "schemaVersion":1,"opportunities":[{
                "sourceUrl":"https://example.edu/recruitment/research","title":"Research fellowship","organization":"Example University",
                "opportunityType":"prospective_contact","availability":"prospective","summary":"Institutional research group",
                "careerLevelEligible":true,"verifiedAt":"2026-09-05T00:00:00Z",
                "sources":[{"title":"Institutional profile","url":"https://example.edu/research","checkedAt":"2026-09-05T00:00:00Z","evidenceType":"primary"}],
                "contacts":[{"name":"Research Office","fitScore":90,"researchSummary":"Verified research programme"}]
            }]
        });
        Ok((paths,value))
    }

    #[tokio::test]
    async fn shelving_filters_pending_and_preserves_orphans_contacts_and_search_decisions() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths,value) = pipeline_fixture(&temp)?;
        let output: SearchOutput = serde_json::from_value(value.clone())?;
        let opportunity = &output.opportunities[0];
        let first = upsert_discovered_contact(&paths,"pipeline",opportunity,&opportunity.contacts[0],None,None)?;
        let mut second = opportunity.contacts[0].clone();
        second.name = "Second Researcher".into();
        let second = upsert_discovered_contact(&paths,"pipeline",opportunity,&second,None,None)?;
        let conn = db::connect(&paths.database)?;
        db::update_target_status(&paths.database, &first.target_id, "shelved")?;
        let pending = db::list_opportunities_by_view(&paths.database,None,0,10,true,None,false)?;
        assert_eq!(pending.pending_total,1, "Other unshelved pending contact keeps the opportunity actionable");
        db::update_target_status(&paths.database, &second.target_id, "shelved")?;
        assert_eq!(db::list_opportunities_by_view(&paths.database,None,0,10,true,None,false)?.total,0);
        assert_eq!(db::list_opportunities_by_view(&paths.database,None,0,1,false,None,true)?.shelved_total,1);
        db::set_opportunity_shelved(&paths.database,&first.opportunity_id,false)?;
        db::update_target_status(&paths.database,&first.target_id,"replied")?;
        db::update_submission_status(&paths.database,&first.target_id,"submitted")?;
        let version:i64=conn.query_row("SELECT status_version FROM contact_targets_v2 WHERE id=?1",[&first.target_id],|r|r.get(0))?;
        db::set_opportunity_shelved(&paths.database,&first.opportunity_id,true)?;
        assert!(!db::apply_agent_status(&paths.database,&first.target_id,"follow_up",Some(version))?);
        assert!(upsert_discovered_contact(&paths,"pipeline",opportunity,&opportunity.contacts[0],None,None).err().context("Shelved opportunity must be blocked")?.is::<ShelvedOpportunityResult>());
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        assert!(db::opportunity_shelved(&conn,&first.opportunity_id)?);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_targets_v2",[],|r|r.get::<_,i64>(0))?,2);
        db::set_opportunity_shelved(&paths.database,&first.opportunity_id,false)?;
        let state:(String,String,bool)=conn.query_row("SELECT status,submission_status,shelved_at IS NULL FROM contact_targets_v2 WHERE id=?1",[&first.target_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!(state,("replied".into(),"submitted".into(),true));
        db::set_opportunity_shelved(&paths.database,&first.opportunity_id,true)?;
        db::update_target_status(&paths.database,&first.target_id,"replied")?;
        assert!(!db::opportunity_shelved(&conn,&first.opportunity_id)?);
        assert_eq!(db::list_opportunities_by_view(&paths.database,None,0,10,true,None,false)?.total,1);
        conn.execute("INSERT INTO opportunities(id,title,organization,opportunity_type) VALUES('orphan','Orphan role','Example','formal_postdoc')",[])?;
        db::set_opportunity_shelved(&paths.database,"orphan",true)?;
        let shelf=db::list_opportunities_by_view(&paths.database,Some("Orphan role"),0,1,false,None,true)?;
        assert_eq!(shelf.total,1);
        assert!(shelf.items[0].shelved && shelf.items[0].contacts.is_empty());
        assert_eq!(db::list_opportunities_by_view(&paths.database,Some("Orphan role"),0,10,true,None,false)?.total,0);
        assert_eq!(db::list_opportunities_by_view(&paths.database,None,0,10,false,None,false)?.overall_total,2);
        crate::migration::initialize(&paths)?; // Reopening never resets a persisted decision.
        assert!(db::opportunity_shelved(&conn,"orphan")?);
        db::set_opportunity_shelved(&paths.database,"orphan",false)?;
        assert_eq!(db::list_opportunities_by_view(&paths.database,Some("Orphan role"),0,10,true,None,false)?.total,1);
        assert!(db::set_opportunity_shelved(&paths.database,"missing",true).is_err());
        Ok(())
    }

    fn add_fixture_materials(value: &mut Value) {
        value["opportunities"][0]["contacts"][0]["materials"] = json!({
            "cvData":{"schemaVersion":1,"name":"Alex Example","tagline":"Research and teaching",
                "contact":"alex@example.org","sections":[
                    {"title":"Education","entries":[{"key":"Doctoral study","body":"Historical research using documented archival methods and comparative source analysis."}]},
                    {"title":"Teaching","entries":[{"key":"Seminars","body":"Designed source-reading seminars and assessment exercises for undergraduate history students."}]}
                ]},
            "fitScores":[
                {"dimension":"research","score":27,"evidence":"Research theme on institutional page","uncertainty":"Funding unknown"},
                {"dimension":"methods","score":23,"evidence":"Archival methods in source CV","uncertainty":"One collection unfamiliar"},
                {"dimension":"evidence","score":14,"evidence":"Source-backed research","uncertainty":"Publication timing unknown"},
                {"dimension":"requirements","score":13,"evidence":"Doctoral background supplied","uncertainty":"Start date unknown"},
                {"dimension":"logistics","score":9,"evidence":"Requested geography","uncertainty":"Visa unknown"},
                {"dimension":"hiring","score":4,"evidence":"Prospective contact only","uncertainty":"No confirmed vacancy"}
            ],
            "emailEn":"Dear Research Office, I would like to discuss research opportunities in your programme.",
            "fitAnalysis":"Relevant historical methods, with funding unconfirmed. Source: https://example.edu/research",
            "piProfile":"Institutional programme and responsible office. No named supervisor is asserted. https://example.edu/research",
            "checklist":[{"itemType":"funding","required":true,"status":"review","origin":"inferred","note":"Confirm funding availability"}]
        });
    }

    #[tokio::test]
    async fn saved_deadline_blocks_materials_and_case_sensitive_postings_stay_separate() -> Result<()> {
        let temp=TempDir::new()?;
        let (paths,mut value)=pipeline_fixture(&temp)?;
        value["opportunities"][0]["opportunityType"]=json!("formal_postdoc");
        value["opportunities"][0]["availability"]=json!("open");
        value["opportunities"][0]["deadline"]=json!("2000-01-01");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        value["opportunities"][0]["deadline"]=Value::Null;
        value["opportunities"][0]["verifiedAt"]=json!("2026-09-06T00:00:00Z");
        add_fixture_materials(&mut value);
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        let conn=db::connect(&paths.database)?;
        assert_eq!(conn.query_row("SELECT status FROM opportunities",[],|r|r.get::<_,String>(0))?,"closed");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_targets_v2",[],|r|r.get::<_,i64>(0))?,0);
        let (_,mut other)=pipeline_fixture(&TempDir::new()?)?;
        other["opportunities"][0]["sourceUrl"]=json!("https://example.edu/jobs/AbC?token=A");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(other.clone())?).await?;
        other["opportunities"][0]["sourceUrl"]=json!("https://example.edu/jobs/abc?token=a");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(other)?).await?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|r|r.get::<_,i64>(0))?,3);
        assert_ne!(canonical_url("https://example.edu/job/"),canonical_url("https://example.edu/job"));
        Ok(())
    }

    #[tokio::test]
    async fn manual_pdf_completion_and_late_failure_use_current_package_requirements() -> Result<()> {
        let temp=TempDir::new()?;
        let (paths,mut value)=pipeline_fixture(&temp)?;
        crate::onboarding::save(&paths,crate::onboarding::OnboardingProfile {preferred_language:"en".into(),..Default::default()})?;
        add_fixture_materials(&mut value);
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        let conn=db::connect(&paths.database)?;
        let (target,pdf):(String,String)=conn.query_row("SELECT target_id,path FROM contact_target_artifacts WHERE artifact_type='cv_pdf'",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        fs::remove_file(paths.data_root.join(pdf))?;
        conn.execute("UPDATE contact_targets_v2 SET material_status='pending',material_error='Missing PDF' WHERE id=?1",[&target])?;
        // Preferences for future tasks must not change the package's required languages.
        crate::onboarding::save(&paths,crate::onboarding::OnboardingProfile {preferred_language:"bilingual".into(),..Default::default()})?;
        typst::generate_cv(&paths,&target).await?;
        let tx=db::publication_transaction(&conn)?;
        assert!(reconcile_material_state(&tx,&paths,&target,Some("old task failed"))?);
        tx.commit()?;
        assert_eq!(conn.query_row("SELECT material_status,material_error FROM contact_targets_v2 WHERE id=?1",[&target],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?)))?,("ready".into(),None));
        Ok(())
    }

    #[tokio::test]
    async fn opportunity_continuation_stays_on_original_record_and_rejects_scope_drift() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, mut value) = pipeline_fixture(&temp)?;
        value["opportunities"][0]["externalId"] = json!("official-role-123");
        let mut orphan = value.clone();
        orphan["opportunities"][0]["contacts"] = json!([]);
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(orphan)?).await?;
        let conn = db::connect(&paths.database)?;
        let id: String = conn.query_row("SELECT id FROM opportunities", [], |row| row.get(0))?;
        let payload = json!({"opportunityId":id,"threshold":75,"maxResults":5});
        let mut changed_url = value.clone();
        changed_url["opportunities"][0]["sourceUrl"] = json!("https://example.edu/new-official-page");
        changed_url["opportunities"][0]["contacts"][0]["fitScore"] = json!(40);
        let result = import_search_output(&paths,"pipeline",&payload,serde_json::from_value(changed_url)?).await?;
        assert_eq!(result["outcome"], "materials_pending");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities", [], |row| row.get::<_,i64>(0))?, 1);
        assert_eq!(conn.query_row("SELECT opportunity_id FROM contact_targets_v2", [], |row| row.get::<_,String>(0))?, id);
        let mut unrelated = value.clone();
        unrelated["opportunities"][0]["sourceUrl"] = json!("https://example.edu/another-role");
        unrelated["opportunities"][0]["externalId"] = json!("official-role-999");
        let result = import_search_output(&paths,"pipeline",&payload,serde_json::from_value(unrelated)?).await?;
        assert_eq!(result["outcome"],"identity_needs_review");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities", [], |row| row.get::<_,i64>(0))?, 1);
        value["opportunities"][0]["sourceUrl"] = json!("https://example.edu/new-official-page");
        value["opportunities"][0]["title"] = json!("Updated official title");
        value["opportunities"][0]["deadline"] = json!("2099-09-01");
        value["opportunities"][0]["contacts"] = json!([]);
        import_search_output(&paths,"pipeline",&payload,serde_json::from_value(value.clone())?).await?;
        let deadline: String = conn.query_row("SELECT deadline FROM opportunities WHERE id=?1", [&id], |row|row.get(0))?;
        assert_eq!(deadline,"2099-09-01");
        value["opportunities"][0]["deadline"] = Value::Null;
        import_search_output(&paths,"pipeline",&payload,serde_json::from_value(value.clone())?).await?;
        assert_eq!(conn.query_row("SELECT deadline FROM opportunities WHERE id=?1", [&id], |row|row.get::<_,String>(0))?,deadline);
        let result = import_search_output(&paths,"pipeline",&json!({"opportunityId":"missing"}),serde_json::from_value(value)?).await?;
        assert_eq!(result["outcome"],"identity_needs_review");
        Ok(())
    }

    #[tokio::test]
    async fn reused_source_with_conflicting_official_ids_never_merges_roles() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, mut value) = pipeline_fixture(&temp)?;
        value["opportunities"][0]["externalId"] = json!("posting-A");
        value["opportunities"][0]["contacts"] = json!([]);
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        let conn = db::connect(&paths.database)?;
        let original:String = conn.query_row("SELECT id FROM opportunities",[],|row|row.get(0))?;
        let source = value["opportunities"][0]["sourceUrl"].clone();
        value["opportunities"][0]["externalId"] = json!("posting-B");
        for payload in [json!({"opportunityId":original}),json!({"opportunityId":original,"confirmedSourceUrl":source})] {
            let result = import_search_output(&paths,"pipeline",&payload,serde_json::from_value(value.clone())?).await?;
            assert_eq!(result["outcome"],"identity_needs_review");
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|row|row.get::<_,i64>(0))?,1);
        }
        // An independent search may legitimately discover the next intake at the
        // same URL, but it must receive its own opportunity record.
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM opportunities",[],|row|row.get::<_,i64>(0))?,2);
        value["opportunities"][0]["deadline"] = json!("2099-10-01");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        assert_eq!(conn.query_row("SELECT deadline FROM opportunities WHERE notes='posting-B'",[],|row|row.get::<_,String>(0))?,"2099-10-01");
        assert!(conn.query_row("SELECT deadline FROM opportunities WHERE id=?1",[&original],|row|row.get::<_,Option<String>>(0))?.is_none());
        // No official ID means the shared page is now ambiguous, not first-row wins.
        value["opportunities"][0]["externalId"] = Value::Null;
        let result = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        assert_eq!(result["outcome"],"identity_needs_review");
        assert_eq!(result["identityReviewCount"],1);
        assert!(result["warnings"][0].as_str().unwrap().contains("多个机会"));
        Ok(())
    }

    #[tokio::test]
    async fn conflicting_people_in_one_result_are_reviewed_before_any_contact_write() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, mut value) = pipeline_fixture(&temp)?;
        let first = &mut value["opportunities"][0]["contacts"][0];
        first["email"] = json!("researcher.one@example.edu");
        first["homepageUrl"] = json!("https://example.edu/people/one");
        let mut second = first.clone();
        second["email"] = json!("researcher.two@example.edu");
        second["homepageUrl"] = json!("https://example.edu/people/two");
        value["opportunities"][0]["contacts"].as_array_mut().unwrap().push(second);
        let result = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        assert_eq!(result["outcome"],"identity_needs_review");
        assert_eq!(result["identityReviewCount"],2);
        assert!(result["warnings"][0].as_str().unwrap().contains("联系人身份待确认"));
        let conn = db::connect(&paths.database)?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_targets_v2",[],|r|r.get::<_,i64>(0))?,0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM pis",[],|r|r.get::<_,i64>(0))?,0);
        Ok(())
    }

    #[test]
    fn contact_identity_conflicts_preserve_email_pi_material_and_history() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, value) = pipeline_fixture(&temp)?;
        let output: SearchOutput = serde_json::from_value(value)?;
        let opportunity = &output.opportunities[0];
        let mut first = opportunity.contacts[0].clone();
        first.email = Some("researcher.one@example.edu".into());
        first.homepage_url = Some("https://example.edu/people/one".into());
        let target = upsert_discovered_contact(&paths,"pipeline",opportunity,&first,None,None)?;
        let conn = db::connect(&paths.database)?;
        db::update_target_status(&paths.database,&target.target_id,"contacted")?;
        store_text_artifact(&paths,&conn,&target.target_id,&target.application_id,"email","en","email.md","Original person-specific material")?;
        let original: (String,String,String) = conn.query_row("SELECT email,pi_id,status FROM contact_targets_v2 WHERE id=?1",[&target.target_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let mut different = first.clone();
        different.email = Some("researcher.two@example.edu".into());
        different.homepage_url = Some("https://example.edu/people/two".into());
        let error = upsert_discovered_contact(&paths,"pipeline",opportunity,&different,None,None).err().context("Different people must not merge")?;
        assert!(error.is::<ContactIdentityConflict>());
        // The global PI registry uses the same check even on a different vacancy.
        assert!(upsert_pi(&conn,opportunity,&different).err().context("PI identity must also be protected")?.is::<ContactIdentityConflict>());
        let after: (String,String,String) = conn.query_row("SELECT email,pi_id,status FROM contact_targets_v2 WHERE id=?1",[&target.target_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!(after,original);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM pis",[],|r|r.get::<_,i64>(0))?,1);
        let artifacts = saved_materials(&conn,&paths,&target.target_id)?;
        assert_eq!(fs::read_to_string(&artifacts[&("email".into(),"en".into())])?,"Original person-specific material");
        Ok(())
    }

    #[tokio::test]
    async fn split_opportunity_entries_check_identity_before_any_contact_update() -> Result<()> {
        let temp=TempDir::new()?;
        let (paths,value)=pipeline_fixture(&temp)?;
        let mut output:SearchOutput=serde_json::from_value(value)?;
        let opportunity=&mut output.opportunities[0];
        opportunity.contacts[0].email=Some("original@example.edu".into());
        opportunity.contacts[0].homepage_url=Some("https://example.edu/people/original".into());
        let target=upsert_discovered_contact(&paths,"pipeline",opportunity,&opportunity.contacts[0],None,None)?;
        let conn=db::connect(&paths.database)?;
        db::update_target_status(&paths.database,&target.target_id,"contacted")?;
        store_text_artifact(&paths,&conn,&target.target_id,&target.application_id,"email","en","email.md","Keep the original person's material")?;

        // Individually the first row looks like a legitimate email correction.
        // The second entry for the same vacancy makes this batch ambiguous.
        let mut corrected=opportunity.clone();
        corrected.verified_at="2026-09-06T00:00:00Z".into();
        corrected.contacts[0].email=Some("corrected@example.edu".into());
        let mut conflicting=corrected.clone();
        conflicting.contacts[0].email=Some("different-person@example.edu".into());
        conflicting.contacts[0].homepage_url=Some("https://example.edu/people/different".into());
        let mut batch=output.clone();
        batch.opportunities=vec![corrected.clone(),conflicting.clone()];
        let result=import_search_output(&paths,"pipeline",&json!({}),batch).await?;
        assert_eq!(result["outcome"],"identity_needs_review");
        assert_eq!(result["identityReviewCount"],2);
        assert!(result["imported"].as_array().unwrap().is_empty());
        assert_eq!(conn.query_row("SELECT email,status FROM contact_targets_v2 WHERE id=?1",[&target.target_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?,("original@example.edu".into(),"contacted".into()));
        assert_eq!(conn.query_row("SELECT email FROM pis WHERE id=(SELECT pi_id FROM contact_targets_v2 WHERE id=?1)",[&target.target_id],|r|r.get::<_,String>(0))?,"original@example.edu");
        let artifacts=saved_materials(&conn,&paths,&target.target_id)?;
        assert_eq!(fs::read_to_string(&artifacts[&("email".into(),"en".into())])?,"Keep the original person's material");

        // Different institutions are not one PI scope merely because names match.
        let mut independent=conflicting.clone();
        independent.organization="Independent Institute".into();
        assert!(conflicting_discovered_contacts(&[
            (90.0,corrected.clone(),corrected.contacts[0].clone(),"role-one".into()),
            (90.0,independent.clone(),independent.contacts[0].clone(),"role-two".into()),
        ]).is_empty());

        // A newer observation of the same vacancy supersedes an older row; stale
        // evidence must not turn a valid email correction into an identity conflict.
        corrected.verified_at="2026-09-07T00:00:00Z".into();
        output.opportunities=vec![conflicting,corrected];
        let result=import_search_output(&paths,"pipeline",&json!({}),output).await?;
        assert_eq!(result["identityReviewCount"],0);
        assert_eq!(conn.query_row("SELECT email FROM contact_targets_v2 WHERE id=?1",[&target.target_id],|r|r.get::<_,String>(0))?,"corrected@example.edu");
        Ok(())
    }

    #[test]
    fn same_person_email_correction_reuses_target_and_distinct_people_stay_separate() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, value) = pipeline_fixture(&temp)?;
        let output: SearchOutput = serde_json::from_value(value)?;
        let opportunity = &output.opportunities[0];
        let mut contact = opportunity.contacts[0].clone();
        contact.email = Some("old@example.edu".into());
        contact.homepage_url = Some("https://example.edu/people/alex".into());
        let original = upsert_discovered_contact(&paths,"pipeline",opportunity,&contact,None,None)?;
        contact.email = Some("new@example.edu".into());
        let corrected = upsert_discovered_contact(&paths,"pipeline",opportunity,&contact,None,None)?;
        assert_eq!(original.target_id,corrected.target_id);
        let conn = db::connect(&paths.database)?;
        assert_eq!(conn.query_row("SELECT email FROM pis",[],|r|r.get::<_,String>(0))?,"new@example.edu");
        let mut independent = contact.clone();
        independent.name = "Independent Researcher".into();
        independent.email = Some("other@example.edu".into());
        independent.homepage_url = Some("https://example.edu/people/other".into());
        let other = upsert_discovered_contact(&paths,"pipeline",opportunity,&independent,None,None)?;
        assert_ne!(other.target_id,original.target_id);
        let (first_pi,second_pi):(String,String) = conn.query_row("SELECT a.pi_id,b.pi_id FROM contact_targets_v2 a,contact_targets_v2 b WHERE a.id=?1 AND b.id=?2",params![original.target_id,other.target_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        assert_ne!(first_pi,second_pi);
        independent.email = contact.email.clone();
        assert!(matches!(compare_contact_identity(&(&contact).into(),&(&independent).into()),identity::Match::Review(_)),"A shared inbox is not proof that different names are one person");
        assert_eq!(compare_contact_identity(
            &ContactIdentity { name: "Prof. Alex Researcher", email: Some("alex@example.edu"), homepage: None },
            &ContactIdentity { name: "Alex Researcher", email: Some("alex@example.edu"), homepage: None },
        ),identity::Match::Same,"Honorifics must not make a verified email look like a shared inbox");
        Ok(())
    }

    #[test]
    fn fresh_opportunity_metadata_updates_category_without_rewriting_contact_history() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, value) = pipeline_fixture(&temp)?;
        let mut output: SearchOutput = serde_json::from_value(value)?;
        let opportunity = &mut output.opportunities[0];
        opportunity.external_id = Some("stable-role".into());
        let first = upsert_discovered_contact(&paths,"pipeline",opportunity,&opportunity.contacts[0],None,None)?;
        let mut independent = opportunity.contacts[0].clone();
        independent.name = "Independent Researcher".into();
        let second = upsert_discovered_contact(&paths,"pipeline",opportunity,&independent,None,None)?;
        let conn = db::connect(&paths.database)?;
        conn.execute("UPDATE contact_targets_v2 SET title='Contact-specific title' WHERE id=?1",[&second.target_id])?;
        db::update_target_status(&paths.database,&first.target_id,"contacted")?;
        db::set_opportunity_shelved(&paths.database,&first.opportunity_id,true)?;
        opportunity.verified_at = "2026-09-06T00:00:00Z".into();
        opportunity.title = "Funded postdoctoral fellowship".into();
        opportunity.organization = "Example University, Research Department".into();
        opportunity.department = Some("Research Department".into());
        opportunity.country = Some("Norway".into());
        opportunity.region = Some("Europe".into());
        opportunity.deadline = Some("2099-10-01".into());
        opportunity.opportunity_type = Some("formal_position".into());
        opportunity.availability = Some("open".into());
        assert_eq!(upsert_opportunity(&conn,opportunity,None,None)?,first.opportunity_id);
        let state:(String,String,String,String,String,bool) = conn.query_row("SELECT opportunity_type,status,title,country,department,shelved_at IS NOT NULL FROM opportunities WHERE id=?1",[&first.opportunity_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
        assert_eq!(state,("formal_position".into(),"open".into(),opportunity.title.clone(),"Norway".into(),"Research Department".into(),true));
        let target:(String,String,String,bool) = conn.query_row("SELECT title,organization,status,shelved_at IS NOT NULL FROM contact_targets_v2 WHERE id=?1",[&first.target_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        assert_eq!(target,(opportunity.title.clone(),opportunity.organization.clone(),"contacted".into(),true));
        assert_eq!(conn.query_row("SELECT title FROM contact_targets_v2 WHERE id=?1",[&second.target_id],|r|r.get::<_,String>(0))?,"Contact-specific title");
        assert_eq!(db::list_opportunities_by_view(&paths.database,None,0,10,false,Some("advertised"),false)?.total,1);
        let verified = opportunity.clone();
        opportunity.verified_at = "2026-09-05T00:00:00Z".into();
        opportunity.opportunity_type = Some("prospective_contact".into());
        opportunity.country = Some("Old country".into());
        upsert_opportunity(&conn,opportunity,None,None)?;
        assert_eq!(conn.query_row("SELECT opportunity_type,country FROM opportunities WHERE id=?1",[&first.opportunity_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?,("formal_position".into(),"Norway".into()));
        *opportunity = verified;
        opportunity.verified_at = "2026-09-07T00:00:00Z".into();
        opportunity.opportunity_type = Some("other".into());
        opportunity.country = Some("unknown".into());
        opportunity.department = None;
        upsert_opportunity(&conn,opportunity,None,None)?;
        assert_eq!(conn.query_row("SELECT opportunity_type,country,department FROM opportunities WHERE id=?1",[&first.opportunity_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?,("formal_position".into(),"Norway".into(),"Research Department".into()));
        opportunity.verified_at = "2026-09-08T00:00:00Z".into();
        opportunity.opportunity_type = Some("prospective_contact".into());
        opportunity.availability = Some("prospective".into());
        upsert_opportunity(&conn,opportunity,None,None)?;
        assert_eq!(db::list_opportunities_by_view(&paths.database,None,0,10,false,Some("prospective"),false)?.total,1);
        Ok(())
    }

    #[tokio::test]
    async fn stale_discovery_cannot_rewrite_contact_metadata_or_prepare_materials() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, mut value) = pipeline_fixture(&temp)?;
        value["opportunities"][0]["externalId"] = json!("stable-posting");
        value["opportunities"][0]["verifiedAt"] = json!("2026-09-04T00:00:00Z");
        value["opportunities"][0]["contacts"][0]["email"] = json!("old@example.edu");
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        let old: SearchOutput = serde_json::from_value(value.clone())?;
        value["opportunities"][0]["verifiedAt"] = json!("2026-09-05T00:00:00Z");
        value["opportunities"][0]["sourceUrl"] = json!("https://example.edu/new-official-page");
        value["opportunities"][0]["contacts"][0]["email"] = json!("new@example.edu");
        value["opportunities"][0]["contacts"][0]["fitScore"] = json!(95);
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        let contact = &old.opportunities[0].contacts[0];
        assert!(upsert_discovered_contact(&paths,"pipeline",&old.opportunities[0],contact,None,None).err().context("旧结果应被拦截")?.is::<StaleOpportunityResult>());
        let mut additional = contact.clone(); additional.name = "Another Contact".into(); additional.email = Some("another@example.edu".into());
        assert!(upsert_discovered_contact(&paths,"pipeline",&old.opportunities[0],&additional,None,None).err().context("旧结果不应添加联系人")?.is::<StaleOpportunityResult>());
        let result = import_search_output(&paths,"pipeline",&json!({}),old).await?;
        assert_eq!(result["outcome"],"stale_results_ignored");
        assert!(result["warnings"][0].as_str().unwrap().contains("跳过旧结果"));
        assert!(result["pendingMaterials"].as_array().unwrap().is_empty());
        let targets = db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?;
        assert_eq!(targets.len(),1);
        assert_eq!(targets[0].email.as_deref(),Some("new@example.edu"));
        assert_eq!(targets[0].source_url.as_deref(),Some("https://example.edu/new-official-page"));
        assert_eq!(targets[0].fit_score,Some(95.0));
        Ok(())
    }

    #[tokio::test]
    async fn partial_packages_fill_missing_files_without_replacing_manual_edits() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, mut value) = pipeline_fixture(&temp)?;
        crate::onboarding::save(&paths,crate::onboarding::OnboardingProfile { preferred_language:"en".into(),..Default::default() })?;
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        let target = db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?.remove(0);
        let conn = db::connect(&paths.database)?;
        store_text_artifact(&paths,&conn,&target.id,&target.application_id,"email","en","manual-email.md","My manually edited email must survive.")?;
        store_text_artifact(&paths,&conn,&target.id,&target.application_id,"opportunity_verification","zh","verification.md","Verified source; this alone is not a complete package.")?;
        conn.execute("INSERT INTO contact_target_checklist(id,target_id,item_type,required,status,origin,note) VALUES('manual-check',?1,'funding',1,'review','inferred','My note')", [&target.id])?;
        assert!(!package_complete(&conn,&target.id,&saved_materials(&conn,&paths,&target.id)?,"en")?);
        add_fixture_materials(&mut value);
        let result = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        assert_eq!(result["outcome"],"ready");
        let saved = saved_materials(&conn,&paths,&target.id)?;
        assert_eq!(fs::read_to_string(&saved[&("email".into(),"en".into())])?,"My manually edited email must survive.");
        assert_eq!(conn.query_row("SELECT note FROM contact_target_checklist WHERE id='manual-check'", [], |row|row.get::<_,String>(0))?,"My note");
        assert!(package_complete(&conn,&target.id,&saved,"en")?);
        let data = fs::read(&saved[&("cv_data".into(),"und".into())])?;
        fs::remove_file(&saved[&("cv_pdf".into(),"en".into())])?;
        let result = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        assert_eq!(result["outcome"],"ready");
        let saved = saved_materials(&conn,&paths,&target.id)?;
        assert_eq!(fs::read(&saved[&("cv_data".into(),"und".into())])?,data);
        assert!(package_complete(&conn,&target.id,&saved,"en")?);
        let mut another = value["opportunities"][0]["contacts"][0].clone();
        another["name"] = json!("Another real contact");
        another["fitScore"] = json!(89);
        another["materials"] = json!({});
        value["opportunities"][0]["contacts"].as_array_mut().unwrap().push(another);
        let result = import_search_output(&paths,"pipeline",&json!({"maxResults":1}),serde_json::from_value(value)?).await?;
        assert_eq!(result["pendingMaterials"].as_array().unwrap().len(),1,"ready contacts must not consume the fill-missing quota");
        Ok(())
    }

    #[tokio::test]
    async fn continuation_jobs_are_loaded_after_reopening_and_invalid_json_is_ignored() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths, mut value) = pipeline_fixture(&temp)?;
        value["opportunities"][0]["contacts"] = json!([]);
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        let conn = db::connect(&paths.database)?;
        let id: String = conn.query_row("SELECT id FROM opportunities",[],|row|row.get(0))?;
        conn.execute("UPDATE native_jobs SET status='failed',message='Review identity',payload_json=?1 WHERE id='pipeline'",[json!({"opportunityId":id}).to_string()])?;
        conn.execute("INSERT INTO native_jobs(id,job_type,status,provider_id,payload_json) VALUES('invalid','full_search','running','openai','not json')",[])?;
        drop(conn);
        let page = db::list_discovered_opportunities_by_category(&paths.database,None,0,10,true,None)?;
        assert_eq!(page.items[0].latest_job.as_ref().unwrap().id,"pipeline");
        assert_eq!(page.items[0].latest_job.as_ref().unwrap().status,"failed");
        Ok(())
    }

    #[tokio::test]
    async fn reply_result_preserves_a_newer_manual_status_and_surfaces_confirmation() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths,value) = pipeline_fixture(&temp)?;
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        let target = db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?.remove(0);
        let mut job = db::job_groups(&paths.database,5)?.running.remove(0);
        job.target_id=Some(target.id.clone());
        let conn=db::connect(&paths.database)?;
        let version:i64=conn.query_row("SELECT status_version FROM contact_targets_v2 WHERE id=?1",[&target.id],|row|row.get(0))?;
        db::update_target_status(&paths.database,&target.id,"shelved")?;
        let output: ReplyOutput=serde_json::from_value(json!({"decision":"continue","summaryZh":"建议进一步确认具体安排","recommendedAction":"Clarify timing","draftReplyEn":"Thank you for your reply. I would like to clarify the timeline.","draftReplyZh":"感谢您的回复，我希望进一步确认具体安排和时间。"}))?;
        let result=import_reply_output(&paths,&job,&json!({"_statusVersion":version}),output).await?;
        assert_eq!(result["statusApplied"],false);
        assert!(result["summary"].as_str().unwrap().contains("保留当前状态"));
        assert_eq!(db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?[0].status,"shelved");
        Ok(())
    }

    #[tokio::test]
    async fn reply_replays_keep_current_drafts_and_retain_readable_history() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths,value) = pipeline_fixture(&temp)?;
        import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        let target = db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?.remove(0);
        let conn = db::connect(&paths.database)?;
        let save_reply = |body: &str| db::save_inbound_reply(&paths.database,&crate::models::InboundReplyRequest {
            target_id:target.id.clone(),sender:None,subject:None,body:body.into(),received_at:None,
        });
        let old_reply = save_reply("Thank you for your interest. Please share your availability.")?;
        let mut old_job = db::job_groups(&paths.database,5)?.running.remove(0);
        old_job.target_id=Some(target.id.clone()); old_job.job_type="reply_followup".into();
        conn.execute("UPDATE native_jobs SET job_type='reply_followup',target_id=?1 WHERE id='pipeline'",[&target.id])?;
        let output = |label: &str| -> Result<ReplyOutput> { Ok(serde_json::from_value(json!({"decision":"continue","summaryZh":format!("{label}：建议确认具体安排"),"recommendedAction":"Clarify timing",
            "draftReplyEn":format!("{label}: Thank you for your reply. Please confirm the timeline."),"draftReplyZh":format!("{label}：感谢您的回复，请进一步确认具体安排和时间。")}))?) };
        let version = || conn.query_row("SELECT status_version FROM contact_targets_v2 WHERE id=?1",[&target.id],|row|row.get::<_,i64>(0));
        let old_payload = json!({"replyId":old_reply.id,"_statusVersion":version()?});
        let result = import_reply_output(&paths,&old_job,&old_payload,output("First")?).await?;
        assert_eq!(result["draftsApplied"],true); assert_eq!(result["statusApplied"],true);
        let artifact_path = || conn.query_row("SELECT path FROM contact_target_artifacts WHERE target_id=?1 AND artifact_type='followup_email' AND language='en'",[&target.id],|row|row.get::<_,String>(0));
        let first_path = artifact_path()?;
        // A deliberate rerun of the current task can replace its own draft, but
        // never overwrites the previous file bytes.
        assert_eq!(import_reply_output(&paths,&old_job,&old_payload,output("Revised")?).await?["draftsApplied"],true);
        assert!(fs::read_to_string(paths.data_root.join(&first_path))?.starts_with("First:"));
        let current_path = artifact_path()?;
        let new_reply = save_reply("The position is now confirmed. Can we arrange an interview?")?;
        let result = import_reply_output(&paths,&old_job,&old_payload,output("Stale before new job")?).await?;
        assert_eq!(result["draftsApplied"],false);
        assert_eq!(artifact_path()?,current_path);
        // Equal-second creation timestamps are resolved by database row order.
        conn.execute("INSERT INTO native_jobs(id,job_type,status,provider_id,target_id,created_at,payload_json) SELECT 'new-reply','reply_followup','running','openai',?1,created_at,'{}' FROM native_jobs WHERE id='pipeline'",[&target.id])?;
        let mut new_job=old_job.clone();new_job.id="new-reply".into();
        let new_payload=json!({"replyId":new_reply.id,"_statusVersion":version()?});
        assert_eq!(import_reply_output(&paths,&new_job,&new_payload,output("Latest")?).await?["draftsApplied"],true);
        let latest_path=artifact_path()?;
        let status_version=version()?;
        // Even rerunning an earlier task for the same reply cannot become latest.
        let result=import_reply_output(&paths,&old_job,&new_payload,output("Old task replay")?).await?;
        assert_eq!(result["draftsApplied"],false);assert_eq!(result["statusApplied"],false);
        assert_eq!(version()?,status_version);
        assert_eq!(artifact_path()?,latest_path);
        assert!(fs::read_to_string(paths.data_root.join(&latest_path))?.starts_with("Latest:"));
        let detail=db::target_detail(&paths.database,&paths.data_root,&target.id)?;
        assert_eq!(detail.revisions.len(),15);
        assert!(detail.revisions.iter().any(|r| r.summary.as_deref().is_some_and(|text|text.contains("仅留存供审核"))));
        for revision in detail.revisions { assert!(Path::new(&revision.artifact_path).is_file()); }
        // A failed history write must roll back the current artifact pointers too.
        conn.execute_batch("CREATE TRIGGER fail_reply_history BEFORE INSERT ON revision_change_sets BEGIN SELECT RAISE(ABORT,'fixture history failure'); END;")?;
        assert!(import_reply_output(&paths,&new_job,&new_payload,output("Must not publish")?).await.is_err());
        assert_eq!(artifact_path()?,latest_path);
        assert_eq!(version()?,status_version);
        assert_eq!(db::target_detail(&paths.database,&paths.data_root,&target.id)?.revisions.len(),15);
        Ok(())
    }

    #[tokio::test]
    async fn discovery_without_materials_survives_retry_and_empty_results_are_not_failures() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths,value) = pipeline_fixture(&temp)?;
        for _ in 0..2 {
            let result = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
            assert_eq!(result["outcome"],"materials_pending");
            assert_eq!(result["pendingMaterials"].as_array().unwrap().len(),1);
        }
        let targets = db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?;
        assert_eq!(targets.len(),1);
        assert_eq!(targets[0].material_status,"pending");
        assert_eq!(targets[0].opportunity_status.as_deref(),Some("prospective"));
        let conn = db::connect(&paths.database)?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_target_artifacts WHERE artifact_type<>\'material_requirements\'",[],|row|row.get::<_,i64>(0))?,0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_target_artifacts WHERE artifact_type=\'material_requirements\'",[],|row|row.get::<_,i64>(0))?,1);
        let result = import_search_output(&paths,"pipeline",&json!({}),SearchOutput{schema_version:1,opportunities:vec![]}).await?;
        assert_eq!(result["outcome"],"no_matches");
        assert_eq!(db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?.len(),1);
        Ok(())
    }

    #[tokio::test]
    async fn source_only_user_gets_atomic_materials_without_references_or_mandatory_english_headings() -> Result<()> {
        let temp = TempDir::new()?;
        let (paths,mut value) = pipeline_fixture(&temp)?;
        crate::onboarding::save(&paths,crate::onboarding::OnboardingProfile {
            preferred_language:"en".into(),..Default::default()
        })?;
        add_fixture_materials(&mut value);
        assert!(!paths.profile.join("master_profile.json").exists());
        let result = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value.clone())?).await?;
        assert_eq!(result["outcome"],"ready","{result}");
        let targets = db::list_targets(&paths.database,"postdoc",None,None,None,0,20)?;
        let target = &targets[0];
        assert_eq!(target.material_status,"ready");
        let conn = db::connect(&paths.database)?;
        let stored: String = conn.query_row("SELECT path FROM contact_target_artifacts WHERE target_id=?1 AND artifact_type='cv_pdf'",[&target.id],|row|row.get(0))?;
        assert!(stored.contains("/versions/"));
        let pdf = paths.data_root.join(&stored);
        let before = fs::read(&pdf)?;
        value["opportunities"][0]["contacts"][0]["materials"]["cvData"]["name"] = json!("Do not replace existing edits");
        let again = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        assert_eq!(again["imported"][0]["preservedExistingMaterials"],true);
        assert_eq!(before,fs::read(&pdf)?);
        assert!(!paths.profile.join("cv_structure.json").exists(),"new users must not get an automatic fixed baseline");
        Ok(())
    }

    #[tokio::test]
    async fn failed_render_keeps_discovery_without_publishing_partial_files() -> Result<()> {
        let temp = TempDir::new()?;
        let (mut paths,mut value) = pipeline_fixture(&temp)?;
        crate::onboarding::save(&paths,crate::onboarding::OnboardingProfile {
            preferred_language:"en".into(),..Default::default()
        })?;
        add_fixture_materials(&mut value);
        value["opportunities"][0]["contacts"][0]["materials"]["cvData"]["sections"] = json!([]);
        paths.runtime = paths.data_root.join("missing-runtime");
        let result = import_search_output(&paths,"pipeline",&json!({}),serde_json::from_value(value)?).await?;
        assert_eq!(result["outcome"],"materials_pending");
        let conn = db::connect(&paths.database)?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_targets_v2",[],|row|row.get::<_,i64>(0))?,1);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_target_artifacts WHERE artifact_type<>\'material_requirements\'",[],|row|row.get::<_,i64>(0))?,0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM contact_target_artifacts WHERE artifact_type=\'material_requirements\'",[],|row|row.get::<_,i64>(0))?,1);
        Ok(())
    }

    #[test]
    fn vacancy_state_does_not_promote_unknown_or_expired_opportunities_to_open() -> Result<()> {
        let temp=TempDir::new()?;
        let (_,value)=pipeline_fixture(&temp)?;
        let mut opportunity:FoundOpportunity=serde_json::from_value(value["opportunities"][0].clone())?;
        opportunity.opportunity_type=Some("formal_position".into());
        opportunity.availability=None;
        assert_eq!(opportunity_availability(&opportunity),"uncertain");
        opportunity.availability=Some("open".into());
        opportunity.deadline=Some("2000-01-01".into());
        assert_eq!(opportunity_availability(&opportunity),"closed");
        opportunity.sources[0].evidence_type="secondary".into();
        assert!(validate_opportunity(&opportunity).is_err());
        Ok(())
    }

}
