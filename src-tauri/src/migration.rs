use crate::models::MigrationReport;
use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::{backup::Backup, params, Connection, OpenFlags, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const LEGACY_FOUNDATION_SCHEMA: &str =
    include_str!("../migrations/0001_legacy_foundation.sql");
const NATIVE_MIGRATION: &str = include_str!("../migrations/0008_native_desktop.sql");
const REPLY_ROUTING_MIGRATION: &str = include_str!("../migrations/0009_reply_routing_and_submission_status.sql");
const RESPONSES_PROVIDERS_MIGRATION: &str = include_str!("../migrations/0010_responses_model_providers.sql");
const SCHEDULER_LEASES_MIGRATION: &str = include_str!("../migrations/0011_scheduler_leases.sql");
const LATEST_NATIVE_SCHEMA_VERSION: i64 = 11;

pub fn initialize(paths: &AppPaths) -> Result<MigrationReport> {
    initialize_with_legacy_root(paths, None)
}

fn initialize_with_legacy_root(
    paths: &AppPaths,
    legacy_root: Option<&Path>,
) -> Result<MigrationReport> {
    paths.ensure()?;
    let mut imported = false;
    let mut created_fresh = false;
    let mut source_hash = None;
    let mut source_hash_after = None;
    let mut backup_path = None;

    if !paths.database.exists() {
        if let Some(root) = legacy_root {
            let source_db = root.join("data/postdoc.db");
            let before = sha256_file(&source_db)?;
            source_hash = Some(before.clone());

            let staging = paths.database.with_extension("sqlite3.importing");
            snapshot_sqlite(&source_db, &staging)?;
            fs::rename(&staging, &paths.database)?;
            copy_tree_if_missing(&root.join("generated"), &paths.generated)?;
            copy_tree_if_missing(&root.join("profile"), &paths.profile)?;
            copy_tree_if_missing(&root.join("data/job_logs"), &paths.logs.join("legacy-jobs"))?;

            let after = sha256_file(&source_db)?;
            source_hash_after = Some(after.clone());
            // The old Streamlit worker may still be writing while the native app
            // takes its SQLite online backup. The source is opened read-only, so
            // a changed hash here means an external writer advanced the old app;
            // the online-backup snapshot itself remains transactionally valid.
            imported = true;
        } else {
            create_fresh_database(&paths.database)?;
            created_fresh = true;
        }
    }

    if !created_fresh {
        let backup = backup_before_native_migration(paths)?;
        if let Some(path) = backup {
            backup_path = Some(path.display().to_string());
        }
    }

    let mut conn = open_migration_connection(&paths.database)?;
    apply_native_schema(&mut conn)?;
    migrate_contact_targets(&mut conn)?;
    // The first pass creates native tables. Contact targets are derived next;
    // rerunning the idempotent SQL now populates target-scoped artifacts,
    // replies, revisions, approvals, and checklists from those targets.
    apply_native_schema(&mut conn)?;
    crate::typst::migrate_typst_sources(paths)?;
    validate_core_counts(&conn, imported)?;

    let report = collect_report(
        &conn,
        imported,
        legacy_root,
        source_hash,
        source_hash_after,
        backup_path,
    )?;
    record_audit(&conn, &report)?;
    Ok(report)
}

fn create_fresh_database(path: &Path) -> Result<()> {
    let staging = path.with_extension("sqlite3.initializing");
    if staging.exists() {
        fs::remove_file(&staging)
            .with_context(|| format!("无法清理未完成的新数据库 {}", staging.display()))?;
    }
    let mut conn = Connection::open(&staging)
        .with_context(|| format!("无法创建新数据库 {}", staging.display()))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    {
        let tx = conn.transaction()?;
        tx.execute_batch(LEGACY_FOUNDATION_SCHEMA)?;
        tx.commit()?;
    }
    drop(conn);
    fs::rename(&staging, path).with_context(|| format!("无法启用新数据库 {}", path.display()))?;
    Ok(())
}

fn snapshot_sqlite(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    let source_conn = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ).with_context(|| format!("无法以只读方式打开旧数据库 {}", source.display()))?;
    source_conn.busy_timeout(std::time::Duration::from_secs(10))?;
    let mut destination_conn = Connection::open(destination)
        .with_context(|| format!("无法建立迁移副本 {}", destination.display()))?;
    {
        let backup = Backup::new(&source_conn, &mut destination_conn)?;
        backup.run_to_completion(128, std::time::Duration::from_millis(10), None)?;
    }
    destination_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(destination_conn);
    drop(source_conn);
    Ok(())
}

fn open_migration_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    Ok(conn)
}

fn backup_before_native_migration(paths: &AppPaths) -> Result<Option<PathBuf>> {
    let conn = Connection::open(&paths.database)?;
    let has_migration_table: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='native_schema_migrations')",
            [],
            |row| row.get(0),
        )?;
    let current_version = if has_migration_table {
        conn.query_row(
            "SELECT COALESCE(MAX(version),0) FROM native_schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    drop(conn);
    if current_version >= LATEST_NATIVE_SCHEMA_VERSION {
        return Ok(None);
    }

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let label = if current_version >= 8 {
        format!("schema-v{current_version}-before-v{LATEST_NATIVE_SCHEMA_VERSION}")
    } else {
        "schema-v7-before-native".into()
    };
    let destination = paths
        .backups
        .join(format!("{label}-{timestamp}.sqlite3"));
    fs::copy(&paths.database, &destination)?;
    Ok(Some(destination))
}

fn apply_native_schema(conn: &mut Connection) -> Result<()> {
    {
        let tx = conn.transaction()?;
        tx.execute_batch(NATIVE_MIGRATION)?;
        tx.execute(
            "INSERT OR IGNORE INTO native_schema_migrations(version, name) VALUES(8, 'native-desktop-foundation')",
            [],
        )?;
        tx.commit()?;
    }
    apply_reply_routing_schema(conn)?;
    apply_responses_provider_schema(conn)?;
    apply_scheduler_leases_schema(conn)?;
    Ok(())
}

fn apply_reply_routing_schema(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    let applied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_schema_migrations WHERE version=9)",
        [],
        |row| row.get(0),
    )?;
    if !applied {
        tx.execute_batch(REPLY_ROUTING_MIGRATION)?;
        tx.execute(
            "INSERT INTO native_schema_migrations(version,name) VALUES(9,'reply-routing-and-submission-status')",
            [],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn apply_responses_provider_schema(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    let applied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_schema_migrations WHERE version=10)",
        [],
        |row| row.get(0),
    )?;
    if !applied {
        tx.execute_batch(RESPONSES_PROVIDERS_MIGRATION)?;
        tx.execute(
            "INSERT INTO native_schema_migrations(version,name) VALUES(10,'responses-model-providers')",
            [],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn apply_scheduler_leases_schema(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    let applied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_schema_migrations WHERE version=11)",
        [],
        |row| row.get(0),
    )?;
    if !applied {
        tx.execute_batch(SCHEDULER_LEASES_MIGRATION)?;
        tx.execute(
            "INSERT INTO native_schema_migrations(version,name) VALUES(11,'scheduler-leases')",
            [],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn migrate_contact_targets(conn: &mut Connection) -> Result<()> {
    let existing: i64 = conn.query_row("SELECT COUNT(*) FROM contact_targets_v2", [], |row| row.get(0))?;
    if existing > 0 {
        return Ok(());
    }

    let tx = conn.transaction()?;
    migrate_application_defaults(&tx)?;
    apply_legacy_contact_overrides(&tx)?;
    tx.commit()?;
    Ok(())
}

fn migrate_application_defaults(tx: &Transaction<'_>) -> Result<()> {
    let mut statement = tx.prepare(
        "SELECT a.id,
                a.opportunity_id,
                a.pi_id,
                a.status,
                a.contacted_at,
                a.replied_at,
                a.follow_up_at,
                COALESCE(p.name, o.organization, '未命名联系人'),
                p.email,
                COALESCE(o.organization, p.institution, '未知机构'),
                COALESCE(o.title, '未命名机会'),
                COALESCE(o.fit_score, 0),
                o.source_url,
                a.created_at,
                a.updated_at
         FROM applications a
         LEFT JOIN opportunities o ON o.id = a.opportunity_id
         LEFT JOIN pis p ON p.id = COALESCE(a.pi_id, o.pi_id)
         ORDER BY a.created_at, a.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, f64>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, String>(14)?,
        ))
    })?;

    for row in rows {
        let (
            application_id,
            opportunity_id,
            pi_id,
            legacy_status,
            contacted_at,
            replied_at,
            follow_up_at,
            name,
            email,
            organization,
            title,
            fit_score,
            source_url,
            created_at,
            updated_at,
        ) = row?;

        let normalized_name = normalize_person(&name);
        let normalized_email = email.as_deref().map(normalize_email);
        let identity = contact_identity(
            opportunity_id.as_deref(),
            source_url.as_deref(),
            normalized_email.as_deref(),
            &normalized_name,
            &organization,
            &title,
        );

        if legacy_status == "closed" {
            tx.execute(
                "INSERT OR IGNORE INTO dedupe_tombstones(id, entity_type, identity_key, reason, source_id)
                 VALUES(?1, 'contact_target', ?2, 'legacy_closed', ?3)",
                params![format!("tombstone:{application_id}"), identity, application_id],
            )?;
            continue;
        }

        let status = map_application_status(&legacy_status);
        tx.execute(
            "INSERT INTO contact_targets_v2(
                id, application_id, opportunity_id, pi_id, name, normalized_name,
                email, normalized_email, organization, title, fit_score, priority,
                status, source_url, identity_key, contacted_at, replied_at, follow_up_at,
                created_at, updated_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 100,
                      ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                format!("target:{application_id}"),
                application_id,
                opportunity_id,
                pi_id,
                name,
                normalized_name,
                email,
                normalized_email,
                organization,
                title,
                fit_score,
                status,
                source_url,
                identity,
                contacted_at,
                replied_at,
                follow_up_at,
                created_at,
                updated_at,
            ],
        )?;
    }
    Ok(())
}

fn apply_legacy_contact_overrides(tx: &Transaction<'_>) -> Result<()> {
    let has_table: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='application_contact_targets')",
        [],
        |row| row.get(0),
    )?;
    if !has_table {
        return Ok(());
    }

    let mut statement = tx.prepare(
        "SELECT id, application_id, name, email, organization, title, fit_score, priority,
                status, source_url, contacted_at, replied_at, promoted_application_id,
                created_at, updated_at
         FROM application_contact_targets
         ORDER BY created_at, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<f64>>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, String>(14)?,
        ))
    })?;

    for row in rows {
        let (
            source_id,
            parent_application_id,
            name,
            email,
            organization,
            title,
            fit_score,
            priority,
            legacy_status,
            source_url,
            contacted_at,
            replied_at,
            promoted_application_id,
            created_at,
            updated_at,
        ) = row?;
        let target_application_id = promoted_application_id
            .as_deref()
            .unwrap_or(parent_application_id.as_str());
        let normalized_name = normalize_person(&name);
        let normalized_email = email.as_deref().map(normalize_email);

        let base: Option<(Option<String>, Option<String>, String, String)> = tx
            .query_row(
                "SELECT opportunity_id, pi_id, organization, title
                 FROM contact_targets_v2 WHERE application_id = ?1 LIMIT 1",
                [target_application_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let (opportunity_id, pi_id, base_org, base_title) = base.unwrap_or((None, None, String::new(), String::new()));
        let organization = organization.filter(|value| !value.trim().is_empty()).unwrap_or(base_org);
        let title = title.filter(|value| !value.trim().is_empty()).unwrap_or(base_title);
        let identity = contact_identity(
            opportunity_id.as_deref(),
            source_url.as_deref(),
            normalized_email.as_deref(),
            &normalized_name,
            &organization,
            &title,
        );

        if legacy_status == "closed" {
            tx.execute(
                "DELETE FROM contact_targets_v2 WHERE application_id = ?1",
                [target_application_id],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO dedupe_tombstones(id, entity_type, identity_key, reason, source_id)
                 VALUES(?1, 'contact_target', ?2, 'legacy_closed', ?3)",
                params![format!("tombstone:{source_id}"), identity, source_id],
            )?;
            continue;
        }

        let status = map_contact_status(&legacy_status);
        let existing_id: Option<String> = tx
            .query_row(
                "SELECT id FROM contact_targets_v2 WHERE application_id = ?1 LIMIT 1",
                [target_application_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(existing_id) = existing_id {
            tx.execute(
                "UPDATE contact_targets_v2
                 SET name=?2, normalized_name=?3, email=?4, normalized_email=?5,
                     organization=?6, title=?7, fit_score=COALESCE(?8, fit_score),
                     priority=?9, status=?10, source_url=COALESCE(?11, source_url),
                     identity_key=?12, source_contact_target_id=?13,
                     contacted_at=COALESCE(?14, contacted_at),
                     replied_at=COALESCE(?15, replied_at), updated_at=?16
                 WHERE id=?1",
                params![
                    existing_id,
                    name,
                    normalized_name,
                    email,
                    normalized_email,
                    organization,
                    title,
                    fit_score,
                    priority,
                    status,
                    source_url,
                    identity,
                    source_id,
                    contacted_at,
                    replied_at,
                    updated_at,
                ],
            )?;
        } else {
            tx.execute(
                "INSERT INTO contact_targets_v2(
                    id, application_id, opportunity_id, pi_id, name, normalized_name,
                    email, normalized_email, organization, title, fit_score, priority,
                    status, source_url, identity_key, source_contact_target_id,
                    contacted_at, replied_at, created_at, updated_at
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                          ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    format!("target:{source_id}"),
                    target_application_id,
                    opportunity_id,
                    pi_id,
                    name,
                    normalized_name,
                    email,
                    normalized_email,
                    organization,
                    title,
                    fit_score,
                    priority,
                    status,
                    source_url,
                    identity,
                    source_id,
                    contacted_at,
                    replied_at,
                    created_at,
                    updated_at,
                ],
            )?;
        }
    }
    Ok(())
}

fn map_application_status(value: &str) -> &'static str {
    match value {
        "contacted" => "contacted",
        "replied" => "replied",
        "follow_up" => "follow_up",
        _ => "ready_to_contact",
    }
}

fn map_contact_status(value: &str) -> &'static str {
    match value {
        "contacted" => "contacted",
        "replied" => "replied",
        _ => "ready_to_contact",
    }
}

fn normalize_email(value: &str) -> String {
    value.trim().to_lowercase()
}

fn normalize_person(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn normalize_text(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| if character.is_alphanumeric() { character } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn contact_identity(
    opportunity_id: Option<&str>,
    source_url: Option<&str>,
    email: Option<&str>,
    normalized_name: &str,
    organization: &str,
    title: &str,
) -> String {
    let opportunity = opportunity_id
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| source_url.filter(|value| !value.trim().is_empty()).map(normalize_text))
        .unwrap_or_else(|| format!("{}:{}", normalize_text(organization), normalize_text(title)));
    let contact = email
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| normalized_name.to_owned());
    format!("{opportunity}::{contact}")
}

fn validate_core_counts(conn: &Connection, imported: bool) -> Result<()> {
    if imported {
        let counts = [
            ("applications", 24_i64),
            ("opportunities", 27_i64),
            ("jobs", 80_i64),
            ("artifact_revisions", 102_i64),
            ("gmail_drafts", 18_i64),
        ];
        for (table, expected_minimum) in counts {
            let count: i64 =
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
            if count < expected_minimum {
                bail!("{table} 迁移数量异常：{count}，预期至少 {expected_minimum}")
            }
        }
    }
    let invalid: i64 = conn.query_row(
        "SELECT COUNT(*) FROM contact_targets_v2
         WHERE archived_at IS NULL
           AND status NOT IN ('ready_to_contact','contacted','replied','follow_up')",
        [],
        |row| row.get(0),
    )?;
    if invalid != 0 {
        bail!("发现 {invalid} 条非法联系状态")
    }
    Ok(())
}

fn collect_report(
    conn: &Connection,
    imported: bool,
    legacy_root: Option<&Path>,
    source_sha256: Option<String>,
    source_sha256_after: Option<String>,
    backup_path: Option<String>,
) -> Result<MigrationReport> {
    let count = |table: &str| -> Result<i64> {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))?)
    };
    Ok(MigrationReport {
        imported,
        legacy_root: legacy_root.map(|path| path.display().to_string()),
        source_sha256,
        source_sha256_after,
        backup_path,
        applications: count("applications")?,
        opportunities: count("opportunities")?,
        legacy_jobs: count("jobs")?,
        revisions: count("artifact_revisions")?,
        gmail_drafts: count("gmail_drafts")?,
        active_targets: conn.query_row(
            "SELECT COUNT(*) FROM contact_targets_v2 WHERE archived_at IS NULL",
            [],
            |row| row.get(0),
        )?,
        hidden_tombstones: count("dedupe_tombstones")?,
    })
}

fn record_audit(conn: &Connection, report: &MigrationReport) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS native_migration_audit (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_sha256 TEXT,
            source_sha256_after TEXT,
            report_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );",
    )?;
    conn.execute(
        "INSERT INTO native_migration_audit(source_sha256, source_sha256_after, report_json)
         SELECT ?1, ?2, ?3
         WHERE NOT EXISTS (SELECT 1 FROM native_migration_audit)",
        params![
            report.source_sha256,
            report.source_sha256_after,
            serde_json::to_string(report)?,
        ],
    )?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_tree_if_missing(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() || destination.read_dir().map(|mut entries| entries.next().is_some()).unwrap_or(false) {
        return Ok(());
    }
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_name() != ".DS_Store" {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn identity_keeps_people_separate() {
        let a = contact_identity(Some("opportunity-1"), None, Some("a@example.com"), "person-a", "Org", "Role");
        let b = contact_identity(Some("opportunity-1"), None, Some("b@example.com"), "person-b", "Org", "Role");
        assert_ne!(a, b);
    }

    #[test]
    fn status_mapping_is_strict() {
        assert_eq!(map_application_status("ready_for_review"), "ready_to_contact");
        assert_eq!(map_application_status("contacted"), "contacted");
        assert_eq!(map_contact_status("to_contact"), "ready_to_contact");
    }

    #[test]
    fn schema_v9_adds_shelving_and_submission_status_idempotently() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE native_schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL);
             INSERT INTO native_schema_migrations(version,name) VALUES(8,'native-desktop-foundation');
             CREATE TABLE contact_targets_v2(
                 id TEXT PRIMARY KEY,
                 status TEXT NOT NULL,
                 archived_at TEXT,
                 updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )?;

        apply_reply_routing_schema(&mut conn)?;
        apply_reply_routing_schema(&mut conn)?;
        conn.execute("INSERT INTO contact_targets_v2(id,status) VALUES('target-1','replied')", [])?;
        let (submission_status, shelved_at): (String, Option<String>) = conn.query_row(
            "SELECT submission_status,shelved_at FROM contact_targets_v2 WHERE id='target-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(submission_status, "not_set");
        assert!(shelved_at.is_none());
        let version_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM native_schema_migrations WHERE version=9",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(version_count, 1);
        assert!(conn.execute(
            "UPDATE contact_targets_v2 SET submission_status='invalid' WHERE id='target-1'",
            [],
        ).is_err());
        Ok(())
    }

    #[test]
    fn schema_v10_adds_responses_provider_connection_fields_idempotently() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE native_schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL);
             INSERT INTO native_schema_migrations(version,name) VALUES(9,'reply-routing-and-submission-status');
             CREATE TABLE model_providers(
                 id TEXT PRIMARY KEY,
                 display_name TEXT NOT NULL,
                 adapter_kind TEXT NOT NULL,
                 connection_mode TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 0,
                 built_in INTEGER NOT NULL DEFAULT 0,
                 sort_order INTEGER NOT NULL DEFAULT 100,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )?;

        apply_responses_provider_schema(&mut conn)?;
        apply_responses_provider_schema(&mut conn)?;
        conn.execute(
            "INSERT INTO model_providers(id,display_name,adapter_kind,connection_mode,base_url)
             VALUES('relay-test','Relay','responses','native_responses','https://relay.example/v1')",
            [],
        )?;
        let (base_url, wire_api): (String, String) = conn.query_row(
            "SELECT base_url,wire_api FROM model_providers WHERE id='relay-test'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(base_url, "https://relay.example/v1");
        assert_eq!(wire_api, "responses");
        let version_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM native_schema_migrations WHERE version=10",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(version_count, 1);
        Ok(())
    }

    #[test]
    fn schema_v11_adds_scheduler_leases_and_active_key_uniqueness() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE native_schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL);
             INSERT INTO native_schema_migrations(version,name) VALUES(10,'responses-model-providers');
             CREATE TABLE native_jobs(
                 id TEXT PRIMARY KEY,
                 status TEXT NOT NULL DEFAULT 'queued'
             );",
        )?;

        apply_scheduler_leases_schema(&mut conn)?;
        apply_scheduler_leases_schema(&mut conn)?;
        conn.execute(
            "INSERT INTO native_jobs(id,status,active_key) VALUES('job-1','queued','material:target:cv')",
            [],
        )?;
        assert!(conn.execute(
            "INSERT INTO native_jobs(id,status,active_key) VALUES('job-2','running','material:target:cv')",
            [],
        ).is_err());
        conn.execute("UPDATE native_jobs SET status='completed' WHERE id='job-1'", [])?;
        conn.execute(
            "INSERT INTO native_jobs(id,status,active_key) VALUES('job-2','queued','material:target:cv')",
            [],
        )?;
        let (timeout_seconds, lease_owner): (i64, Option<String>) = conn.query_row(
            "SELECT timeout_seconds,lease_owner FROM native_jobs WHERE id='job-2'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(timeout_seconds, 3600);
        assert!(lease_owner.is_none());
        let version_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM native_schema_migrations WHERE version=11",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(version_count, 1);
        Ok(())
    }

    #[test]
    fn fresh_install_creates_empty_native_database_and_reopens() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path().to_path_buf();
        let paths = AppPaths {
            database: root.join("database/careeros.sqlite3"),
            generated: root.join("generated"),
            profile: root.join("profile"),
            workspaces: root.join("workspaces"),
            codex_home: root.join("codex"),
            backups: root.join("backups"),
            cache: root.join("cache"),
            logs: root.join("logs"),
            runtime: root.join("runtime"),
            data_root: root,
        };

        let first = initialize_with_legacy_root(&paths, None)?;
        assert!(!first.imported);
        assert!(first.legacy_root.is_none());
        assert!(first.backup_path.is_none());
        assert_eq!(first.applications, 0);
        assert_eq!(first.opportunities, 0);
        assert_eq!(first.active_targets, 0);

        let conn = open_migration_connection(&paths.database)?;
        let schema_version: i64 = conn.query_row(
            "SELECT MAX(version) FROM native_schema_migrations",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(schema_version, LATEST_NATIVE_SCHEMA_VERSION);
        let foreign_key_violations: i64 =
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        assert_eq!(foreign_key_violations, 0);
        drop(conn);

        let dashboard = crate::db::dashboard(&paths.database, "postdoc")?;
        assert!(dashboard.metrics.iter().all(|metric| metric.value == 0));
        assert!(dashboard.regions.is_empty());
        assert!(dashboard.priority_targets.is_empty());
        assert!(crate::db::list_targets(&paths.database, "postdoc", None, None, None, 0, 20)?.is_empty());

        let second = initialize_with_legacy_root(&paths, None)?;
        assert!(!second.imported);
        assert_eq!(second.applications, 0);
        assert_eq!(second.active_targets, 0);
        Ok(())
    }

}
