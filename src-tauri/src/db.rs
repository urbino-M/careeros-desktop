use crate::models::{
    ArtifactItem, ChecklistItem, DashboardData, DashboardMetric, JobGroups, JobSummary,
    InboundReplyRequest, ProviderInfo, ProviderModelInfo, RegionCount, ReplyItem,
    ProviderRuntimeConfig, ProviderRuntimeModel, RevisionItem, TargetCard, TargetDetail,
    TaskModelDefault,
};
use crate::paths::AppPaths;
use crate::providers::ProviderDiscovery;
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

pub fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(Duration::from_secs(10))?;
    Ok(conn)
}

pub fn dashboard(path: &Path, career_track: &str) -> Result<DashboardData> {
    validate_career_track(career_track)?;
    let conn = connect(path)?;
    let status_count = |status: &str| -> Result<i64> {
        let sql = if status == "shelved" {
            "SELECT COUNT(*)
             FROM contact_targets_v2 t
             LEFT JOIN opportunities o ON o.id=t.opportunity_id
             WHERE t.archived_at IS NULL AND t.shelved_at IS NOT NULL
               AND (
                    (?1='internship' AND o.opportunity_type='industry_internship')
                    OR (?1='postdoc' AND COALESCE(o.opportunity_type,'')<>'industry_internship')
               )"
        } else {
            "SELECT COUNT(*)
             FROM contact_targets_v2 t
             LEFT JOIN opportunities o ON o.id=t.opportunity_id
             WHERE t.archived_at IS NULL AND t.shelved_at IS NULL
               AND t.status=?2
               AND (
                    (?1='internship' AND o.opportunity_type='industry_internship')
                    OR (?1='postdoc' AND COALESCE(o.opportunity_type,'')<>'industry_internship')
               )"
        };
        Ok(if status == "shelved" {
            conn.query_row(sql, [career_track], |row| row.get(0))?
        } else {
            conn.query_row(sql, params![career_track, status], |row| row.get(0))?
        })
    };
    let total: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM contact_targets_v2 t
         LEFT JOIN opportunities o ON o.id=t.opportunity_id
         WHERE t.archived_at IS NULL
           AND (
                (?1='internship' AND o.opportunity_type='industry_internship')
                OR (?1='postdoc' AND COALESCE(o.opportunity_type,'')<>'industry_internship')
           )",
        [career_track],
        |row| row.get(0),
    )?;
    let high_fit: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM contact_targets_v2 t
         LEFT JOIN opportunities o ON o.id=t.opportunity_id
         WHERE t.archived_at IS NULL AND t.shelved_at IS NULL AND t.fit_score >= 85
           AND (
                (?1='internship' AND o.opportunity_type='industry_internship')
                OR (?1='postdoc' AND COALESCE(o.opportunity_type,'')<>'industry_internship')
           )",
        [career_track],
        |row| row.get(0),
    )?;
    let replied = status_count("replied")?;
    let (follow_up_running, follow_up_review): (i64, i64) = conn.query_row(
        "SELECT
            COUNT(DISTINCT CASE WHEN j.status IN ('queued','running') THEN j.target_id END),
            COUNT(DISTINCT CASE WHEN j.status='needs_review' THEN j.target_id END)
         FROM native_jobs j
         LEFT JOIN contact_targets_v2 t ON t.id=j.target_id
         LEFT JOIN opportunities o ON o.id=t.opportunity_id
         WHERE j.job_type='reply_followup' AND t.shelved_at IS NULL
           AND (
                (?1='internship' AND o.opportunity_type='industry_internship')
                OR (?1='postdoc' AND COALESCE(o.opportunity_type,'')<>'industry_internship')
           )",
        [career_track],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let follow_up_helper = format!(
        "{} 封新回复 · {} 处理中 · {} 待审核",
        replied, follow_up_running, follow_up_review
    );
    let metrics = if career_track == "internship" {
        let submission_count = |submission_status: &str| -> Result<i64> {
            Ok(conn.query_row(
                "SELECT COUNT(*)
                 FROM contact_targets_v2 t
                 LEFT JOIN opportunities o ON o.id=t.opportunity_id
                 WHERE t.archived_at IS NULL AND t.submission_status=?2
                   AND ?1='internship'
                   AND o.opportunity_type='industry_internship'",
                params![career_track, submission_status],
                |row| row.get(0),
            )?)
        };
        vec![
            DashboardMetric { key: "all".into(), label: "申请机会".into(), value: total, helper: "只显示行业 Internship".into() },
            DashboardMetric { key: "high_fit".into(), label: "高匹配".into(), value: high_fit, helper: "评分 ≥ 85".into() },
            DashboardMetric { key: "portal_pending".into(), label: "待投递".into(), value: submission_count("portal_pending")?, helper: "已核验，等待官网投递".into() },
            DashboardMetric { key: "submitted".into(), label: "已投递".into(), value: submission_count("submitted")?, helper: "等待面试或后续通知".into() },
            DashboardMetric { key: "not_set".into(), label: "未开始".into(), value: submission_count("not_set")?, helper: "还没有记录投递动作".into() },
            DashboardMetric { key: "not_required".into(), label: "无需投递".into(), value: submission_count("not_required")?, helper: "仅保存机会与资格核验".into() },
        ]
    } else {
        vec![
            DashboardMetric { key: "all".into(), label: "联系目标".into(), value: total, helper: "按 PI / 邮箱独立管理".into() },
            DashboardMetric { key: "high_fit".into(), label: "高匹配".into(), value: high_fit, helper: "评分 ≥ 85".into() },
            DashboardMetric { key: "ready_to_contact".into(), label: "待联系".into(), value: status_count("ready_to_contact")?, helper: "尚未确认发送".into() },
            DashboardMetric { key: "contacted".into(), label: "已联系".into(), value: status_count("contacted")?, helper: "已确认发送".into() },
            DashboardMetric { key: "replied".into(), label: "已回复".into(), value: replied, helper: "等待判断下一步".into() },
            DashboardMetric { key: "follow_up".into(), label: "跟进".into(), value: status_count("follow_up")?, helper: follow_up_helper },
            DashboardMetric { key: "shelved".into(), label: "搁置".into(), value: status_count("shelved")?, helper: "明确拒绝或无需继续".into() },
        ]
    };

    let mut region_statement = conn.prepare(
        "SELECT NULLIF(TRIM(o.region),''), NULLIF(TRIM(o.country),''), COUNT(*)
         FROM contact_targets_v2 t
         LEFT JOIN opportunities o ON o.id=t.opportunity_id
         WHERE t.archived_at IS NULL
           AND (
                (?1='internship' AND o.opportunity_type='industry_internship')
                OR (?1='postdoc' AND COALESCE(o.opportunity_type,'')<>'industry_internship')
           )
         GROUP BY NULLIF(TRIM(o.region),''), NULLIF(TRIM(o.country),'')",
        )?;
    let raw_regions = region_statement
        .query_map([career_track], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut region_counts = BTreeMap::new();
    for (region, country, count) in raw_regions {
        *region_counts
            .entry(dashboard_region(region.as_deref(), country.as_deref()))
            .or_insert(0) += count;
    }
    let mut regions = region_counts
        .into_iter()
        .map(|(region, count)| RegionCount { region, count })
        .collect::<Vec<_>>();
    regions.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.region.cmp(&right.region))
    });
    regions.truncate(8);

    let priority_targets = list_targets_with_conn(
        &conn,
        career_track,
        if career_track == "postdoc" { Some("ready_to_contact") } else { None },
        if career_track == "internship" { Some("portal_pending") } else { None },
        None,
        0,
        4,
        true,
    )?;
    Ok(DashboardData {
        metrics,
        regions,
        priority_targets,
    })
}

fn dashboard_region(region: Option<&str>, country: Option<&str>) -> String {
    let region = region.map(str::trim).filter(|value| !value.is_empty());
    let country = country.map(str::trim).filter(|value| !value.is_empty());
    if region.is_some_and(|value| value.eq_ignore_ascii_case("Europe"))
        || country.is_some_and(is_european_country)
    {
        return "Europe".into();
    }
    region.or(country).unwrap_or("其他").to_owned()
}

fn is_european_country(country: &str) -> bool {
    matches!(
        country.to_ascii_lowercase().as_str(),
        "albania"
            | "andorra"
            | "austria"
            | "belarus"
            | "belgium"
            | "bosnia and herzegovina"
            | "bulgaria"
            | "croatia"
            | "cyprus"
            | "czech republic"
            | "czechia"
            | "denmark"
            | "estonia"
            | "finland"
            | "france"
            | "germany"
            | "greece"
            | "hungary"
            | "iceland"
            | "ireland"
            | "italy"
            | "kosovo"
            | "latvia"
            | "liechtenstein"
            | "lithuania"
            | "luxembourg"
            | "malta"
            | "moldova"
            | "monaco"
            | "montenegro"
            | "netherlands"
            | "north macedonia"
            | "norway"
            | "poland"
            | "portugal"
            | "romania"
            | "russia"
            | "san marino"
            | "serbia"
            | "slovakia"
            | "slovenia"
            | "spain"
            | "sweden"
            | "switzerland"
            | "ukraine"
            | "united kingdom"
            | "vatican city"
    )
}

pub fn list_targets(
    path: &Path,
    career_track: &str,
    status: Option<&str>,
    submission_status: Option<&str>,
    search: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Vec<TargetCard>> {
    validate_career_track(career_track)?;
    validate_status_filter(status)?;
    validate_submission_status_filter(submission_status)?;
    let conn = connect(path)?;
    list_targets_with_conn(&conn, career_track, status, submission_status, search, offset, limit.clamp(1, 100), false)
}

fn list_targets_with_conn(
    conn: &Connection,
    career_track: &str,
    status: Option<&str>,
    submission_status: Option<&str>,
    search: Option<&str>,
    offset: usize,
    limit: usize,
    high_fit_first: bool,
) -> Result<Vec<TargetCard>> {
    validate_career_track(career_track)?;
    validate_status_filter(status)?;
    validate_submission_status_filter(submission_status)?;
    let status = status.filter(|value| *value != "all");
    let submission_status = submission_status.filter(|value| *value != "all");
    let search = search.map(str::trim).filter(|value| !value.is_empty());
    let pattern = search.map(|value| format!("%{}%", value.to_lowercase()));
    let order = if high_fit_first {
        "t.fit_score DESC, t.priority, t.updated_at DESC"
    } else {
        "t.fit_score DESC, t.updated_at DESC"
    };
    let sql = format!(
        "SELECT t.id, t.application_id, t.opportunity_id, t.name, t.email,
                t.organization, t.title, o.country, o.region, t.fit_score,
                t.priority, CASE WHEN t.shelved_at IS NOT NULL THEN 'shelved' ELSE t.status END,
                t.submission_status, o.deadline, COALESCE(t.source_url,o.source_url), t.updated_at,
                CASE WHEN o.opportunity_type='industry_internship' THEN 'internship' ELSE 'postdoc' END
         FROM contact_targets_v2 t
         LEFT JOIN opportunities o ON o.id=t.opportunity_id
         WHERE t.archived_at IS NULL
           AND (
                (?1='internship' AND o.opportunity_type='industry_internship')
                OR (?1='postdoc' AND COALESCE(o.opportunity_type,'')<>'industry_internship')
           )
           AND (
                ?2 IS NULL
                OR (?2='shelved' AND t.shelved_at IS NOT NULL)
                OR (?2<>'shelved' AND t.shelved_at IS NULL AND t.status=?2)
           )
           AND (?3 IS NULL OR t.submission_status=?3)
           AND (?4 IS NULL OR lower(t.name) LIKE ?4 OR lower(t.organization) LIKE ?4
                OR lower(t.title) LIKE ?4 OR lower(COALESCE(t.email,'')) LIKE ?4)
         ORDER BY {order}
         LIMIT ?5 OFFSET ?6"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(
        params![career_track, status, submission_status, pattern, limit as i64, offset as i64],
        target_from_row,
    )?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn target_detail(path: &Path, data_root: &Path, target_id: &str) -> Result<TargetDetail> {
    let conn = connect(path)?;
    let target = conn
        .query_row(
            "SELECT t.id, t.application_id, t.opportunity_id, t.name, t.email,
                    t.organization, t.title, o.country, o.region, t.fit_score,
                    t.priority, CASE WHEN t.shelved_at IS NOT NULL THEN 'shelved' ELSE t.status END,
                    t.submission_status, o.deadline, COALESCE(t.source_url,o.source_url), t.updated_at,
                    CASE WHEN o.opportunity_type='industry_internship' THEN 'internship' ELSE 'postdoc' END
             FROM contact_targets_v2 t
             LEFT JOIN opportunities o ON o.id=t.opportunity_id
             WHERE t.id=?1 AND t.archived_at IS NULL",
            [target_id],
            target_from_row,
        )
        .optional()?
        .with_context(|| format!("没有找到联系目标 {target_id}"))?;

    let (summary, department, pi_research_summary, application_notes): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn.query_row(
        "SELECT o.summary, o.department, p.research_summary, a.notes
         FROM contact_targets_v2 t
         JOIN applications a ON a.id=t.application_id
         LEFT JOIN opportunities o ON o.id=t.opportunity_id
         LEFT JOIN pis p ON p.id=t.pi_id
         WHERE t.id=?1",
        [target_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;

    let mut artifacts_statement = conn.prepare(
        "SELECT artifact_type, language, path, updated_at
         FROM contact_target_artifacts WHERE target_id=?1
         ORDER BY CASE artifact_type
                    WHEN 'cv_pdf' THEN 10 WHEN 'cv_tex' THEN 20 WHEN 'email' THEN 30
                    WHEN 'fit_analysis' THEN 40 WHEN 'pi_profile' THEN 50 ELSE 100 END,
                  language",
    )?;
    let artifacts = artifacts_statement
        .query_map([target_id], |row| {
            let path_value: String = row.get(2)?;
            let resolved = resolve_data_path(data_root, &path_value);
            Ok(ArtifactItem {
                artifact_type: row.get(0)?,
                language: row.get(1)?,
                path: resolved.display().to_string(),
                exists: resolved.exists(),
                updated_at: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut checklist_statement = conn.prepare(
        "SELECT id, item_type, required, status, evidence, note
         FROM contact_target_checklist WHERE target_id=?1 ORDER BY sort_order, item_type",
    )?;
    let checklist = checklist_statement
        .query_map([target_id], |row| {
            Ok(ChecklistItem {
                id: row.get(0)?,
                item_type: row.get(1)?,
                required: row.get::<_, i64>(2)? != 0,
                status: row.get(3)?,
                evidence: row.get(4)?,
                note: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut replies_statement = conn.prepare(
        "SELECT r.id, r.sender, r.subject, r.body, r.received_at, r.created_at
         FROM application_replies r
         JOIN reply_contact_targets m ON m.reply_id=r.id
         WHERE m.target_id=?1 ORDER BY COALESCE(r.received_at,r.created_at) DESC",
    )?;
    let replies = replies_statement
        .query_map([target_id], |row| {
            Ok(ReplyItem {
                id: row.get(0)?,
                sender: row.get(1)?,
                subject: row.get(2)?,
                body: row.get(3)?,
                received_at: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut revisions_statement = conn.prepare(
        "SELECT r.id, r.artifact_type, r.language, r.artifact_path, r.backup_path,
                c.job_id, r.editor, r.note, c.summary, c.locations_json, c.diff_json,
                c.provider_id, c.model_id, c.reasoning, r.created_at
         FROM artifact_revisions r
         JOIN revision_contact_targets m ON m.revision_id=r.id
         LEFT JOIN revision_change_sets c ON c.revision_id=r.id
         WHERE m.target_id=?1 ORDER BY r.created_at DESC LIMIT 100",
    )?;
    let revisions = revisions_statement
        .query_map([target_id], |row| {
            let artifact_path: String = row.get(3)?;
            let backup_path: Option<String> = row.get(4)?;
            Ok(RevisionItem {
                id: row.get(0)?,
                artifact_type: row.get(1)?,
                language: row.get(2)?,
                artifact_path: resolve_data_path(data_root, &artifact_path).display().to_string(),
                backup_path: backup_path.map(|value| {
                    resolve_data_path(data_root, &value).display().to_string()
                }),
                job_id: row.get(5)?,
                editor: row.get(6)?,
                note: row.get(7)?,
                summary: row.get(8)?,
                locations_json: row.get(9)?,
                diff_json: row.get(10)?,
                provider_id: row.get(11)?,
                model_id: row.get(12)?,
                reasoning: row.get(13)?,
                created_at: row.get(14)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(TargetDetail {
        target,
        summary,
        department,
        pi_research_summary,
        application_notes,
        artifacts,
        checklist,
        replies,
        revisions,
    })
}

pub fn update_target_status(path: &Path, target_id: &str, status: &str) -> Result<()> {
    validate_status_filter(Some(status))?;
    if status == "all" {
        bail!("“全部”只是筛选条件，不能保存为状态")
    }
    let conn = connect(path)?;
    let changed = conn.execute(
        "UPDATE contact_targets_v2
         SET status=CASE WHEN ?2='shelved' THEN status ELSE ?2 END,
             shelved_at=CASE
                 WHEN ?2='shelved' THEN COALESCE(shelved_at,strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                 ELSE NULL
             END,
             contacted_at=CASE WHEN ?2='contacted' THEN COALESCE(contacted_at,strftime('%Y-%m-%dT%H:%M:%SZ','now')) ELSE contacted_at END,
             replied_at=CASE WHEN ?2='replied' THEN COALESCE(replied_at,strftime('%Y-%m-%dT%H:%M:%SZ','now')) ELSE replied_at END,
             follow_up_at=CASE WHEN ?2='follow_up' THEN COALESCE(follow_up_at,strftime('%Y-%m-%dT%H:%M:%SZ','now')) ELSE follow_up_at END,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND archived_at IS NULL",
        params![target_id, status],
    )?;
    if changed != 1 {
        bail!("联系目标不存在，状态未更新")
    }
    Ok(())
}

pub fn update_submission_status(path: &Path, target_id: &str, status: &str) -> Result<()> {
    if !matches!(status, "not_set" | "portal_pending" | "submitted" | "not_required") {
        bail!("未知投递状态：{status}")
    }
    let conn = connect(path)?;
    let changed = conn.execute(
        "UPDATE contact_targets_v2
         SET submission_status=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND archived_at IS NULL",
        params![target_id, status],
    )?;
    if changed != 1 {
        bail!("联系目标不存在，投递状态未更新")
    }
    Ok(())
}

pub fn save_inbound_reply(path: &Path, request: &InboundReplyRequest) -> Result<ReplyItem> {
    let body = request.body.trim();
    if body.len() < 12 {
        bail!("回复原文太短，请粘贴完整内容")
    }
    if body.len() > 100_000 {
        bail!("回复原文超过 100,000 字符")
    }
    let mut conn = connect(path)?;
    let application_id: String = conn
        .query_row(
            "SELECT application_id FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL",
            [&request.target_id],
            |row| row.get(0),
        )
        .with_context(|| format!("没有找到联系目标 {}", request.target_id))?;
    let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
    let tx = conn.transaction()?;
    let existing_id: Option<String> = tx
        .query_row(
            "SELECT id FROM application_replies WHERE application_id=?1 AND body_sha256=?2",
            params![application_id, digest],
            |row| row.get(0),
        )
        .optional()?;
    let (reply_id, insert_new) = match existing_id {
        Some(value) => (value, false),
        None => (format!("reply-native-{}", Uuid::new_v4().simple()), true),
    };
    if insert_new {
        tx.execute(
            "INSERT INTO application_replies(
                id,application_id,sender,subject,body,body_sha256,received_at
             ) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                reply_id,
                application_id,
                request.sender.as_deref().map(str::trim).filter(|value| !value.is_empty()),
                request.subject.as_deref().map(str::trim).filter(|value| !value.is_empty()),
                body,
                digest,
                request.received_at,
            ],
        )?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO reply_contact_targets(reply_id,target_id) VALUES(?1,?2)",
        params![reply_id, request.target_id],
    )?;
    tx.execute(
        "UPDATE contact_targets_v2
         SET status='replied',
             shelved_at=NULL,
             replied_at=COALESCE(replied_at,?2,strftime('%Y-%m-%dT%H:%M:%SZ','now')),
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND archived_at IS NULL",
        params![request.target_id, request.received_at],
    )?;
    let result = tx.query_row(
        "SELECT id,sender,subject,body,received_at,created_at
         FROM application_replies WHERE id=?1",
        [&reply_id],
        |row| {
            Ok(ReplyItem {
                id: row.get(0)?,
                sender: row.get(1)?,
                subject: row.get(2)?,
                body: row.get(3)?,
                received_at: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    )?;
    tx.commit()?;
    Ok(result)
}

pub fn providers(path: &Path) -> Result<Vec<ProviderInfo>> {
    let conn = connect(path)?;
    let mut provider_statement = conn.prepare(
        "SELECT p.id, p.display_name, p.adapter_kind, p.connection_mode, p.enabled,
                p.base_url, p.last_validated_at, p.validation_message,
                EXISTS(SELECT 1 FROM provider_accounts a
                       WHERE a.provider_id=p.id AND a.enabled=1
                         AND a.secret_keychain_ref IS NOT NULL)
         FROM model_providers p
         WHERE p.adapter_kind != 'reserved'
         ORDER BY p.sort_order, p.display_name",
    )?;
    let providers = provider_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)? != 0,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    providers
        .into_iter()
        .map(|(id, display_name, adapter_kind, connection_mode, enabled, base_url, last_validated_at, validation_message, configured)| {
            let mut model_statement = conn.prepare(
                "SELECT id, model_slug, display_name, enabled, supports_reasoning,
                        supports_tools, supports_vision, metadata_json
                 FROM provider_models WHERE provider_id=?1 ORDER BY display_name",
            )?;
            let models = model_statement
                .query_map([&id], |row| {
                    Ok(ProviderModelInfo {
                        id: row.get(0)?,
                        slug: row.get(1)?,
                        display_name: row.get(2)?,
                        enabled: row.get::<_, i64>(3)? != 0,
                        supports_reasoning: row.get::<_, i64>(4)? != 0,
                        supports_tools: row.get::<_, i64>(5)? != 0,
                        supports_vision: row.get::<_, i64>(6)? != 0,
                        reasoning_levels: reasoning_levels(
                            &row.get::<_, String>(7)?,
                            &id,
                        ),
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(ProviderInfo {
                id,
                display_name,
                adapter_kind,
                connection_mode,
                enabled,
                base_url,
                configured,
                last_validated_at,
                validation_message,
                models,
            })
        })
        .collect()
}

pub fn task_defaults(path: &Path) -> Result<Vec<TaskModelDefault>> {
    let conn = connect(path)?;
    let mut statement = conn.prepare(
        "SELECT task_type, provider_id, model_id, reasoning
         FROM task_model_defaults ORDER BY task_type",
    )?;
    Ok(statement
        .query_map([], |row| {
            Ok(TaskModelDefault {
                task_type: row.get(0)?,
                provider_id: row.get(1)?,
                model_id: row.get(2)?,
                reasoning: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn save_task_default(path: &Path, value: &TaskModelDefault) -> Result<()> {
    let conn = connect(path)?;
    let valid: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM provider_models m
            JOIN model_providers p ON p.id=m.provider_id
            WHERE m.id=?1 AND m.provider_id=?2 AND m.enabled=1 AND p.enabled=1
         )",
        params![value.model_id, value.provider_id],
        |row| row.get(0),
    )?;
    if !valid {
        bail!("该服务商或模型尚未启用")
    }
    let account_id: Option<String> = conn
        .query_row(
            "SELECT id FROM provider_accounts
             WHERE provider_id=?1 AND enabled=1
             ORDER BY updated_at DESC, id LIMIT 1",
            [&value.provider_id],
            |row| row.get(0),
        )
        .optional()?;
    if value.provider_id != "openai" && account_id.is_none() {
        bail!("该服务商没有可用凭据，请重新连接")
    }
    conn.execute(
        "INSERT INTO task_model_defaults(task_type, provider_id, account_id, model_id, reasoning)
         VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(task_type) DO UPDATE SET provider_id=excluded.provider_id,
             account_id=excluded.account_id, model_id=excluded.model_id,
             reasoning=excluded.reasoning,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![value.task_type, value.provider_id, account_id, value.model_id, value.reasoning],
    )?;
    Ok(())
}

pub fn upsert_response_provider(path: &Path, discovery: &ProviderDiscovery) -> Result<()> {
    let mut conn = connect(path)?;
    let tx = conn.transaction()?;
    let sort_order = if discovery.id == "deepseek" { 40 } else { 70 };
    tx.execute(
        "INSERT INTO model_providers(
             id,display_name,adapter_kind,connection_mode,enabled,built_in,sort_order,
             base_url,wire_api,last_validated_at,validation_message
         ) VALUES(?1,?2,?3,'native_responses',1,0,?4,?5,'responses',
                  strftime('%Y-%m-%dT%H:%M:%SZ','now'),?6)
         ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name,
             adapter_kind=excluded.adapter_kind,connection_mode='native_responses',enabled=1,
             base_url=excluded.base_url,wire_api='responses',
             last_validated_at=excluded.last_validated_at,
             validation_message=excluded.validation_message,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![
            discovery.id,
            discovery.display_name,
            discovery.adapter_kind,
            sort_order,
            discovery.base_url,
            discovery.validation_message,
        ],
    )?;
    let account_id = format!("{}-active", discovery.id);
    tx.execute(
        "INSERT INTO provider_accounts(
             id,provider_id,display_name,auth_kind,secret_keychain_ref,enabled,last_validated_at
         ) VALUES(?1,?2,?3,'api_key',?4,1,strftime('%Y-%m-%dT%H:%M:%SZ','now'))
         ON CONFLICT(id) DO UPDATE SET provider_id=excluded.provider_id,
             display_name=excluded.display_name,auth_kind='api_key',
             secret_keychain_ref=excluded.secret_keychain_ref,enabled=1,
             last_validated_at=excluded.last_validated_at,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![
            account_id,
            discovery.id,
            format!("{} API Key", discovery.display_name),
            discovery.secret_reference,
        ],
    )?;
    tx.execute(
        "UPDATE provider_models SET enabled=0,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE provider_id=?1",
        [&discovery.id],
    )?;
    for model in &discovery.models {
        let model_id = format!("{}:{}", discovery.id, model.id);
        let metadata = serde_json::to_string(&serde_json::json!({
            "reasoningLevels": model.reasoning_levels,
        }))?;
        tx.execute(
            "INSERT INTO provider_models(
                 id,provider_id,model_slug,display_name,supports_reasoning,supports_tools,
                 supports_vision,enabled,metadata_json
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8)
             ON CONFLICT(provider_id,model_slug) DO UPDATE SET
                 id=excluded.id,display_name=excluded.display_name,
                 supports_reasoning=excluded.supports_reasoning,
                 supports_tools=excluded.supports_tools,supports_vision=excluded.supports_vision,
                 enabled=1,metadata_json=excluded.metadata_json,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![
                model_id,
                discovery.id,
                model.id,
                model.display_name,
                model.capabilities.reasoning as i64,
                model.capabilities.tools as i64,
                model.capabilities.vision as i64,
                metadata,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn disable_response_provider(path: &Path, provider_id: &str) -> Result<String> {
    if provider_id == "openai" {
        bail!("OpenAI 连接由上方账号设置管理")
    }
    let mut conn = connect(path)?;
    let tx = conn.transaction()?;
    let secret_reference: String = tx
        .query_row(
            "SELECT secret_keychain_ref FROM provider_accounts
             WHERE provider_id=?1 AND secret_keychain_ref IS NOT NULL
             ORDER BY updated_at DESC LIMIT 1",
            [provider_id],
            |row| row.get(0),
        )
        .context("没有找到该服务商的凭据记录")?;
    tx.execute(
        "UPDATE model_providers SET enabled=0,
             validation_message='已断开',updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND adapter_kind != 'reserved'",
        [provider_id],
    )?;
    tx.execute(
        "UPDATE provider_accounts SET enabled=0,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE provider_id=?1",
        [provider_id],
    )?;
    tx.execute(
        "UPDATE task_model_defaults SET provider_id='openai',account_id='openai-active',
             model_id=CASE WHEN task_type='maintenance' THEN 'openai:gpt-5.6-luna'
                           ELSE 'openai:gpt-5.6-sol' END,
             reasoning=CASE WHEN task_type='maintenance' THEN 'medium' ELSE 'xhigh' END,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE provider_id=?1",
        [provider_id],
    )?;
    tx.commit()?;
    Ok(secret_reference)
}

pub fn provider_runtime_config(
    path: &Path,
    provider_id: &str,
    account_id: Option<&str>,
) -> Result<ProviderRuntimeConfig> {
    let conn = connect(path)?;
    let selected: Option<(String, String, String, String, String)> = conn
        .query_row(
            "SELECT p.id,p.display_name,p.adapter_kind,p.base_url,a.secret_keychain_ref
             FROM model_providers p
             JOIN provider_accounts a ON a.provider_id=p.id AND a.enabled=1
             WHERE p.id=?1 AND p.enabled=1 AND p.adapter_kind != 'reserved'
               AND (?2 IS NULL OR a.id=?2)
             ORDER BY a.updated_at DESC LIMIT 1",
            params![provider_id, account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?;
    let (id, display_name, adapter_kind, base_url, secret_reference) =
        selected.context("该模型服务尚未连接或已被禁用")?;
    let mut statement = conn.prepare(
        "SELECT model_slug,display_name,supports_vision,metadata_json
         FROM provider_models WHERE provider_id=?1 AND enabled=1 ORDER BY display_name",
    )?;
    let models = statement
        .query_map([&id], |row| {
            let metadata: String = row.get(3)?;
            Ok(ProviderRuntimeModel {
                slug: row.get(0)?,
                display_name: row.get(1)?,
                supports_vision: row.get::<_, i64>(2)? != 0,
                reasoning_levels: reasoning_levels(&metadata, &id),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if models.is_empty() {
        bail!("该模型服务没有已启用的模型")
    }
    Ok(ProviderRuntimeConfig {
        id,
        display_name,
        adapter_kind,
        base_url,
        secret_reference,
        models,
    })
}

fn reasoning_levels(metadata: &str, provider_id: &str) -> Vec<String> {
    let configured = serde_json::from_str::<serde_json::Value>(metadata)
        .ok()
        .and_then(|value| value.get("reasoningLevels").and_then(|levels| levels.as_array()).cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    if !configured.is_empty() {
        configured
    } else if provider_id == "openai" {
        vec!["low".into(), "medium".into(), "high".into(), "xhigh".into()]
    } else {
        vec!["low".into(), "medium".into(), "high".into()]
    }
}

pub fn set_openai_auth_kind(path: &Path, auth_kind: &str) -> Result<()> {
    if !matches!(auth_kind, "chatgpt_oauth" | "api_key") {
        bail!("未知 OpenAI 连接方式")
    }
    let conn = connect(path)?;
    conn.execute(
        "UPDATE provider_accounts
         SET auth_kind=?2,
             display_name=CASE ?2 WHEN 'api_key' THEN 'OpenAI API Key' ELSE 'ChatGPT / Codex OAuth' END,
             enabled=1, updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1",
        params!["openai-active", auth_kind],
    )?;
    conn.execute(
        "UPDATE task_model_defaults SET account_id='openai-active',
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE provider_id='openai'",
        [],
    )?;
    Ok(())
}

pub fn job_groups(path: &Path, page_size: usize) -> Result<JobGroups> {
    let conn = connect(path)?;
    let page_size = page_size.clamp(1, 100);
    let native_count: i64 = conn.query_row("SELECT COUNT(*) FROM native_jobs", [], |row| row.get(0))?;
    let native_review_count: i64 = conn.query_row("SELECT COUNT(*) FROM native_jobs WHERE status='needs_review'", [], |row| row.get(0))?;
    let running = query_native_jobs(&conn, Some("running"), 100)?;
    let queued = query_native_jobs(&conn, Some("queued"), 100)?;
    let needs_review = query_native_jobs(&conn, Some("needs_review"), page_size)?;
    let recent = query_native_jobs(&conn, None, page_size)?;
    Ok(JobGroups {
        running,
        queued,
        needs_review,
        needs_review_total: native_review_count,
        recent,
        recent_total: native_count,
        capacity: 5,
    })
}

fn query_native_jobs(conn: &Connection, status: Option<&str>, limit: usize) -> Result<Vec<JobSummary>> {
    let mut statement = conn.prepare(
        "SELECT id, job_type, target_id, status, progress, message, provider_id,
                account_id, model_id, reasoning, thread_id, error, created_at, started_at, finished_at
         FROM native_jobs
         WHERE (?1 IS NULL OR status=?1)
         ORDER BY CASE status WHEN 'running' THEN 0 WHEN 'queued' THEN 1 ELSE 2 END,
                  created_at DESC LIMIT ?2",
    )?;
    let mut jobs = statement
        .query_map(params![status, limit as i64], job_from_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    attach_result_targets(conn, &mut jobs)?;
    Ok(jobs)
}


fn target_from_row(row: &Row<'_>) -> rusqlite::Result<TargetCard> {
    Ok(TargetCard {
        id: row.get(0)?,
        application_id: row.get(1)?,
        opportunity_id: row.get(2)?,
        name: row.get(3)?,
        email: row.get(4)?,
        organization: row.get(5)?,
        title: row.get(6)?,
        country: row.get(7)?,
        region: row.get(8)?,
        fit_score: row.get(9)?,
        priority: row.get(10)?,
        status: row.get(11)?,
        submission_status: row.get(12)?,
        deadline: row.get(13)?,
        source_url: row.get(14)?,
        updated_at: row.get(15)?,
        career_track: row.get(16)?,
    })
}

fn job_from_row(row: &Row<'_>) -> rusqlite::Result<JobSummary> {
    Ok(JobSummary {
        id: row.get(0)?,
        job_type: row.get(1)?,
        target_id: row.get(2)?,
        result_target_ids: Vec::new(),
        status: row.get(3)?,
        progress: row.get(4)?,
        message: row.get(5)?,
        provider_id: row.get(6)?,
        account_id: row.get(7)?,
        model_id: row.get(8)?,
        reasoning: row.get(9)?,
        thread_id: row.get(10)?,
        error: row.get(11)?,
        created_at: row.get(12)?,
        started_at: row.get(13)?,
        finished_at: row.get(14)?,
    })
}

fn attach_result_targets(conn: &Connection, jobs: &mut [JobSummary]) -> Result<()> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT target_id FROM native_job_results
         WHERE job_id=?1 AND target_id IS NOT NULL
         ORDER BY created_at, target_id",
    )?;
    for job in jobs {
        job.result_target_ids = statement
            .query_map([&job.id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
    }
    Ok(())
}


fn validate_status_filter(status: Option<&str>) -> Result<()> {
    if let Some(value) = status {
        if !matches!(
            value,
            "ready_to_contact" | "contacted" | "replied" | "follow_up" | "shelved" | "all"
        ) {
            bail!("未知申请状态：{value}")
        }
    }
    Ok(())
}

fn validate_career_track(value: &str) -> Result<()> {
    if !matches!(value, "postdoc" | "internship") {
        bail!("未知职业系统：{value}")
    }
    Ok(())
}

fn validate_submission_status_filter(status: Option<&str>) -> Result<()> {
    if let Some(value) = status {
        if !matches!(value, "not_set" | "portal_pending" | "submitted" | "not_required" | "all") {
            bail!("未知投递状态：{value}")
        }
    }
    Ok(())
}

fn resolve_data_path(data_root: &Path, value: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        data_root.join(path)
    }
}

pub fn read_artifact(paths: &AppPaths, artifact_path: &str) -> Result<String> {
    let resolved = resolve_data_path(&paths.data_root, artifact_path);
    let canonical_root = paths.data_root.canonicalize()?;
    let canonical = resolved
        .canonicalize()
        .with_context(|| format!("材料文件不存在：{}", resolved.display()))?;
    if !canonical.starts_with(canonical_root) {
        bail!("材料路径不在 CareerOS 数据目录内")
    }
    let extension = canonical.extension().and_then(|value| value.to_str()).unwrap_or("");
    if !matches!(extension, "md" | "txt" | "json" | "tex" | "typ") {
        bail!("该材料不是可直接预览的文本文件")
    }
    Ok(std::fs::read_to_string(canonical)?)
}

pub fn read_pdf_preview(paths: &AppPaths, artifact_path: &str) -> Result<Vec<u8>> {
    let resolved = resolve_data_path(&paths.data_root, artifact_path);
    let canonical_root = paths.data_root.canonicalize()?;
    let canonical = resolved
        .canonicalize()
        .with_context(|| format!("PDF 文件不存在：{}", resolved.display()))?;
    if !canonical.starts_with(canonical_root) {
        bail!("PDF 路径不在 CareerOS 数据目录内")
    }
    if canonical.extension().and_then(|value| value.to_str()) != Some("pdf") {
        bail!("该材料不是 PDF 文件")
    }
    let metadata = canonical.metadata()?;
    if metadata.len() > 25 * 1024 * 1024 {
        bail!("PDF 超过 25 MB，请使用系统预览打开")
    }
    Ok(std::fs::read(canonical)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{AdapterModel, ModelCapabilities};
    use tempfile::TempDir;

    #[test]
    fn shelving_preserves_underlying_stage_and_submission_is_independent() -> Result<()> {
        let temp = TempDir::new()?;
        let database = temp.path().join("status.sqlite3");
        let conn = connect(&database)?;
        conn.execute_batch(
            "CREATE TABLE contact_targets_v2(
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                shelved_at TEXT,
                submission_status TEXT NOT NULL DEFAULT 'not_set',
                contacted_at TEXT,
                replied_at TEXT,
                follow_up_at TEXT,
                archived_at TEXT,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             INSERT INTO contact_targets_v2(id,status) VALUES('target-1','replied');",
        )?;
        drop(conn);

        update_target_status(&database, "target-1", "shelved")?;
        update_submission_status(&database, "target-1", "portal_pending")?;
        let conn = connect(&database)?;
        let (status, shelved, submission): (String, Option<String>, String) = conn.query_row(
            "SELECT status,shelved_at,submission_status FROM contact_targets_v2 WHERE id='target-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(status, "replied");
        assert!(shelved.is_some());
        assert_eq!(submission, "portal_pending");
        drop(conn);

        update_target_status(&database, "target-1", "follow_up")?;
        let conn = connect(&database)?;
        let (status, shelved): (String, Option<String>) = conn.query_row(
            "SELECT status,shelved_at FROM contact_targets_v2 WHERE id='target-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(status, "follow_up");
        assert!(shelved.is_none());
        assert!(update_submission_status(&database, "target-1", "invalid").is_err());
        Ok(())
    }

    #[test]
    fn dashboard_region_normalizes_european_city_and_subregion_labels() {
        assert_eq!(dashboard_region(Some("Trondheim"), Some("Norway")), "Europe");
        assert_eq!(dashboard_region(Some("Catalonia"), Some("Spain")), "Europe");
        assert_eq!(dashboard_region(Some("Europe"), Some("Netherlands")), "Europe");
        assert_eq!(
            dashboard_region(Some("Middle East"), Some("Saudi Arabia")),
            "Middle East"
        );
    }

    #[test]
    fn task_views_return_native_jobs_without_deleting_imported_history() -> Result<()> {
        let temp = TempDir::new()?;
        let database = temp.path().join("jobs.sqlite3");
        let conn = connect(&database)?;
        conn.execute_batch(
            "CREATE TABLE native_jobs(
                id TEXT PRIMARY KEY, job_type TEXT NOT NULL, target_id TEXT,
                status TEXT NOT NULL, progress INTEGER NOT NULL, message TEXT,
                provider_id TEXT NOT NULL, account_id TEXT, model_id TEXT,
                reasoning TEXT, thread_id TEXT, error TEXT, created_at TEXT NOT NULL,
                started_at TEXT, finished_at TEXT
             );
             CREATE TABLE native_job_results(
                job_id TEXT NOT NULL, target_id TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE jobs(id TEXT PRIMARY KEY);
             INSERT INTO native_jobs(
                id,job_type,status,progress,provider_id,created_at
             ) VALUES('native-1','full_search','completed',100,'openai','2026-09-02T01:00:00Z');
             INSERT INTO jobs(id) VALUES('legacy-1');",
        )?;
        drop(conn);

        let groups = job_groups(&database, 10)?;
        assert_eq!(groups.recent_total, 1);
        assert_eq!(groups.recent.len(), 1);
        assert_eq!(groups.recent[0].id, "native-1");

        let conn = connect(&database)?;
        let imported_count: i64 = conn.query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))?;
        assert_eq!(imported_count, 1);
        Ok(())
    }

    #[test]
    fn response_provider_round_trips_without_storing_secret_bytes() -> Result<()> {
        let temp = TempDir::new()?;
        let database = temp.path().join("providers.sqlite3");
        let conn = connect(&database)?;
        conn.execute_batch(
            "CREATE TABLE model_providers(
                id TEXT PRIMARY KEY,display_name TEXT NOT NULL,adapter_kind TEXT NOT NULL,
                connection_mode TEXT NOT NULL,enabled INTEGER NOT NULL,built_in INTEGER NOT NULL,
                sort_order INTEGER NOT NULL,base_url TEXT,wire_api TEXT NOT NULL,
                last_validated_at TEXT,validation_message TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE provider_accounts(
                id TEXT PRIMARY KEY,provider_id TEXT NOT NULL,display_name TEXT NOT NULL,
                auth_kind TEXT NOT NULL,secret_keychain_ref TEXT,account_email TEXT,
                enabled INTEGER NOT NULL,last_validated_at TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE provider_models(
                id TEXT PRIMARY KEY,provider_id TEXT NOT NULL,model_slug TEXT NOT NULL,
                display_name TEXT NOT NULL,supports_reasoning INTEGER NOT NULL,
                supports_tools INTEGER NOT NULL,supports_vision INTEGER NOT NULL,
                enabled INTEGER NOT NULL,metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(provider_id,model_slug)
             );",
        )?;
        drop(conn);
        let discovery = ProviderDiscovery {
            id: "relay-test".into(),
            display_name: "Test Relay".into(),
            adapter_kind: "responses_relay".into(),
            base_url: "https://relay.example/v1".into(),
            secret_reference: "model-provider:relay-test:api-key".into(),
            validation_message: "Responses 已验证".into(),
            models: vec![AdapterModel {
                id: "gpt-test".into(),
                display_name: "GPT Test".into(),
                capabilities: ModelCapabilities {
                    reasoning: true,
                    tools: true,
                    vision: false,
                    streaming: true,
                },
                reasoning_levels: vec!["low".into(), "high".into()],
            }],
        };
        upsert_response_provider(&database, &discovery)?;
        let listed = providers(&database)?;
        assert_eq!(listed.len(), 1);
        assert!(listed[0].configured);
        assert_eq!(listed[0].models[0].reasoning_levels, vec!["low", "high"]);
        let runtime = provider_runtime_config(&database, "relay-test", None)?;
        assert_eq!(runtime.secret_reference, discovery.secret_reference);
        assert_eq!(runtime.models[0].slug, "gpt-test");
        let raw = std::fs::read(&database)?;
        assert!(!String::from_utf8_lossy(&raw).contains("test-secret-value"));
        Ok(())
    }
}
