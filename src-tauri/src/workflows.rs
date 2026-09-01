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
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;
use uuid::Uuid;

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
    materials: MaterialPackage,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaterialPackage {
    cv_data: Value,
    email_en: String,
    email_zh: String,
    fit_analysis: String,
    fit_analysis_zh: String,
    pi_profile: String,
    pi_profile_zh: String,
    #[serde(default)]
    checklist: Vec<ChecklistOutput>,
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
        "opportunityType":"formal_postdoc|fellowship|prospective_pi",
        "deadline":"ISO date or null",
        "summary":"verified summary",
        "keywords":["keyword"],
        "careerLevelEligible":true,
        "verifiedAt":"UTC ISO-8601",
        "sources":[{"title":"primary source","url":"https://...","checkedAt":"UTC ISO-8601","evidenceType":"primary"}],
        "contacts":[{
            "name":"PI or contact","email":"verified email or null","fitScore":86,
            "priority":1,"homepageUrl":"optional","labUrl":"optional","researchSummary":"verified",
            "materials":{
                "cvData": crate::cv_schema::contract(),
                "emailEn":"complete reviewable Markdown",
                "emailZh":"complete Chinese translation Markdown",
                "fitAnalysis":"evidence-based Markdown",
                "fitAnalysisZh":"concise Chinese explanation of the English analysis",
                "piProfile":"verified Markdown with source links",
                "piProfileZh":"concise Chinese explanation of the English PI profile",
                "checklist":[{"itemType":"unique stable key such as eligibility_confirmation","required":true,"status":"ready|review|missing","origin":"verified|inferred","evidence":"text","sourceUrl":"https://...","note":"optional","sortOrder":10}]
            }
        }]
    })
}

pub fn result_contract(job_type: &str) -> Value {
    match job_type {
        "full_run" | "full_search" | "research_pi" => json!({
            "file": "output/search-results.json",
            "schemaVersion": 1,
            "rules": [
                "Use exact camelCase keys shown below; do not emit snake_case alternatives.",
                "Every contact must contain a complete reviewable material package.",
                "cvData must match the exact object shape shown below.",
                "cvData is the final target-specific selection, not a full master CV and not content copied from another contact.",
                "Select CV evidence by fit: target research direction first, then the strongest relevant experience, outputs, and required skills.",
                "For an open formal vacancy or fellowship, tailor cvData to the verified duties and requirements on the primary recruitment page.",
                "For a prospective PI without an open vacancy, tailor cvData to the PI's verified current research direction and never imply that a vacancy exists.",
                "Do not repeat the same claim in multiple sections or create synonymous duplicate sections.",
                "Order research evidence sections as selected research outputs or publications, then selected patents, then selected research projects; articles and patents must always appear before projects.",
                "Use the compact bundled CV layout, bold Miao, H. in publication author lists, and describe the unfinished doctorate as Ph.D. Candidate.",
                "The rendered CV must be exactly two well-filled A4 pages; a one-page CV, a sparse second page, or a CV over two pages is invalid.",
                "Provide at least 36 distinct target-relevant entries so both pages carry substantive evidence.",
                "Fill both pages with additional target-relevant verified evidence from the master profile, never with repetition, generic padding, invented claims, or unreadably compressed text."
            ],
            "limits": {"discovery":20,"deepVerification":8,"completePackages":5},
            "required": {
                "schemaVersion": 1,
                "opportunities": [opportunity_contract()]
            }
        }),
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
            "required":{"schemaVersion":1,"items":[{"itemType":"unique key such as eligibility_confirmation or cv","required":true,"status":"ready|review|missing","origin":"verified|inferred","evidence":"text","sourceUrl":"https://...","note":"optional","sortOrder":10}]}
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
        "full_run" | "full_search" | "research_pi" => {
            let output = read_json::<SearchOutput>(&workspace.join("output/search-results.json"))?;
            ensure_protocol_version(output.schema_version, "search-results.json")?;
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
            if job.job_type == "pi_verification" { "PI 重新核验" } else { "机会重新核验" },
            if output.active { "有效" } else { "可能失效" },
            output.checked_at,
            output.summary_zh,
            source_lines,
        );
        store_text_artifact(paths, &conn, target_id, &application_id, artifact_type, "zh", filename, &body)?;
        record_sources(&conn, "contact_target", target_id, &output.sources)?;
        conn.execute(
            "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id)
             VALUES(?1,?2,?3,?3)",
            params![job.id, artifact_type, target_id],
        )?;
    }
    Ok(json!({"reviewOnly":true,"active":output.active,"verification":output}))
}

fn import_follow_up_scan(paths: &AppPaths, job_id: &str, output: FollowUpScanOutput) -> Result<Value> {
    let conn = db::connect(&paths.database)?;
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
    if live.exists() {
        let backup = paths.backups.join(format!("{}-learned_preferences.json",Utc::now().format("%Y%m%dT%H%M%SZ")));
        fs::copy(&live,backup)?;
    }
    fs::write(&live,serde_json::to_vec_pretty(&value)?)?;
    Ok(json!({"observations":observations.len(),"path":live}))
}

async fn import_search_output(
    paths: &AppPaths,
    job_id: &str,
    payload: &Value,
    mut output: SearchOutput,
) -> Result<Value> {
    let threshold = payload.get("threshold").and_then(Value::as_f64).unwrap_or(75.0);
    output.opportunities.truncate(20);
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    for opportunity in output.opportunities {
        if let Err(error) = validate_opportunity(&opportunity) {
            warnings.push(format!("{}：{error}", opportunity.title));
            continue;
        }
        for mut contact in opportunity.contacts.iter().cloned() {
            if contact.fit_score <= threshold {
                continue;
            }
            match crate::cv_schema::normalize_value(&contact.materials.cv_data) {
                Ok(value) => contact.materials.cv_data = value,
                Err(error) => {
                    warnings.push(format!("{} / {}：CV 结构不兼容（{error:#}）", opportunity.title, contact.name));
                    continue;
                }
            }
            if let Err(error) = validate_contact(&contact) {
                warnings.push(format!("{} / {}：{error}", opportunity.title, contact.name));
                continue;
            }
            candidates.push((contact.fit_score, opportunity.clone(), contact));
        }
    }
    candidates.sort_by(|a,b| b.0.total_cmp(&a.0));
    if candidates.is_empty() {
        bail!("没有结果同时通过职业层级、来源、严格阈值和完整材料校验：{}", warnings.join("；"))
    }
    let mut imported = Vec::new();
    for (_, opportunity, contact) in candidates {
        if imported.len() >= 5 {
            break;
        }
        let validated_pages = match typst::validate_cv_data(paths, &contact.materials.cv_data).await {
            Ok(page_count) => page_count,
            Err(error) => {
                warnings.push(format!("{}：目标定制 CV 未通过两页预检（{error:#}）", contact.name));
                continue;
            }
        };
        let target = upsert_complete_contact(paths, job_id, &opportunity, &contact)?;
        let generated = typst::generate_cv(paths, &target.target_id).await
            .with_context(|| format!("{}：已通过预检，但正式 CV 生成失败", contact.name))?;
        imported.push(json!({
            "targetId":target.target_id,
            "applicationId":target.application_id,
            "opportunityId":target.opportunity_id,
            "contact":contact.name,
            "cvPages":generated.page_count,
            "validatedPages":validated_pages,
        }));
    }
    if imported.is_empty() {
        bail!("没有候选人的目标定制 CV 通过两页预检：{}", warnings.join("；"))
    }
    Ok(json!({"imported":imported,"warnings":warnings,"thresholdStrictlyGreaterThan":threshold}))
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
    let conn = db::connect(&paths.database)?;
    let application_id: String = conn.query_row(
        "SELECT application_id FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL",
        [target_id], |row| row.get(0),
    )?;
    let analysis = format!(
        "# 回复处理判断\n\n- 决策：{}\n- 工作流阶段：{}\n- 建议动作：{}\n\n{}\n",
        output.decision,
        if next_status == "shelved" { "搁置" } else { "跟进" },
        output.recommended_action,
        output.summary_zh
    );
    store_text_artifact(paths,&conn,target_id,&application_id,"reply_analysis","zh","reply-analysis.md",&analysis)?;
    store_text_artifact(paths,&conn,target_id,&application_id,"followup_email","en","followup-email-en.md",&output.draft_reply_en)?;
    store_text_artifact(paths,&conn,target_id,&application_id,"followup_email","zh","followup-email-zh.md",&output.draft_reply_zh)?;
    record_sources(&conn,"contact_target",target_id,&output.sources)?;
    conn.execute(
        "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id)
         VALUES(?1,'reply_analysis',?2,?2)",
        params![job.id,target_id],
    )?;
    drop(conn);
    let referrals = if output.recommended_opportunities.is_empty() {
        json!({"imported":[],"warnings":[]})
    } else {
        import_search_output(
            paths,
            &job.id,
            payload,
            SearchOutput { schema_version: protocol_version(), opportunities: output.recommended_opportunities },
        ).await?
    };
    db::update_target_status(&paths.database, target_id, next_status)?;
    Ok(json!({"targetId":target_id,"decision":output.decision,"nextStatus":next_status,"recommendedAction":output.recommended_action,"referrals":referrals}))
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

fn upsert_complete_contact(
    paths: &AppPaths,
    job_id: &str,
    opportunity: &FoundOpportunity,
    contact: &FoundContact,
) -> Result<ImportedTarget> {
    validate_opportunity(opportunity)?;
    validate_contact(contact)?;
    let normalized_cv = crate::cv_schema::normalize_value(&contact.materials.cv_data)?;
    let normalized_checklist = normalize_checklist_items(&contact.materials.checklist)?;
    for source in &opportunity.sources { validate_source(source)?; }
    let mut conn = db::connect(&paths.database)?;
    // Reserve the writer before the dedupe reads. A deferred transaction can read a
    // stale WAL snapshot and then fail with SQLITE_BUSY_SNAPSHOT when one of the other
    // five workers commits before this import attempts its first write.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let opportunity_id = upsert_opportunity(&tx, opportunity)?;
    let pi_id = upsert_pi(&tx, opportunity, contact)?;
    let normalized_email = contact.email.as_deref().map(normalize_email);
    let normalized_name = normalize_person(&contact.name);
    let existing: Option<(String,String)> = tx.query_row(
        "SELECT id,application_id FROM contact_targets_v2
         WHERE opportunity_id=?1 AND archived_at IS NULL
           AND ((?2 IS NOT NULL AND normalized_email=?2) OR normalized_name=?3)
         LIMIT 1",
        params![opportunity_id,normalized_email,normalized_name],
        |row|Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    let (target_id, application_id) = if let Some(value)=existing {
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
                organization,title,fit_score,priority,status,source_url,identity_key
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'ready_to_contact',?13,?14)",
            params![target_id,application_id,opportunity_id,pi_id,contact.name,normalized_name,
                contact.email,normalized_email,opportunity.organization,opportunity.title,
                contact.fit_score,contact.priority,opportunity.source_url,identity],
        )?;
        (target_id,application_id)
    };
    tx.commit()?;
    let conn=db::connect(&paths.database)?;
    typst::install_cv_sources(paths,&target_id,&application_id,&normalized_cv)?;
    store_text_artifact(paths,&conn,&target_id,&application_id,"email","en","email-en.md",&contact.materials.email_en)?;
    store_text_artifact(paths,&conn,&target_id,&application_id,"email","zh","email-zh.md",&contact.materials.email_zh)?;
    store_text_artifact(paths,&conn,&target_id,&application_id,"fit_analysis","en","fit-analysis.md",&contact.materials.fit_analysis)?;
    store_text_artifact(paths,&conn,&target_id,&application_id,"fit_analysis","zh","fit-analysis-zh.md",&contact.materials.fit_analysis_zh)?;
    store_text_artifact(paths,&conn,&target_id,&application_id,"pi_profile","en","pi-profile.md",&contact.materials.pi_profile)?;
    store_text_artifact(paths,&conn,&target_id,&application_id,"pi_profile","zh","pi-profile-zh.md",&contact.materials.pi_profile_zh)?;
    replace_checklist_with_conn(&conn,&target_id,&normalized_checklist)?;
    record_sources(&conn,"opportunity",&opportunity_id,&opportunity.sources)?;
    conn.execute(
        "INSERT OR IGNORE INTO native_job_results(job_id,result_type,entity_id,target_id)
         VALUES(?1,'contact_target',?2,?2)",
        params![job_id,target_id],
    )?;
    Ok(ImportedTarget{target_id,application_id,opportunity_id})
}

fn upsert_opportunity(conn:&Connection,value:&FoundOpportunity)->Result<String>{
    let identity=opportunity_identity(value);
    let existing:Option<String>=conn.query_row(
        "SELECT id FROM opportunities WHERE identity_key=?1 OR lower(rtrim(COALESCE(source_url,''),'/'))=?2 LIMIT 1",
        params![identity,canonical_url(&value.source_url)],|row|row.get(0),
    ).optional()?;
    if let Some(id)=existing{
        conn.execute(
            "UPDATE opportunities SET last_verified_at=?2,fit_score=MAX(COALESCE(fit_score,0),?3),
             status='open',updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id=?1",
            params![id,value.verified_at,value.contacts.iter().map(|item|item.fit_score).fold(0.0,f64::max)],
        )?;
        return Ok(id)
    }
    let id=format!("opportunity-native-{}",Uuid::new_v4().simple());
    conn.execute(
        "INSERT INTO opportunities(
            id,identity_key,title,organization,department,country,region,opportunity_type,status,
            deadline,source_url,source_title,discovered_at,last_verified_at,summary,keywords_json,fit_score,priority,notes
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'open',?9,?10,?11,?12,?12,?13,?14,?15,?16,?17)",
        params![id,identity,value.title,value.organization,value.department,value.country,value.region,
            value.opportunity_type.as_deref().unwrap_or("formal_postdoc"),value.deadline,value.source_url,
            value.source_title,value.verified_at,value.summary,serde_json::to_string(&value.keywords)?,
            value.contacts.iter().map(|item|item.fit_score).fold(0.0,f64::max),"review",value.external_id],
    )?;
    Ok(id)
}

fn upsert_pi(conn:&Connection,opportunity:&FoundOpportunity,contact:&FoundContact)->Result<String>{
    let normalized_email=contact.email.as_deref().map(normalize_email);
    let existing:Option<String>=conn.query_row(
        "SELECT id FROM pis WHERE (?1 IS NOT NULL AND lower(COALESCE(email,''))=?1)
          OR (lower(name)=lower(?2) AND lower(institution)=lower(?3)) LIMIT 1",
        params![normalized_email,contact.name,opportunity.organization],|row|row.get(0),
    ).optional()?;
    if let Some(id)=existing{return Ok(id)}
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
    let directory=target_directory(paths,target_id);
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
    let tx = conn.unchecked_transaction()?;
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
    for (index, item) in items.iter().cloned().enumerate() {
        if !matches!(item.status.as_str(), "ready" | "review" | "missing") {
            bail!("清单项 {} 的状态无效：{}", index + 1, item.status)
        }
        if !matches!(item.origin.as_str(), "verified" | "inferred") {
            bail!("清单项 {} 的来源类型无效：{}", index + 1, item.origin)
        }
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

fn validate_opportunity(value:&FoundOpportunity)->Result<()> {
    if !value.career_level_eligible { bail!("职业层级不符合博士后或研究员门槛") }
    if value.title.trim().is_empty() || value.organization.trim().is_empty() { bail!("职位或机构为空") }
    if !is_http_url(&value.source_url) { bail!("缺少有效的机会来源 URL") }
    if value.verified_at.trim().is_empty() { bail!("缺少核验时间") }
    chrono::DateTime::parse_from_rfc3339(&value.verified_at).context("机会核验时间不是 ISO-8601")?;
    if let Some(kind)=value.opportunity_type.as_deref() {
        if !matches!(kind,"formal_postdoc"|"fellowship"|"prospective_pi") { bail!("机会类型无效：{kind}") }
    }
    if value.sources.is_empty() { bail!("缺少来源证据") }
    for source in &value.sources { validate_source(source)?; }
    Ok(())
}

fn validate_contact(value:&FoundContact)->Result<()> {
    if value.name.trim().is_empty() { bail!("联系人姓名为空") }
    if let Some(email)=value.email.as_deref(){if !valid_email(email){bail!("联系人邮箱无效")}}
    crate::cv_schema::normalize(&value.materials.cv_data).context("缺少有效的结构化 CV")?;
    normalize_checklist_items(&value.materials.checklist)?;
    for (label,text) in [
        ("英文套磁信",&value.materials.email_en),("中文套磁信",&value.materials.email_zh),
        ("匹配分析",&value.materials.fit_analysis),("匹配分析中文说明",&value.materials.fit_analysis_zh),
        ("PI 简报",&value.materials.pi_profile),("PI 简报中文说明",&value.materials.pi_profile_zh),
    ] { if text.trim().len()<40 { bail!("{label}不完整") } }
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
    if let Some(id)=value.external_id.as_deref().map(str::trim).filter(|value|!value.is_empty()){
        return format!("external:{}",normalize_text(id))
    }
    if is_http_url(&value.source_url){return format!("url:{}",canonical_url(&value.source_url))}
    format!("fingerprint:{}:{}:{}",normalize_text(&value.organization),normalize_text(&value.title),value.deadline.as_deref().unwrap_or("unknown"))
}

fn canonical_url(value:&str)->String{
    if let Ok(mut url)=Url::parse(value){
        url.set_fragment(None);
        let tracking=["utm_source","utm_medium","utm_campaign","utm_term","utm_content","fbclid","gclid"];
        let pairs=url.query_pairs().filter(|(key,_)|!tracking.contains(&key.as_ref())).map(|(k,v)|(k.into_owned(),v.into_owned())).collect::<Vec<_>>();
        url.set_query(None);
        if !pairs.is_empty(){url.query_pairs_mut().extend_pairs(pairs);}
        return url.to_string().trim_end_matches('/').to_lowercase()
    }
    value.trim().trim_end_matches('/').to_lowercase()
}

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
    #[test] fn url_dedupe_removes_tracking(){assert_eq!(canonical_url("https://Example.edu/job/?utm_source=x#top"),"https://example.edu/job");}
    #[test] fn same_role_different_people_never_share_contact_key(){assert_ne!(normalize_person("Alice A"),normalize_person("Bob B"));}

    #[test]
    fn protocol_contracts_are_versioned_and_exact() {
        let search = result_contract("research_pi");
        assert_eq!(search["required"]["schemaVersion"], 1);
        assert_eq!(search["required"]["opportunities"][0]["contacts"][0]["materials"]["cvData"]["schemaVersion"], 1);
        assert_ne!(search["required"]["opportunities"][0]["contacts"][0]["materials"]["checklist"][0]["itemType"], "item");
        let reply = result_contract("reply_followup");
        assert!(reply["required"]["recommendedOpportunities"].is_array());
    }

    #[test]
    fn every_versioned_output_envelope_deserializes() -> Result<()> {
        let search:SearchOutput=serde_json::from_value(json!({"schemaVersion":1,"opportunities":[]}))?;
        let reply:ReplyOutput=serde_json::from_value(json!({
            "schemaVersion":1,"decision":"wait","summaryZh":"等待进一步消息并保留当前记录。",
            "recommendedAction":"暂不发送邮件。","draftReplyEn":"Draft reply retained for review only and never sent automatically.",
            "draftReplyZh":"仅供审核的中文回复草稿，系统绝不会自动发送。","sources":[],"recommendedOpportunities":[]
        }))?;
        let checklist:ChecklistResult=serde_json::from_value(json!({"schemaVersion":1,"items":[]}))?;
        let follow_up:FollowUpScanOutput=serde_json::from_value(json!({"schemaVersion":1,"items":[]}))?;
        let verification:VerificationOutput=serde_json::from_value(json!({"schemaVersion":1,"active":true,"summaryZh":"已核验。","checkedAt":"2026-08-31T00:00:00Z","sources":[]}))?;
        assert_eq!((search.schema_version,reply.schema_version,checklist.schema_version,follow_up.schema_version,verification.schema_version),(1,1,1,1,1));
        Ok(())
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
    fn one_opportunity_keeps_two_pi_statuses_independent() -> Result<()> {
        let temp=TempDir::new()?;
        let root=temp.path().to_path_buf();
        let paths=AppPaths{
            database:root.join("database/postdocos.sqlite3"),generated:root.join("generated"),
            profile:root.join("profile"),workspaces:root.join("workspaces"),codex_home:root.join("codex"),
            backups:root.join("backups"),cache:root.join("cache"),logs:root.join("logs"),
            runtime:PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime"),data_root:root,
        };
        paths.ensure()?;
        let conn=db::connect(&paths.database)?;
        conn.execute_batch(include_str!("../../../postdoc-os/postdoc_os/schema.sql"))?;
        conn.execute_batch(include_str!("../migrations/0008_native_desktop.sql"))?;
        conn.execute_batch(include_str!("../migrations/0009_reply_routing_and_submission_status.sql"))?;
        conn.execute("INSERT INTO native_jobs(id,job_type,status,provider_id,payload_json) VALUES('job-test','full_search','running','openai','{}')",[])?;
        drop(conn);
        let cv=json!({"schemaVersion":1,"name":"Hongbo Miao","tagline":"Marine AI","contact":"urbinohbmiao@gmail.com","affiliations":"HKU · HEU","sections":[{"title":"Research Profile","entries":[{"key":"Focus","body":"Verified underwater acoustics and marine robotics research."}]}]});
        let materials=MaterialPackage{
            cv_data:cv,email_en:"Complete English reviewable draft with verified facts and no invented claims for this contact.".into(),
            email_zh:"完整中文审核稿，仅使用已确认事实，不会自动发送，也不会提交任何申请。".into(),
            fit_analysis:"Evidence-based fit analysis with career gate, methods, gaps, logistics, and verified sources.".into(),
            fit_analysis_zh:"基于证据的中文匹配说明，覆盖职业层级、研究方法、能力缺口、现实条件与已核验来源。".into(),
            pi_profile:"Verified PI profile with research direction, recent trajectory, laboratory context, and sources.".into(),
            pi_profile_zh:"已核验的中文 PI 说明，覆盖研究方向、近期轨迹、实验室背景以及对应来源。".into(),
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
            keywords:vec![],career_level_eligible:true,verified_at:"2026-08-31T00:00:00Z".into(),
            sources:vec![SourceEvidence{title:"Official".into(),url:"https://example.edu/jobs/1".into(),checked_at:"2026-08-31T00:00:00Z".into(),evidence_type:"primary".into()}],contacts:vec![],
        };
        let mut invalid_materials = materials.clone();
        invalid_materials.cv_data = json!({"schemaVersion":1,"name":"Hongbo Miao"});
        let invalid = FoundContact{name:"Invalid PI".into(),email:Some("invalid@example.edu".into()),fit_score:90.0,priority:1,homepage_url:None,lab_url:None,research_summary:None,materials:invalid_materials};
        assert!(upsert_complete_contact(&paths,"job-test",&opportunity,&invalid).is_err());
        let conn=db::connect(&paths.database)?;
        let partial_count:i64=conn.query_row("SELECT COUNT(*) FROM contact_targets_v2",[],|row|row.get(0))?;
        assert_eq!(partial_count,0,"CV 校验失败前不应写入半成品联系人");
        drop(conn);
        let a=FoundContact{name:"Alice PI".into(),email:Some("alice@example.edu".into()),fit_score:90.0,priority:1,homepage_url:None,lab_url:None,research_summary:None,materials:materials.clone()};
        let b=FoundContact{name:"Bob PI".into(),email:Some("bob@example.edu".into()),fit_score:88.0,priority:2,homepage_url:None,lab_url:None,research_summary:None,materials};
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
}
