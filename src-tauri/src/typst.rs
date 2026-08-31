use crate::db;
use crate::cv_schema::{self, CvData, CvEntry, CvSection};
use crate::materials::{DiffEntry, RevisionResult};
use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use chrono::Utc;
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

const CV_TEMPLATE: &str = include_str!("../resources/templates/cv.typ");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CvGenerationResult {
    pub pdf_path: String,
    pub backup_path: Option<String>,
    pub page_count: usize,
    pub previous_page_count: Option<usize>,
    pub source_sha256: String,
    pub font_policy: String,
    pub revision: RevisionResult,
}

pub fn migrate_typst_sources(paths: &AppPaths) -> Result<usize> {
    let conn = db::connect(&paths.database)?;
    let mut statement = conn.prepare(
        "SELECT a.target_id,a.path,t.application_id
         FROM contact_target_artifacts a
         JOIN contact_targets_v2 t ON t.id=a.target_id
         WHERE a.artifact_type='cv_tex' AND a.language='en' AND t.archived_at IS NULL
         ORDER BY a.target_id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    let mut created = 0;
    for (target_id, stored, application_id) in rows {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM contact_target_artifacts
             WHERE target_id=?1 AND artifact_type='cv_typst' AND language='en')",
            [&target_id],
            |row| row.get(0),
        )?;
        if exists {
            continue;
        }
        let source = resolve_data_path(&paths.data_root, &stored);
        if !source.is_file() {
            continue;
        }
        let tex = fs::read_to_string(&source)?;
        let expanded = expand_makerubrics(&tex, &source)?;
        let data = parse_curve_tex(&expanded)?;
        let directory = target_material_dir(paths, &target_id);
        fs::create_dir_all(&directory)?;
        let typ_path = directory.join("cv.typ");
        let data_path = directory.join("cv-data.json");
        fs::write(&typ_path, CV_TEMPLATE)?;
        fs::write(&data_path, serde_json::to_vec_pretty(&data)?)?;
        conn.execute(
            "INSERT INTO contact_target_artifacts(
                target_id,artifact_type,language,path,source_application_id
             ) VALUES(?1,'cv_typst','en',?2,?3)",
            params![target_id, display_path(&paths.data_root, &typ_path), application_id],
        )?;
        conn.execute(
            "INSERT INTO contact_target_artifacts(
                target_id,artifact_type,language,path,source_application_id
             ) VALUES(?1,'cv_data','und',?2,?3)",
            params![target_id, display_path(&paths.data_root, &data_path), application_id],
        )?;
        created += 1;
    }
    Ok(created)
}

pub fn install_cv_sources(
    paths: &AppPaths,
    target_id: &str,
    application_id: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let proposed = cv_schema::normalize(value)
        .context("Agent 返回的 CV 结构化数据不符合模板")?;
    let data = best_existing_cv(paths, Some(target_id))
        .map(|baseline| cv_schema::merge_preserving_baseline(&baseline, &proposed))
        .unwrap_or(proposed);
    let directory = target_material_dir(paths, target_id);
    fs::create_dir_all(&directory)?;
    let typ_path = directory.join("cv.typ");
    let data_path = directory.join("cv-data.json");
    fs::write(&typ_path, CV_TEMPLATE)?;
    fs::write(&data_path, serde_json::to_vec_pretty(&data)?)?;
    let conn = db::connect(&paths.database)?;
    conn.execute(
        "INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id)
         VALUES(?1,'cv_typst','en',?2,?3)
         ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET
           path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![target_id, display_path(&paths.data_root, &typ_path), application_id],
    )?;
    conn.execute(
        "INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id)
         VALUES(?1,'cv_data','und',?2,?3)
         ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET
           path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![target_id, display_path(&paths.data_root, &data_path), application_id],
    )?;
    Ok(())
}

pub async fn generate_cv(paths: &AppPaths, target_id: &str) -> Result<CvGenerationResult> {
    let conn = db::connect(&paths.database)?;
    let (application_id, source_stored, data_stored): (String, String, String) = conn.query_row(
        "SELECT t.application_id,s.path,d.path
         FROM contact_targets_v2 t
         JOIN contact_target_artifacts s ON s.target_id=t.id AND s.artifact_type='cv_typst' AND s.language='en'
         JOIN contact_target_artifacts d ON d.target_id=t.id AND d.artifact_type='cv_data' AND d.language='und'
         WHERE t.id=?1 AND t.archived_at IS NULL",
        [target_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).context("当前联系人还没有 Typst CV 源文件")?;
    let source = resolve_data_path(&paths.data_root, &source_stored);
    let data_path = resolve_data_path(&paths.data_root, &data_stored);
    let source_dir = source.parent().context("Typst 源文件目录无效")?;
    if fs::read_to_string(&source).unwrap_or_default() != CV_TEMPLATE {
        fs::write(&source, CV_TEMPLATE).context("无法更新 CV Typst 模板")?;
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&data_path)?)
        .context("CV 结构化数据不是有效 JSON")?;
    let current_data = cv_schema::normalize(&value)
        .context("CV 结构化数据无效，请先修复 cv-data.json")?;
    let merged_data = best_existing_cv(paths, Some(target_id))
        .map(|baseline| cv_schema::merge_preserving_baseline(&baseline, &current_data))
        .unwrap_or_else(|| current_data.clone());
    let revisions_dir = source_dir.join("revisions");
    fs::create_dir_all(&revisions_dir)?;
    let old_entry_count = current_data.sections.iter().map(|section| section.entries.len()).sum::<usize>();
    let new_entry_count = merged_data.sections.iter().map(|section| section.entries.len()).sum::<usize>();
    let data_backup = if merged_data != current_data {
        let backup = revisions_dir.join(format!("{}-cv-data.json", Utc::now().format("%Y%m%dT%H%M%SZ")));
        fs::copy(&data_path, &backup)?;
        fs::write(&data_path, serde_json::to_vec_pretty(&merged_data)?)?;
        Some(backup)
    } else { None };
    let source_sha256 = format!("{:x}", Sha256::digest(fs::read(&data_path)?));
    let binary = locate_typst_binary(paths)?;
    let temporary = source_dir.join(format!("cv-{}.tmp.pdf", Uuid::new_v4().simple()));
    let output = Command::new(&binary)
        .arg("compile")
        .arg("--root")
        .arg(source_dir)
        .arg(&source)
        .arg(&temporary)
        .output()
        .await
        .with_context(|| format!("无法启动内置 Typst：{}", binary.display()))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let _ = fs::remove_file(&temporary);
        bail!("Typst 生成失败：{message}")
    }
    let document = Document::load(&temporary).context("Typst 输出不是有效 PDF")?;
    let page_count = document.get_pages().len();
    if page_count == 0 || page_count > 4 {
        let _ = fs::remove_file(&temporary);
        bail!("CV 页数异常：{page_count} 页")
    }

    let current_pdf: Option<String> = conn.query_row(
        "SELECT path FROM contact_target_artifacts
         WHERE target_id=?1 AND artifact_type='cv_pdf' AND language='en'",
        [target_id],
        |row| row.get(0),
    ).optional()?;
    let current_path = current_pdf.as_deref().map(|value| resolve_data_path(&paths.data_root, value));
    let previous_page_count = current_path.as_ref().and_then(|path| Document::load(path).ok()).map(|pdf| pdf.get_pages().len());
    let backup = if let Some(current) = current_path.as_ref().filter(|path| path.is_file()) {
        let value = revisions_dir.join(format!("{}-cv.pdf", Utc::now().format("%Y%m%dT%H%M%SZ")));
        fs::copy(current, &value)?;
        Some(value)
    } else {
        None
    };
    let live_pdf = source_dir.join("cv.pdf");
    fs::rename(&temporary, &live_pdf)?;
    let relative_pdf = display_path(&paths.data_root, &live_pdf);
    let relative_backup = backup.as_ref().map(|value| display_path(&paths.data_root, value));
    let revision_id = format!("revision-native-{}", Uuid::new_v4().simple());
    let summary = if data_backup.is_some() {
        format!("已恢复完整主 CV 基线（{old_entry_count} → {new_entry_count} 条），再由内置 Typst 生成 {page_count} 页 CV")
    } else {
        format!("由内置 Typst 生成 {page_count} 页 CV；结构化数据与目标联系人独立保存")
    };
    let mut diff = vec![DiffEntry {
        line: 1,
        before: previous_page_count.map(|value| format!("旧 PDF：{value} 页")).unwrap_or_else(|| "没有旧 PDF".into()),
        after: format!("新 Typst PDF：{page_count} 页"),
    }];
    if data_backup.is_some() {
        diff.push(DiffEntry {
            line: 1,
            before: format!("不完整的定制 CV：{old_entry_count} 条"),
            after: format!("完整主 CV + 目标定制：{new_entry_count} 条"),
        });
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO contact_target_artifacts(
            target_id,artifact_type,language,path,source_application_id
         ) VALUES(?1,'cv_pdf','en',?2,?3)
         ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET
            path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![target_id, relative_pdf, application_id],
    )?;
    if let Some(data_backup) = data_backup.as_ref() {
        let data_revision_id = format!("revision-native-{}", Uuid::new_v4().simple());
        let relative_data = display_path(&paths.data_root, &data_path);
        let relative_data_backup = display_path(&paths.data_root, data_backup);
        tx.execute(
            "INSERT INTO artifact_revisions(
                id,application_id,artifact_type,language,artifact_path,backup_path,editor,note
             ) VALUES(?1,?2,'cv_data','und',?3,?4,'generator','Restored complete master CV baseline before Typst generation')",
            params![data_revision_id, application_id, relative_data, relative_data_backup],
        )?;
        tx.execute(
            "INSERT INTO revision_contact_targets(revision_id,target_id) VALUES(?1,?2)",
            params![data_revision_id, target_id],
        )?;
        tx.execute(
            "INSERT INTO revision_change_sets(
                id,revision_id,summary,locations_json,diff_json,provider_id,model_id,reasoning
             ) VALUES(?1,?2,?3,?4,?5,'local-validator','cv-baseline-guard-v1',NULL)",
            params![
                format!("changes:{data_revision_id}"), data_revision_id,
                format!("防止 Agent 的半份定制数据覆盖完整主 CV；已恢复 {new_entry_count} 条基线内容"),
                json!(["CV 结构完整性", "完整主 CV 基线"]).to_string(),
                serde_json::to_string(&vec![DiffEntry { line: 1, before: format!("{old_entry_count} 条"), after: format!("{new_entry_count} 条") }])?,
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO artifact_revisions(
            id,application_id,artifact_type,language,artifact_path,backup_path,editor,note
         ) VALUES(?1,?2,'cv_pdf','en',?3,?4,'generator','Generated by bundled Typst 0.15.1')",
        params![revision_id, application_id, relative_pdf, relative_backup],
    )?;
    tx.execute(
        "INSERT INTO revision_contact_targets(revision_id,target_id) VALUES(?1,?2)",
        params![revision_id, target_id],
    )?;
    tx.execute(
        "INSERT INTO revision_change_sets(
            id,revision_id,summary,locations_json,diff_json,provider_id,model_id,reasoning
         ) VALUES(?1,?2,?3,?4,?5,'local-typst','typst-0.15.1',NULL)",
        params![
            format!("changes:{revision_id}"),
            revision_id,
            summary,
            json!(["PDF 页面与排版"]).to_string(),
            serde_json::to_string(&diff)?,
        ],
    )?;
    tx.execute(
        "DELETE FROM target_artifact_approvals
         WHERE target_id=?1 AND artifact_type='cv_pdf' AND language='en'",
        [target_id],
    )?;
    tx.commit()?;

    let revision = RevisionResult {
        revision_id,
        artifact_path: relative_pdf.clone(),
        backup_path: relative_backup.clone().unwrap_or_default(),
        summary,
        locations: vec!["PDF 页面与排版".into()],
        diff,
    };
    Ok(CvGenerationResult {
        pdf_path: live_pdf.display().to_string(),
        backup_path: backup.map(|value| value.display().to_string()),
        page_count,
        previous_page_count,
        source_sha256,
        font_policy: "Times New Roman; New Computer Modern; bundled Libertinus Serif fallback".into(),
        revision,
    })
}

fn best_existing_cv(paths: &AppPaths, exclude_target: Option<&str>) -> Option<CvData> {
    let conn = db::connect(&paths.database).ok()?;
    let mut statement = conn.prepare(
        "SELECT target_id,path FROM contact_target_artifacts
         WHERE artifact_type='cv_data' AND language='und' ORDER BY updated_at DESC"
    ).ok()?;
    let rows = statement.query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).ok()?;
    let mut best: Option<(usize,CvData)> = None;
    for row in rows.flatten() {
        if exclude_target.is_some_and(|target| target == row.0) { continue }
        let path = resolve_data_path(&paths.data_root, &row.1);
        let bytes = match fs::read(path) { Ok(bytes) => bytes, Err(_) => continue };
        let value: serde_json::Value = match serde_json::from_slice(&bytes) { Ok(value) => value, Err(_) => continue };
        let data = match cv_schema::normalize(&value) { Ok(data) => data, Err(_) => continue };
        let score = cv_schema::completeness_score(&data);
        if best.as_ref().is_none_or(|(current,_)| score > *current) { best = Some((score,data)); }
    }
    best.map(|(_,data)| data)
}

fn parse_curve_tex(source: &str) -> Result<CvData> {
    let capture = |pattern: &str| -> Option<String> {
        Regex::new(pattern).ok()?.captures(source)?.get(1).map(|value| latex_to_plain(value.as_str()))
    };
    let name = capture(r"\\LARGE\\bfseries\s+([^}]+)").unwrap_or_else(|| "Hongbo Miao".into());
    let tagline = capture(r"\\normalsize\s+([^}]+)").unwrap_or_else(|| "Research Curriculum Vitae".into());
    let email = Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")?
        .find(source).map(|value| value.as_str().to_owned()).unwrap_or_default();
    let phone = capture(r"\\makefield\{\\faPhone\}\{([^}]*)\}").unwrap_or_default();
    let affiliations = Regex::new(r"\\makefield\{\\faUniversity\}\{([^}]*)\}")?
        .captures_iter(source)
        .filter_map(|capture| capture.get(1).map(|value| latex_to_plain(value.as_str())))
        .collect::<Vec<_>>()
        .join(" · ");
    let rubric = Regex::new(r"(?s)\\begin\{rubric\}\{([^}]+)\}(.*?)\\end\{rubric\}")?;
    let sections = rubric.captures_iter(source).filter_map(|section| {
        let title = latex_to_plain(section.get(1)?.as_str());
        let body = section.get(2)?.as_str();
        let mut entries = body.split("\\entry*[").skip(1).filter_map(|item| {
            let (key, raw_body) = item.split_once(']')?;
            let key = latex_to_plain(key);
            let body = latex_to_plain(raw_body.trim_start().trim_start_matches('%'));
            (!body.is_empty()).then_some(CvEntry { key, body })
        }).collect::<Vec<_>>();
        if entries.is_empty() {
            let overview = latex_to_plain(body);
            if !overview.is_empty() {
                entries.push(CvEntry { key: "Overview".into(), body: overview });
            }
        }
        (!entries.is_empty()).then_some(CvSection { title, entries })
    }).collect::<Vec<_>>();
    if sections.is_empty() {
        bail!("无法从旧 CV 中提取结构化章节")
    }
    Ok(CvData {
        schema_version: 1,
        name,
        tagline,
        contact: [email, phone].into_iter().filter(|value| !value.is_empty()).collect::<Vec<_>>().join(" · "),
        affiliations,
        sections,
    })
}

fn expand_makerubrics(source: &str, source_path: &Path) -> Result<String> {
    let maker = Regex::new(r"\\makerubric\{([^}]+)\}")?;
    let mut expanded = source.to_owned();
    let legacy_parent = crate::paths::locate_legacy_root().and_then(|path| path.parent().map(Path::to_path_buf));
    for capture in maker.captures_iter(source) {
        let Some(name) = capture.get(1).map(|value| value.as_str()) else { continue };
        let filename = format!("{name}.tex");
        let candidates = [
            source_path.parent().map(|path| path.join(&filename)),
            legacy_parent.as_ref().map(|path| path.join(&filename)),
        ];
        if let Some(path) = candidates.into_iter().flatten().find(|path| path.is_file()) {
            expanded.push('\n');
            expanded.push_str(&fs::read_to_string(path)?);
        }
    }
    Ok(expanded)
}

fn latex_to_plain(value: &str) -> String {
    let mut text = value
        .replace("\\%", "%")
        .replace("\\&", "&")
        .replace("\\_", "_")
        .replace("\\enspace", " ")
        .replace("\\par", " ")
        .replace("\\newpage", " ")
        .replace("\\\\", " ")
        .replace('~', " ");
    let wrapped = Regex::new(r"\\(?:textbf|textit|emph|textsc)\{([^{}]*)\}").expect("latex wrapper regex");
    loop {
        let updated = wrapped.replace_all(&text, "$1").to_string();
        if updated == text { break }
        text = updated;
    }
    let commands = Regex::new(r"\\[A-Za-z@]+(?:\[[^\]]*\])?").expect("latex command regex");
    text = commands.replace_all(&text, " ").to_string();
    text = text.replace(['{', '}'], " ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
    fn curve_tex_is_structured_without_latex_commands() -> Result<()> {
        let source = r"\leftheader{{\LARGE\bfseries Hongbo Miao}\par {\normalsize Marine AI}\par \makefield{\faEnvelope[regular]}{urbinohbmiao@gmail.com} \makefield{\faPhone}{123} \makefield{\faUniversity}{The University of Hong Kong}} \begin{rubric}{Research Profile}\entry*[Focus]% \textbf{Marine sensing:} Verified work.\end{rubric}";
        let data = parse_curve_tex(source)?;
        assert_eq!(data.name, "Hongbo Miao");
        assert_eq!(data.sections[0].entries[0].key, "Focus");
        assert!(!data.sections[0].entries[0].body.contains("textbf"));
        Ok(())
    }

    #[test]
    fn bundled_typst_compiles_a_real_imported_cv() -> Result<()> {
        let Some(root) = crate::paths::locate_legacy_root() else { return Ok(()) };
        let source = root.join("generated/applications/app-cityu-junwei-young-scholars-2027/cv.tex");
        if !source.is_file() { return Ok(()) }
        let data = parse_curve_tex(&fs::read_to_string(source)?)?;
        let temp = tempfile::tempdir()?;
        fs::write(temp.path().join("cv.typ"), CV_TEMPLATE)?;
        fs::write(temp.path().join("cv-data.json"), serde_json::to_vec_pretty(&data)?)?;
        let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/typst");
        let output = std::process::Command::new(binary)
            .arg("compile")
            .arg("--root")
            .arg(temp.path())
            .arg(temp.path().join("cv.typ"))
            .arg(temp.path().join("cv.pdf"))
            .output()?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr))
        }
        let pdf = Document::load(temp.path().join("cv.pdf"))?;
        assert!((1..=4).contains(&pdf.get_pages().len()));
        assert!(data.sections.len() >= 5);
        Ok(())
    }
}
