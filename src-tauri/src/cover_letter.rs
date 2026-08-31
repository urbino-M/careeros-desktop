use crate::cv_schema::{self, CvData};
use crate::db;
use crate::materials::{DiffEntry, RevisionResult};
use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use chrono::{Datelike, Local, Utc};
use lopdf::Document;
use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

const COVER_LETTER_TEMPLATE: &str = include_str!("../resources/templates/cover-letter.typ");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverLetterGenerationResult {
    pub pdf_path: String,
    pub source_path: String,
    pub text_path: String,
    pub backup_path: Option<String>,
    pub page_count: usize,
    pub source_sha256: String,
    pub revision: RevisionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverLetterData {
    name: String,
    headline_lines: Vec<String>,
    contact: String,
    date: String,
    recipient_lines: Vec<String>,
    subject: String,
    greeting: String,
    paragraphs: Vec<String>,
    closing: String,
    signature: String,
    footer: String,
}

struct TargetContext {
    application_id: String,
    name: String,
    organization: String,
    title: String,
    department: Option<String>,
    region: Option<String>,
    country: Option<String>,
    summary: Option<String>,
}

pub async fn generate(paths: &AppPaths, target_id: &str) -> Result<CoverLetterGenerationResult> {
    let conn = db::connect(&paths.database)?;
    let target = conn.query_row(
        "SELECT t.application_id,t.name,t.organization,t.title,o.department,o.region,o.country,o.summary
         FROM contact_targets_v2 t
         LEFT JOIN opportunities o ON o.id=t.opportunity_id
         WHERE t.id=?1 AND t.archived_at IS NULL",
        [target_id],
        |row| Ok(TargetContext {
            application_id: row.get(0)?,
            name: row.get(1)?,
            organization: row.get(2)?,
            title: row.get(3)?,
            department: row.get(4)?,
            region: row.get(5)?,
            country: row.get(6)?,
            summary: row.get(7)?,
        }),
    ).context("当前联系人不存在，无法生成 Cover Letter")?;

    let email_stored: Option<String> = conn.query_row(
        "SELECT path FROM contact_target_artifacts
         WHERE target_id=?1 AND artifact_type='email' AND language='en'",
        [target_id],
        |row| row.get(0),
    ).optional()?;
    let email_text = email_stored.as_deref()
        .map(|stored| resolve_data_path(&paths.data_root, stored))
        .filter(|path| path.is_file())
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_default();
    let cv_data = load_cv_data(paths, &conn, target_id);
    let data = build_data(&target, &email_text, cv_data.as_ref());
    drop(conn);
    render_and_persist(paths, target_id, &target.application_id, data, true).await
}

pub async fn regenerate_from_text(
    paths: &AppPaths,
    target_id: &str,
) -> Result<CoverLetterGenerationResult> {
    let conn = db::connect(&paths.database)?;
    let (application_id, data_stored, text_stored): (String, String, String) = conn.query_row(
        "SELECT t.application_id,d.path,m.path
         FROM contact_targets_v2 t
         JOIN contact_target_artifacts d ON d.target_id=t.id AND d.artifact_type='cover_letter_data' AND d.language='und'
         JOIN contact_target_artifacts m ON m.target_id=t.id AND m.artifact_type='cover_letter_text' AND m.language='en'
         WHERE t.id=?1 AND t.archived_at IS NULL",
        [target_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).context("当前联系人还没有可编辑的 Cover Letter；请先点击“添加 Cover Letter”")?;
    let data_path = resolve_data_path(&paths.data_root, &data_stored);
    let text_path = resolve_data_path(&paths.data_root, &text_stored);
    let existing: CoverLetterData = serde_json::from_slice(&fs::read(&data_path)?)
        .context("Cover Letter 结构化内容无效")?;
    let revised = parse_rendered_markdown(&fs::read_to_string(&text_path)?, existing)?;
    drop(conn);
    render_and_persist(paths, target_id, &application_id, revised, false).await
}

async fn render_and_persist(
    paths: &AppPaths,
    target_id: &str,
    application_id: &str,
    data: CoverLetterData,
    write_text: bool,
) -> Result<CoverLetterGenerationResult> {
    let conn = db::connect(&paths.database)?;

    let directory = target_material_dir(paths, target_id);
    let revisions_dir = directory.join("revisions");
    fs::create_dir_all(&revisions_dir)?;
    let source_path = directory.join("cover-letter.typ");
    let data_path = directory.join("cover-letter-data.json");
    let text_path = directory.join("cover-letter.md");
    fs::write(&source_path, COVER_LETTER_TEMPLATE)?;
    fs::write(&data_path, serde_json::to_vec_pretty(&data)?)?;
    if write_text {
        fs::write(&text_path, render_markdown(&data))?;
    }

    let binary = locate_typst_binary(paths)?;
    let temporary = directory.join(format!("cover-letter-{}.tmp.pdf", Uuid::new_v4().simple()));
    let output = Command::new(&binary)
        .arg("compile")
        .arg("--root")
        .arg(&directory)
        .arg(&source_path)
        .arg(&temporary)
        .output()
        .await
        .with_context(|| format!("无法启动内置 Typst：{}", binary.display()))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let _ = fs::remove_file(&temporary);
        bail!("Cover Letter 排版失败：{message}")
    }
    let document = Document::load(&temporary).context("Typst 输出的 Cover Letter 不是有效 PDF")?;
    let page_count = document.get_pages().len();
    if page_count != 1 {
        let _ = fs::remove_file(&temporary);
        bail!("Cover Letter 应为 1 页，当前生成了 {page_count} 页；请先精简英文套磁信后重试")
    }

    let existing: Option<String> = conn.query_row(
        "SELECT path FROM contact_target_artifacts
         WHERE target_id=?1 AND artifact_type='cover_letter' AND language='en'",
        [target_id],
        |row| row.get(0),
    ).optional()?;
    let backup = existing.as_deref()
        .map(|stored| resolve_data_path(&paths.data_root, stored))
        .filter(|path| path.is_file())
        .map(|current| -> Result<PathBuf> {
            let backup = revisions_dir.join(format!("{}-cover-letter.pdf", Utc::now().format("%Y%m%dT%H%M%SZ")));
            fs::copy(current, &backup)?;
            Ok(backup)
        })
        .transpose()?;
    let live_pdf = directory.join("cover-letter.pdf");
    fs::rename(&temporary, &live_pdf)?;

    let relative_pdf = display_path(&paths.data_root, &live_pdf);
    let relative_source = display_path(&paths.data_root, &source_path);
    let relative_data = display_path(&paths.data_root, &data_path);
    let relative_text = display_path(&paths.data_root, &text_path);
    let relative_backup = backup.as_ref().map(|path| display_path(&paths.data_root, path));
    let revision_id = format!("revision-native-{}", Uuid::new_v4().simple());
    let summary = if write_text {
        "按旧版 TeX Cover Letter 版式生成 1 页 A4 PDF；正文来自当前联系目标的英文材料".to_owned()
    } else {
        "根据已修订的 Cover Letter 正文重新生成 1 页 A4 PDF".to_owned()
    };
    let diff = vec![DiffEntry {
        line: 1,
        before: if backup.is_some() { "旧 Cover Letter PDF".into() } else { "没有 Cover Letter".into() },
        after: "新的 1 页 Typst Cover Letter PDF".into(),
    }];
    let tx = conn.unchecked_transaction()?;
    for (artifact_type, language, path) in [
        ("cover_letter", "en", relative_pdf.as_str()),
        ("cover_letter_typst", "en", relative_source.as_str()),
        ("cover_letter_data", "und", relative_data.as_str()),
        ("cover_letter_text", "en", relative_text.as_str()),
    ] {
        tx.execute(
            "INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET
               path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![target_id, artifact_type, language, path, application_id],
        )?;
    }
    tx.execute(
        "UPDATE applications SET cover_letter_path=?1,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id=?2",
        params![relative_pdf, application_id],
    )?;
    tx.execute(
        "UPDATE contact_target_checklist SET status='ready',note='On-demand Cover Letter generated locally with Typst',
           updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE target_id=?1 AND item_type='cover_letter'",
        [target_id],
    )?;
    tx.execute(
        "INSERT INTO artifact_revisions(id,application_id,artifact_type,language,artifact_path,backup_path,editor,note)
         VALUES(?1,?2,'cover_letter','en',?3,?4,'generator','Generated on demand by bundled Typst')",
        params![revision_id, application_id, relative_pdf, relative_backup],
    )?;
    tx.execute(
        "INSERT INTO revision_contact_targets(revision_id,target_id) VALUES(?1,?2)",
        params![revision_id, target_id],
    )?;
    tx.execute(
        "INSERT INTO revision_change_sets(id,revision_id,summary,locations_json,diff_json,provider_id,model_id,reasoning)
         VALUES(?1,?2,?3,?4,?5,'local-typst','cover-letter-tex-restore-v1',NULL)",
        params![
            format!("changes:{revision_id}"),
            revision_id,
            summary,
            json!(["Cover Letter 页面与排版", "当前联系目标正文"]).to_string(),
            serde_json::to_string(&diff)?,
        ],
    )?;
    tx.commit()?;

    Ok(CoverLetterGenerationResult {
        pdf_path: live_pdf.display().to_string(),
        source_path: source_path.display().to_string(),
        text_path: text_path.display().to_string(),
        backup_path: backup.map(|path| path.display().to_string()),
        page_count,
        source_sha256: format!("{:x}", Sha256::digest(fs::read(&data_path)?)),
        revision: RevisionResult {
            revision_id,
            artifact_path: relative_pdf,
            backup_path: relative_backup.unwrap_or_default(),
            summary,
            locations: vec!["Cover Letter 页面与排版".into(), "当前联系目标正文".into()],
            diff,
        },
    })
}

fn build_data(target: &TargetContext, email: &str, cv: Option<&CvData>) -> CoverLetterData {
    let (email_subject, email_greeting, email_paragraphs) = parse_email(email);
    let name = cv.map(|value| value.name.trim()).filter(|value| !value.is_empty())
        .unwrap_or("Hongbo Miao").to_owned();
    let contact = cv.map(|value| value.contact.trim()).filter(|value| !value.is_empty())
        .unwrap_or("urbinohbmiao@gmail.com · +86 188 4510 9373").to_owned();
    let mut headline_lines = cv.map(|value| value.affiliations.split(" · ")
        .map(str::trim).filter(|value| !value.is_empty()).take(2).map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    if headline_lines.is_empty() {
        headline_lines = vec![
            "Ph.D. Candidate, Underwater Acoustic Engineering, Harbin Engineering University".into(),
            "Visiting Ph.D. Student, Electrical and Computer Engineering (ECE), The University of Hong Kong".into(),
        ];
    }
    let now = Local::now();
    let date = format!("{} {} {}", now.day(), now.format("%B"), now.year());
    let mut recipient_lines = vec![format!("Professor {}", target.name.trim())];
    if let Some(department) = target.department.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        recipient_lines.push(department.to_owned());
    }
    recipient_lines.push(target.organization.clone());
    let location = [target.region.as_deref(), target.country.as_deref()]
        .into_iter().flatten().map(str::trim).filter(|value| !value.is_empty())
        .fold(Vec::<&str>::new(), |mut values, value| {
            if !values.iter().any(|existing| existing.eq_ignore_ascii_case(value)) { values.push(value); }
            values
        }).join(" · ");
    if !location.is_empty() { recipient_lines.push(location); }
    let subject = email_subject.unwrap_or_else(|| format!("Application for {}", target.title));
    let last_name = target.name.split_whitespace().last().unwrap_or(target.name.as_str());
    let greeting = email_greeting.unwrap_or_else(|| format!("Dear Professor {last_name},"));
    let paragraphs = if email_paragraphs.len() >= 2 {
        email_paragraphs
    } else {
        fallback_paragraphs(target, cv)
    };
    CoverLetterData {
        name: name.clone(),
        headline_lines,
        contact,
        date,
        recipient_lines,
        subject: subject.trim().trim_start_matches("Re:").trim().to_owned(),
        greeting,
        paragraphs,
        closing: "Sincerely,".into(),
        signature: name.clone(),
        footer: format!("{name}  |  {}", target.organization),
    }
}

fn parse_email(value: &str) -> (Option<String>, Option<String>, Vec<String>) {
    let subject_pattern = Regex::new(r"(?i)^\s*subject\s*:\s*(.+?)\s*$").expect("subject regex");
    let transport_pattern = Regex::new(r"(?i)^\s*(to|from|cc|bcc)\s*:").expect("transport regex");
    let mut subject = None;
    let mut body_lines = Vec::new();
    let normalized = value.replace("\r\n", "\n");
    for line in normalized.lines() {
        if let Some(capture) = subject_pattern.captures(line) {
            subject = capture.get(1).map(|value| normalize_text(value.as_str()));
        } else if !transport_pattern.is_match(line) {
            body_lines.push(line);
        }
    }
    let body = body_lines.join("\n");
    let mut blocks = body.split("\n\n").map(normalize_text).filter(|value| !value.is_empty())
        .filter(|value| !value.eq_ignore_ascii_case("cover letter"))
        .collect::<Vec<_>>();
    let greeting_index = blocks.iter().position(|value| value.to_ascii_lowercase().starts_with("dear "));
    let greeting = greeting_index.map(|index| blocks.remove(index));
    blocks.retain(|value| {
        let lower = value.to_ascii_lowercase();
        !lower.starts_with("sincerely")
            && !lower.starts_with("best regards")
            && !lower.starts_with("kind regards")
            && !lower.starts_with("hongbo miao")
    });
    (subject, greeting, blocks)
}

fn parse_rendered_markdown(value: &str, mut data: CoverLetterData) -> Result<CoverLetterData> {
    let normalized = value.replace("\r\n", "\n");
    let mut blocks = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if blocks.first().is_some_and(|value| value.trim_start_matches('#').trim().eq_ignore_ascii_case("cover letter")) {
        blocks.remove(0);
    }
    if blocks.len() < 6 {
        bail!("Cover Letter 正文结构不完整：请保留日期、收件人、Re: 主题、称呼、正文和结尾")
    }
    let subject_index = blocks.iter().position(|value| {
        value.trim_matches('*').trim().to_ascii_lowercase().starts_with("re:")
    }).context("Cover Letter 缺少“Re:”主题行")?;
    if subject_index < 2 || subject_index + 3 > blocks.len() {
        bail!("Cover Letter 的收件人与主题位置不完整")
    }
    data.date = normalize_text(blocks[0]);
    data.recipient_lines = blocks[1]
        .lines()
        .map(|line| normalize_text(line.trim_end_matches("  ")))
        .filter(|line| !line.is_empty())
        .collect();
    data.subject = normalize_text(blocks[subject_index])
        .trim_start_matches("Re:")
        .trim()
        .to_owned();
    data.greeting = normalize_text(blocks[subject_index + 1]);
    let closing_block = blocks.last().context("Cover Letter 缺少结尾")?;
    let closing_lines = closing_block.lines()
        .map(|line| normalize_text(line.trim_end_matches("  ")))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if closing_lines.len() < 2 {
        bail!("Cover Letter 结尾必须保留 Sincerely 和署名")
    }
    data.closing = closing_lines[0].clone();
    data.signature = closing_lines[1].clone();
    data.paragraphs = blocks[subject_index + 2..blocks.len() - 1]
        .iter()
        .map(|value| normalize_text(value))
        .filter(|value| !value.is_empty())
        .collect();
    if data.paragraphs.is_empty() {
        bail!("Cover Letter 正文不能为空")
    }
    Ok(data)
}

fn fallback_paragraphs(target: &TargetContext, cv: Option<&CvData>) -> Vec<String> {
    let mut paragraphs = vec![format!(
        "I am writing to apply for the {} at {}. I am a Ph.D. candidate in Underwater Acoustic Engineering at Harbin Engineering University and a Visiting Ph.D. Student in Electrical and Computer Engineering at The University of Hong Kong. I expect to complete my Ph.D. in December 2026 and would be available from January 2027.",
        target.title, target.organization,
    )];
    if let Some(cv) = cv {
        if let Some(profile) = cv.sections.iter().find(|section| section.title == "Research Profile") {
            let body = profile.entries.iter().take(2).map(|entry| entry.body.as_str()).collect::<Vec<_>>().join(" ");
            if !body.is_empty() { paragraphs.push(compact_text(&body, 950)); }
        }
        if let Some(alignment) = cv.sections.iter().find(|section| section.title == "Target Alignment")
            .and_then(|section| section.entries.first()) {
            paragraphs.push(compact_text(&alignment.body, 850));
        }
    }
    if paragraphs.len() < 3 {
        if let Some(summary) = target.summary.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            paragraphs.push(format!("The opportunity is especially relevant to my background because {}", compact_text(summary, 750)));
        }
    }
    paragraphs.push(format!(
        "Thank you for considering my application. I would welcome the opportunity to discuss how my background could contribute to the research programme at {}. I have enclosed my curriculum vitae and would be pleased to provide any additional information.",
        target.organization,
    ));
    paragraphs
}

fn normalize_text(value: &str) -> String {
    let markdown_link = Regex::new(r"\[([^\]]+)\]\([^\)]+\)").expect("markdown link regex");
    let value = markdown_link.replace_all(value, "$1");
    value.trim().trim_start_matches('#').trim()
        .replace("**", "").replace("__", "").replace('*', "")
        .split_whitespace().collect::<Vec<_>>().join(" ")
}

fn compact_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit { return normalize_text(value) }
    let shortened = value.chars().take(limit).collect::<String>();
    format!("{}...", normalize_text(&shortened))
}

fn render_markdown(data: &CoverLetterData) -> String {
    let recipient = data.recipient_lines.join("  \n");
    let paragraphs = data.paragraphs.join("\n\n");
    format!(
        "# Cover Letter\n\n{}\n\n{}\n\n**Re: {}**\n\n{}\n\n{}\n\n{}  \n**{}**\n",
        data.date, recipient, data.subject, data.greeting, paragraphs, data.closing, data.signature,
    )
}

fn load_cv_data(paths: &AppPaths, conn: &rusqlite::Connection, target_id: &str) -> Option<CvData> {
    let stored: String = conn.query_row(
        "SELECT path FROM contact_target_artifacts
         WHERE target_id=?1 AND artifact_type='cv_data' AND language='und'",
        [target_id],
        |row| row.get(0),
    ).optional().ok()??;
    let value: serde_json::Value = serde_json::from_slice(&fs::read(resolve_data_path(&paths.data_root, &stored)).ok()?).ok()?;
    cv_schema::normalize(&value).ok()
}

fn locate_typst_binary(paths: &AppPaths) -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("POSTDOCOS_TYPST_BIN") {
        let path = PathBuf::from(value);
        if path.is_file() { return Ok(path) }
    }
    for path in [paths.runtime.join("typst"), paths.data_root.join("runtime/typst")] {
        if path.is_file() { return Ok(path) }
    }
    bail!("没有找到内置 Typst 运行时")
}

fn target_material_dir(paths: &AppPaths, target_id: &str) -> PathBuf {
    let safe = target_id.chars().map(|character| {
        if character.is_ascii_alphanumeric() || character == '-' { character } else { '_' }
    }).collect::<String>();
    paths.generated.join("contact-targets").join(safe)
}

fn resolve_data_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() { path } else { root.join(path) }
}

fn display_path(root: &Path, value: &Path) -> String {
    value.strip_prefix(root).unwrap_or(value).to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_headers_are_not_rendered_as_letter_paragraphs() {
        let (subject, greeting, paragraphs) = parse_email(
            "To: pi@example.edu\nSubject: Marine robotics postdoc\n\nDear Professor Smith,\n\nFirst paragraph.\n\nSecond paragraph.\n\nSincerely,\nHongbo Miao",
        );
        assert_eq!(subject.as_deref(), Some("Marine robotics postdoc"));
        assert_eq!(greeting.as_deref(), Some("Dear Professor Smith,"));
        assert_eq!(paragraphs, vec!["First paragraph.", "Second paragraph."]);
    }

    #[test]
    fn editable_markdown_round_trips_back_to_typst_data() -> Result<()> {
        let original = CoverLetterData {
            name: "Hongbo Miao".into(),
            headline_lines: vec!["Ph.D. Candidate".into()],
            contact: "urbinohbmiao@gmail.com".into(),
            date: "31 August 2026".into(),
            recipient_lines: vec!["Professor Example".into(), "Example University".into()],
            subject: "Application for Research Fellow".into(),
            greeting: "Dear Professor Example,".into(),
            paragraphs: vec!["Old paragraph.".into(), "Second paragraph.".into()],
            closing: "Sincerely,".into(),
            signature: "Hongbo Miao".into(),
            footer: "Hongbo Miao | Example University".into(),
        };
        let revised_markdown = render_markdown(&original).replace("Old paragraph.", "Revised exact paragraph.");
        let revised = parse_rendered_markdown(&revised_markdown, original)?;
        assert_eq!(revised.paragraphs[0], "Revised exact paragraph.");
        assert_eq!(revised.recipient_lines, vec!["Professor Example", "Example University"]);
        assert_eq!(revised.subject, "Application for Research Fellow");
        Ok(())
    }

    #[test]
    fn bundled_template_compiles_to_one_page() -> Result<()> {
        let data = CoverLetterData {
            name: "Hongbo Miao".into(),
            headline_lines: vec!["Ph.D. Candidate, Underwater Acoustic Engineering".into(), "Visiting Ph.D. Student, The University of Hong Kong".into()],
            contact: "urbinohbmiao@gmail.com · +86 188 4510 9373".into(),
            date: "31 August 2026".into(),
            recipient_lines: vec!["Professor Example".into(), "Department of Engineering".into(), "Example University".into()],
            subject: "Application for Research Fellow".into(),
            greeting: "Dear Professor Example,".into(),
            paragraphs: vec!["I am writing to apply for the Research Fellow position. My research focuses on underwater acoustics and marine sensing.".into(); 5],
            closing: "Sincerely,".into(),
            signature: "Hongbo Miao".into(),
            footer: "Hongbo Miao | Example University".into(),
        };
        let temp = tempfile::tempdir()?;
        fs::write(temp.path().join("cover-letter.typ"), COVER_LETTER_TEMPLATE)?;
        fs::write(temp.path().join("cover-letter-data.json"), serde_json::to_vec_pretty(&data)?)?;
        let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/typst");
        let output = std::process::Command::new(binary)
            .arg("compile").arg("--root").arg(temp.path())
            .arg(temp.path().join("cover-letter.typ"))
            .arg(temp.path().join("cover-letter.pdf")).output()?;
        if !output.status.success() { bail!("{}", String::from_utf8_lossy(&output.stderr)) }
        assert_eq!(Document::load(temp.path().join("cover-letter.pdf"))?.get_pages().len(), 1);
        Ok(())
    }
}
