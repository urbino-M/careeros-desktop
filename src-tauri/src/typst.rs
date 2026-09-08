use crate::db;
use crate::cv_schema::{self, CvData, CvEntry, CvSection};
use crate::materials::{DiffEntry, RevisionResult};
use crate::paths::{AppPaths, runtime_binary};
use anyhow::{bail, Context, Result};
use lopdf::Document;
use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

const CV_TEMPLATE: &str = include_str!("../resources/templates/cv.typ");
const A4_HEIGHT_PT: f64 = 841.89;
const PAGE_MARGIN_Y_PT: f64 = 25.51;
const MIN_FINAL_PAGE_FILL_RATIO: f64 = 0.42;
const PREFERRED_MAX_ENTRY_KEY_CHARACTERS: usize = 18;

#[derive(Debug)]
pub(crate) struct CvLayoutError(pub String);
impl std::fmt::Display for CvLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for CvLayoutError {}

pub(crate) fn layout_policy_contract() -> Value {
    json!({
        "pageCount": {
            "source": "cvCustomization.pageCount",
            "default": "auto",
            "modes": ["auto", "fixed"],
            "fixedRange": [1, 20]
        },
        "preferredMaximumEntryKeyCharacters": PREFERRED_MAX_ENTRY_KEY_CHARACTERS,
        "finalPageMinimumUsableHeightPercent": (MIN_FINAL_PAGE_FILL_RATIO * 100.0) as usize,
        "template": "bundled cv.typ",
        "forbidReducedTypography": true
    })
}

pub(crate) fn layout_generation_rules() -> Vec<String> {
    vec![
        format!(
            "Read cvCustomization.pageCount. In auto mode, choose the cleanest readable number of pages. In fixed mode, render exactly the requested number of A4 pages. Keep the final page meaningfully occupied (at least about {:.0}% of usable height when there is more than one page), without padding or reducing typography. Section and entry counts are not locked; follow user-requested changes with source-backed content.",
            MIN_FINAL_PAGE_FILL_RATIO * 100.0,
        ),
        format!(
            "Keep each entry key short, preferably {} characters or fewer; keep bodies readable and adapt source-backed content to the target.",
            PREFERRED_MAX_ENTRY_KEY_CHARACTERS,
        ),
        "Use the bundled compact CV template. Never fill space with repeated claims, generic padding, invented facts, oversized spacing, or unreadably compressed text.".into(),
    ]
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum CvLayoutMarker {
    ContentEnd { page: usize, y: f64 },
    Entry { page: usize, characters: usize },
}

#[derive(Debug)]
struct CvLayoutMetrics {
    content_end_page: usize,
    content_end_y: f64,
    entry_characters_by_page: BTreeMap<usize, usize>,
}

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

/// Compile the exact target-specific selection before it can enter the contact
/// pipeline. This keeps page-limit failures from creating a ready-to-contact
/// record and proves the same normalized data that will later be installed.
pub async fn validate_cv_data(paths: &AppPaths, value: &serde_json::Value) -> Result<usize> {
    let directory = paths.cache.join("cv-validation").join(Uuid::new_v4().simple().to_string());
    let result = render_cv_package(paths, value, &directory).await;
    let _ = fs::remove_dir_all(&directory);
    result
}

/// Render once into a new immutable package. No database pointers change here.
pub(crate) async fn render_cv_package(paths: &AppPaths, value: &serde_json::Value, directory: &Path) -> Result<usize> {
    let data = cv_schema::normalize(value)
        .context("Agent 返回的 CV 结构化数据不符合模板")?;
    let requested_page_count = {
        let _guard = crate::onboarding::profile_guard()?;
        crate::onboarding::recover_profile_import(paths)?;
        validate_cv_policy(paths, &data)?;
        requested_cv_page_count(paths)?
    };
    fs::create_dir_all(&directory)?;
    async {
        let source = directory.join("cv.typ");
        let data_path = directory.join("cv-data.json");
        let pdf_path = directory.join("cv.pdf");
        fs::write(&source, CV_TEMPLATE)?;
        fs::write(&data_path, serde_json::to_vec_pretty(&data)?)?;
        let binary = locate_typst_binary(paths)?;
        let output = Command::new(&binary)
            .kill_on_drop(true)
            .arg("compile")
            .arg("--root")
            .arg(&directory)
            .arg(&source)
            .arg(&pdf_path)
            .output()
            .await
            .with_context(|| format!("无法启动内置 Typst：{}", binary.display()))?;
        if !output.status.success() {
            bail!("Typst 预检失败：{}", String::from_utf8_lossy(&output.stderr).trim())
        }
        let page_count = inspect_rendered_cv(
            &binary,
            &directory,
            &source,
            &pdf_path,
            requested_page_count,
        ).await
            .context("Typst CV 版面质量预检失败")?;
        Ok(page_count)
    }.await
}

pub async fn generate_cv(paths: &AppPaths, target_id: &str) -> Result<CvGenerationResult> {
    let prepared = prepare_cv(paths, paths, target_id, None).await?;
    let conn = db::connect(&paths.database)?;
    let tx = db::publication_transaction(&conn)?;
    let result = publish_cv(&tx, paths, &prepared)?;
    crate::workflows::reconcile_material_state(&tx,paths,target_id,None)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) struct PreparedCv {
    pub directory: PathBuf,
    target_id: String,
    application_id: String,
    fingerprint: String,
    page_count: usize,
    previous_page_count: Option<usize>,
    backup: Option<PathBuf>,
    data_backup: PathBuf,
}

/// Stage the entire package without touching any current artifact or approval.
pub(crate) async fn prepare_cv(
    paths: &AppPaths, policy: &AppPaths, target_id: &str, replacement: Option<&Value>,
) -> Result<PreparedCv> {
    let conn = db::connect(&paths.database)?;
    let (application_id, stored): (String,String) = conn.query_row(
        "SELECT t.application_id,a.path FROM contact_targets_v2 t JOIN contact_target_artifacts a ON a.target_id=t.id
         WHERE t.id=?1 AND t.archived_at IS NULL AND a.artifact_type='cv_data' AND a.language='und'",
        [target_id], |r| Ok((r.get(0)?,r.get(1)?)),
    ).context("当前联系人没有 CV 结构化源数据")?;
    let fingerprint = crate::materials::artifact_fingerprint(&conn, paths, target_id, "cv_%")?;
    let original = fs::read(resolve_data_path(&paths.data_root, &stored))?;
    let value = match replacement { Some(value) => value.clone(), None => serde_json::from_slice(&original)? };
    let directory = crate::materials::new_material_version(paths, target_id);
    fs::create_dir_all(&directory)?;
    let mut frozen_policy = policy.clone();
    if !policy.profile.join(".snapshot-complete").is_file() {
        // Local validation needs only JSON policy/source facts, not another copy of the uploaded PDF.
        let _guard = crate::onboarding::profile_guard()?;
        crate::onboarding::recover_profile_import(policy)?;
        frozen_policy.profile = directory.join("policy");
        fs::create_dir_all(&frozen_policy.profile)?;
        for name in ["master_profile.json","cv_customization.json"] {
            let source=policy.profile.join(name);
            if source.is_file() { fs::copy(source,frozen_policy.profile.join(name))?; }
        }
    }
    let data_backup = directory.join("previous-cv-data.json");
    fs::write(&data_backup, original)?;
    let previous: Option<String> = conn.query_row(
        "SELECT path FROM contact_target_artifacts WHERE target_id=?1 AND artifact_type='cv_pdf' AND language='en'",
        [target_id], |r| r.get(0),
    ).optional()?;
    let backup = if let Some(source) = previous.map(|p| resolve_data_path(&paths.data_root,&p)).filter(|p| p.is_file()) {
        let backup = directory.join("previous-cv.pdf");
        fs::copy(source,&backup)?;
        Some(backup)
    } else { None };
    let previous_page_count = backup.as_ref().and_then(|p| Document::load(p).ok()).map(|pdf| pdf.get_pages().len());
    drop(conn);
    let page_count = render_cv_package(&frozen_policy, &value, &directory).await
        .context("CV 修改未通过完整预检；当前 CV 数据、PDF 和审核状态均未改动")?;
    Ok(PreparedCv { directory,target_id:target_id.into(),application_id,fingerprint,page_count,previous_page_count,backup,data_backup })
}

pub(crate) fn publish_cv(conn: &rusqlite::Connection, paths: &AppPaths, prepared: &PreparedCv) -> Result<CvGenerationResult> {
    crate::scheduler::ensure_can_publish(conn)?;
    if crate::materials::artifact_fingerprint(conn, paths, &prepared.target_id, "cv_%")? != prepared.fingerprint {
        bail!("CV 在生成期间已改变；新候选版本已保留，未替换当前材料")
    }
    let active: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL)", [&prepared.target_id], |r| r.get(0))?;
    if !active { bail!("联系人已归档；新 CV 未发布") }
    for (kind,language,name) in [("cv_pdf","en","cv.pdf"),("cv_data","und","cv-data.json"),("cv_typst","en","cv.typ")] {
        conn.execute(
            "INSERT INTO contact_target_artifacts(target_id,artifact_type,language,path,source_application_id) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET path=excluded.path,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![prepared.target_id,kind,language,display_path(&paths.data_root,&prepared.directory.join(name)),prepared.application_id],
        )?;
    }
    let revision_id = format!("revision-native-{}",Uuid::new_v4().simple());
    let pdf_path = prepared.directory.join("cv.pdf");
    let relative_pdf = display_path(&paths.data_root,&pdf_path);
    let relative_backup = prepared.backup.as_ref().map(|p| display_path(&paths.data_root,p));
    let summary = format!("内置 Typst 已校验并发布 {} 页 CV；源数据、模板与 PDF 同版保存",prepared.page_count);
    let diff = vec![DiffEntry { line:1,before:prepared.previous_page_count.map(|v|format!("{v} 页")).unwrap_or("没有 PDF".into()),after:format!("{} 页",prepared.page_count) }];
    for (id,kind,language,path,backup) in [
        (revision_id.clone(),"cv_pdf","en",relative_pdf.clone(),relative_backup.clone()),
        (format!("revision-native-{}",Uuid::new_v4().simple()),"cv_data","und",display_path(&paths.data_root,&prepared.directory.join("cv-data.json")),Some(display_path(&paths.data_root,&prepared.data_backup))),
    ] {
        conn.execute(
            "INSERT INTO artifact_revisions(id,application_id,artifact_type,language,artifact_path,backup_path,editor,note) VALUES(?1,?2,?3,?4,?5,?6,'generator','Validated immutable CV package')",
            params![id,prepared.application_id,kind,language,path,backup],
        )?;
        conn.execute("INSERT INTO revision_contact_targets(revision_id,target_id) VALUES(?1,?2)",params![id,prepared.target_id])?;
    }
    conn.execute(
        "INSERT INTO revision_change_sets(id,revision_id,summary,locations_json,diff_json,provider_id,model_id,reasoning) VALUES(?1,?2,?3,?4,?5,'local-typst','typst',NULL)",
        params![format!("changes:{revision_id}"),revision_id,summary,json!(["PDF 页面与排版"]).to_string(),serde_json::to_string(&diff)?],
    )?;
    conn.execute("DELETE FROM target_artifact_approvals WHERE target_id=?1 AND artifact_type='cv_pdf' AND language='en'",[&prepared.target_id])?;
    Ok(CvGenerationResult {
        pdf_path:pdf_path.display().to_string(),backup_path:prepared.backup.as_ref().map(|p|p.display().to_string()),
        page_count:prepared.page_count,previous_page_count:prepared.previous_page_count,
        source_sha256:format!("{:x}",Sha256::digest(fs::read(prepared.directory.join("cv-data.json"))?)),
        font_policy:"Times New Roman; New Computer Modern; bundled Libertinus Serif fallback".into(),
        revision:RevisionResult { revision_id,artifact_path:relative_pdf,backup_path:relative_backup.unwrap_or_default(),summary,locations:vec!["PDF 页面与排版".into()],diff },
    })
}

fn requested_cv_page_count(paths: &AppPaths) -> Result<Option<usize>> {
    crate::materials::load_cv_customization(paths)?.page_count.requested_page_count()
}

fn ensure_cv_page_count(page_count: usize, requested_page_count: Option<usize>) -> Result<()> {
    if page_count == 0 {
        bail!("CV 没有生成可用页面")
    }
    if let Some(requested_page_count) = requested_page_count {
        if page_count != requested_page_count {
            bail!(
                "CV 设置要求正好为 {requested_page_count} 页，当前为 {page_count} 页；请按用户要求重新组织有来源的内容与分页后重试，不要凑数"
            )
        }
    }
    Ok(())
}

fn validate_cv_policy(paths: &AppPaths, data: &CvData) -> Result<()> {
    let profile_path = paths.profile.join("master_profile.json");
    let profile: serde_json::Value = if profile_path.is_file() {
        serde_json::from_slice(&fs::read(&profile_path)?).context("CV 事实主档案不是有效 JSON")?
    } else {
        serde_json::json!({})
    };
    cv_schema::validate_generation_policy(data, &profile)
        .context("CV 未通过底层格式与内容规则")
}

async fn inspect_rendered_cv(
    binary: &Path,
    root: &Path,
    source: &Path,
    pdf_path: &Path,
    requested_page_count: Option<usize>,
) -> Result<usize> {
    let document = Document::load(pdf_path).context("Typst 输出不是有效 PDF")?;
    let page_count = document.get_pages().len();
    ensure_cv_page_count(page_count, requested_page_count)
        .map_err(|error| CvLayoutError(error.to_string()))?;
    let layout = query_cv_layout(binary, root, source).await?;
    ensure_cv_layout_quality(&layout, page_count)
        .map_err(|error| CvLayoutError(error.to_string()))?;
    Ok(page_count)
}

async fn query_cv_layout(binary: &Path, root: &Path, source: &Path) -> Result<CvLayoutMetrics> {
    let output = Command::new(binary)
        .kill_on_drop(true)
        .arg("query")
        .arg("--root")
        .arg(root)
        .arg(source)
        .arg("metadata")
        .arg("--field")
        .arg("value")
        .output()
        .await
        .with_context(|| format!("无法查询 Typst CV 内容边界：{}", binary.display()))?;
    if !output.status.success() {
        bail!("Typst 无法报告 CV 内容边界：{}", String::from_utf8_lossy(&output.stderr).trim())
    }
    let markers: Vec<CvLayoutMarker> = serde_json::from_slice(&output.stdout)
        .context("Typst 返回的 CV 版面标记无效")?;
    let mut content_end = None;
    let mut entry_characters_by_page = BTreeMap::new();
    for marker in markers {
        match marker {
            CvLayoutMarker::ContentEnd { page, y } => content_end = Some((page, y)),
            CvLayoutMarker::Entry { page, characters } => {
                *entry_characters_by_page.entry(page).or_insert(0) += characters;
            }
        }
    }
    let (content_end_page, content_end_y) = content_end.context("Typst CV 缺少内容结束标记")?;
    Ok(CvLayoutMetrics {
        content_end_page,
        content_end_y,
        entry_characters_by_page,
    })
}

fn ensure_cv_layout_quality(layout: &CvLayoutMetrics, page_count: usize) -> Result<()> {
    ensure_cv_layout_metrics(
        layout.content_end_page,
        layout.content_end_y,
        &layout.entry_characters_by_page,
        page_count,
    )
}

fn ensure_cv_layout_metrics(
    content_end_page: usize,
    content_end_y: f64,
    entry_characters_by_page: &BTreeMap<usize, usize>,
    page_count: usize,
) -> Result<()> {
    if content_end_page != page_count {
        bail!("CV 内容结束标记不在最后一页，当前在第 {content_end_page} 页、PDF 共 {page_count} 页")
    }
    for page in 1..=page_count {
        if entry_characters_by_page.get(&page).copied().unwrap_or_default() == 0 {
            bail!("CV 第 {page} 页没有可校验的大条目内容")
        }
    }
    if page_count == 1 {
        return Ok(())
    }
    let usable_height = A4_HEIGHT_PT - 2.0 * PAGE_MARGIN_Y_PT;
    let used_height = (content_end_y - PAGE_MARGIN_Y_PT).clamp(0.0, usable_height);
    let fill_ratio = used_height / usable_height;
    if fill_ratio < MIN_FINAL_PAGE_FILL_RATIO {
        bail!(
            "CV 最后一页填充不足：共 {page_count} 页，第 {content_end_page} 页实际占用约 {:.0}%，要求至少 {:.0}%；请重新组织有依据的内容与分页，遵守用户指定页数与本次修改要求，不要凑字数",
            fill_ratio * 100.0,
            MIN_FINAL_PAGE_FILL_RATIO * 100.0,
        )
    }
    Ok(())
}

fn parse_curve_tex(source: &str) -> Result<CvData> {
    let capture = |pattern: &str| -> Option<String> {
        Regex::new(pattern).ok()?.captures(source)?.get(1).map(|value| latex_to_plain(value.as_str()))
    };
    let name = capture(r"\\LARGE\\bfseries\s+([^}]+)").unwrap_or_else(|| "Candidate".into());
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
        author_name: String::new(),
        tagline,
        contact: [email, phone].into_iter().filter(|value| !value.is_empty()).collect::<Vec<_>>().join(" · "),
        affiliations,
        sections,
    })
}

fn expand_makerubrics(source: &str, source_path: &Path) -> Result<String> {
    let maker = Regex::new(r"\\makerubric\{([^}]+)\}")?;
    let mut expanded = source.to_owned();
    for capture in maker.captures_iter(source) {
        let Some(name) = capture.get(1).map(|value| value.as_str()) else { continue };
        let filename = format!("{name}.tex");
        if let Some(path) = source_path.parent().map(|path| path.join(&filename)).filter(|path| path.is_file()) {
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
    if let Some(value) = std::env::var_os("CAREEROS_TYPST_BIN") {
        let path = PathBuf::from(value);
        if path.is_file() { return Ok(path) }
    }
    for path in [
        runtime_binary(&paths.runtime, "typst"),
        runtime_binary(&paths.data_root.join("runtime"), "typst"),
    ] {
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
        let source = r"\leftheader{{\LARGE\bfseries Alex Morgan}\par {\normalsize Targeted Profile}\par \makefield{\faEnvelope[regular]}{candidate@example.org} \makefield{\faPhone}{123} \makefield{\faUniversity}{Example Institute}} \begin{rubric}{Research Profile}\entry*[Focus]% \textbf{Research focus:} Verified work.\end{rubric}";
        let data = parse_curve_tex(source)?;
        assert_eq!(data.name, "Alex Morgan");
        assert_eq!(data.sections[0].entries[0].key, "Focus");
        assert!(!data.sections[0].entries[0].body.contains("textbf"));
        Ok(())
    }

    #[tokio::test]
    async fn preflight_respects_a_fixed_page_count() -> Result<()> {
        let temp = tempfile::tempdir()?;
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
            runtime: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime"),
            data_root: root,
        };
        let entries = (0..120).map(|index| json!({
            "key": format!("Item {index}"),
            "body": format!("Distinct verified research evidence number {index} with enough explanatory detail to consume a visible line in the rendered curriculum vitae."),
        })).collect::<Vec<_>>();
        fs::create_dir_all(&paths.profile)?;
        fs::write(paths.profile.join("master_profile.json"), serde_json::to_vec_pretty(&json!({
            "projects":[{"title":"Verified project"}],
            "referees":[
                {"name":"Prof. Ada One","role":"Professor","institution":"University One","email":"ada.one@example.org","claim_status":"usable"},
                {"name":"Dr. Ben Two","role":"Associate Professor","institution":"University Two","email":"ben.two@example.org","claim_status":"verified"},
                {"name":"Prof. Cy Three","role":"Professor","institution":"University Three","email":"cy.three@example.org","claim_status":"approved"}
            ]
        }))?)?;
        fs::write(paths.profile.join("cv_customization.json"), serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1,
            "pageCount": {"mode": "fixed", "value": 2}
        }))?)?;
        let value = json!({
            "schemaVersion":1,
            "name":"Alex Morgan",
            "authorName":"Morgan, A.",
            "tagline":"Target-specific research CV",
            "contact":"verified@example.org",
            "affiliations":"Example Institute",
            "sections":[
                {"title":"Selected Research Projects","entries":entries},
                {"title":"References","entries":[
                    {"key":"Prof. Ada One","body":"Prof. Ada One, Professor, University One, ada.one@example.org"},
                    {"key":"Dr. Ben Two","body":"Dr. Ben Two, Associate Professor, University Two, ben.two@example.org"},
                    {"key":"Prof. Cy Three","body":"Prof. Cy Three, Professor, University Three, cy.three@example.org"}
                ]}
            ]
        });
        let error = validate_cv_data(&paths, &value).await.unwrap_err();
        assert!(format!("{error:#}").contains("设置要求正好为 2 页"));
        Ok(())
    }

    #[test]
    fn page_count_guards_allow_automatic_or_enforce_a_fixed_value() -> Result<()> {
        assert!(ensure_cv_page_count(1, None).is_ok());
        assert!(ensure_cv_page_count(2, None).is_ok());
        assert!(ensure_cv_page_count(3, None).is_ok());
        assert!(ensure_cv_page_count(2, Some(2)).is_ok());
        assert!(ensure_cv_page_count(1, Some(2)).is_err());
        assert!(ensure_cv_page_count(3, Some(2)).is_err());
        Ok(())
    }

    #[test]
    fn rendered_layout_guards_reject_sparse_or_empty_final_pages() -> Result<()> {
        let occupied = BTreeMap::from([(1, 3_000), (2, 2_200)]);
        assert!(ensure_cv_layout_metrics(2, 700.0, &occupied, 2).is_ok());
        let sparse = ensure_cv_layout_metrics(2, 300.0, &occupied, 2).unwrap_err();
        assert!(format!("{sparse:#}").contains("最后一页填充不足"));
        let empty = BTreeMap::from([(1, 3_000)]);
        let error = ensure_cv_layout_metrics(2, 700.0, &empty, 2).unwrap_err();
        assert!(format!("{error:#}").contains("第 2 页没有可校验"));
        Ok(())
    }

    #[tokio::test]
    async fn bundled_template_reports_content_end_position() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("cv.typ");
        fs::write(&source, CV_TEMPLATE)?;
        fs::write(temp.path().join("cv-data.json"), serde_json::to_vec_pretty(&json!({
            "schemaVersion":1,
            "name":"Alex Morgan",
            "authorName":"Morgan, A.",
            "tagline":"Targeted research CV",
            "contact":"verified@example.org",
            "affiliations":"Example Institute",
            "sections":[{"title":"Selected Research Projects","entries":[
                {"key":"Project","body":"Specific verified research project with methods, contribution, and outcome."}
            ]}]
        }))?)?;
        let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/typst");
        let layout = query_cv_layout(&binary, temp.path(), &source).await?;
        assert_eq!(layout.content_end_page, 1);
        assert!(layout.content_end_y > PAGE_MARGIN_Y_PT);
        assert!(layout.entry_characters_by_page.get(&1).is_some_and(|value| *value > 0));
        Ok(())
    }

    #[test]
    fn bundled_template_styles_aliases_without_forcing_titles_or_pagebreaks() {
        assert!(CV_TEMPLATE.contains("title.contains(\"education\")"));
        assert!(CV_TEMPLATE.contains("title.contains(\"research experience\")"));
        assert!(CV_TEMPLATE.contains("title.contains(\"publication\")"));
        assert!(!CV_TEMPLATE.contains("#pagebreak()"));
    }

    #[tokio::test]
    async fn preflight_accepts_a_well_balanced_two_page_cv() -> Result<()> {
        let temp = tempfile::tempdir()?;
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
            runtime: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime"),
            data_root: root,
        };
        fs::create_dir_all(&paths.profile)?;
        fs::write(paths.profile.join("master_profile.json"), serde_json::to_vec_pretty(&json!({
            "publications": [{"title": "Publication"}],
            "patents": [{"title": "Patent"}],
            "projects": [{"title": "Project"}],
            "education": [{"degree": "PhD"}],
            "skills": {"methods": ["Method"]},
            "honors_and_service": {"honors": ["Honor"]},
            "referees": [
                {"name": "Prof. Ada One", "role": "Professor", "institution": "University One", "email": "ada.one@example.org", "claim_status": "usable"},
                {"name": "Dr. Ben Two", "role": "Associate Professor", "institution": "University Two", "email": "ben.two@example.org", "claim_status": "verified"},
                {"name": "Prof. Cy Three", "role": "Professor", "institution": "University Three", "email": "cy.three@example.org", "claim_status": "approved"}
            ]
        }))?)?;
        let entries = |prefix: &str, count: usize| {
            (0..count).map(|index| json!({
                "key": format!("{prefix} {index}"),
                "body": format!(
                    "Distinct verified {prefix} evidence {index} with specific experimental methods, documented contribution, reproducible outcome, and target-relevant research value."
                )
            })).collect::<Vec<_>>()
        };
        let detailed_entries = |prefix: &str, count: usize| {
            (0..count).map(|index| json!({
                "key": format!("{prefix} {index}"),
                "body": format!(
                    "Distinct verified {prefix} evidence {index} with specific experimental methods, documented contribution, reproducible outcome, and target-relevant research value. The record identifies the setting, responsibility, evidence source, and practical impact for the target role."
                )
            })).collect::<Vec<_>>()
        };
        let value = json!({
            "schemaVersion": 1,
            "name": "Alex Morgan",
            "authorName": "Morgan, A.",
            "tagline": "Targeted research CV",
            "contact": "candidate@example.org",
            "affiliations": "Example Institute",
            "sections": [
                {"title": "Selected Research Outputs", "entries": entries("output", 4)},
                {"title": "Selected Patents", "entries": entries("patent", 1)},
                {"title": "Selected Research Projects", "entries": entries("project", 6)},
                {"title": "Education & Current Stage", "entries": entries("education", 2)},
                {"title": "Technical Capabilities", "entries": entries("capability", 4)},
                {"title": "Honors, Teaching & Service", "entries": detailed_entries("service", 16)},
                {"title": "References", "entries": [
                    {"key": "Prof. Ada One", "body": "Prof. Ada One, Professor, University One, ada.one@example.org"},
                    {"key": "Dr. Ben Two", "body": "Dr. Ben Two, Associate Professor, University Two, ben.two@example.org"},
                    {"key": "Prof. Cy Three", "body": "Prof. Cy Three, Professor, University Three, cy.three@example.org"}
                ]}
            ]
        });
        assert_eq!(validate_cv_data(&paths, &value).await?, 2);
        Ok(())
    }

    #[tokio::test]
    async fn preflight_rejects_a_multi_page_cv_with_a_sparse_final_page() -> Result<()> {
        let temp = tempfile::tempdir()?;
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
            runtime: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime"),
            data_root: root,
        };
        fs::create_dir_all(&paths.profile)?;
        fs::write(paths.profile.join("master_profile.json"), serde_json::to_vec_pretty(&json!({
            "publications":[{"title":"Publication"}],
            "patents":[{"title":"Patent"}],
            "projects":[{"title":"Project"}],
            "education":[{"degree":"PhD"}],
            "skills":{"methods":["Method"]},
            "honors_and_service":{"honors":["Honor"]},
            "referees":[
                {"name":"Prof. Ada One","role":"Professor","institution":"University One","email":"ada.one@example.org","claim_status":"usable"},
                {"name":"Dr. Ben Two","role":"Associate Professor","institution":"University Two","email":"ben.two@example.org","claim_status":"verified"},
                {"name":"Prof. Cy Three","role":"Professor","institution":"University Three","email":"cy.three@example.org","claim_status":"approved"}
            ]
        }))?)?;
        let entries = |prefix: &str, count: usize| {
            (0..count).map(|index| json!({
                "key": format!("{prefix} {index}"),
                "body": format!("Verified {prefix} evidence {index} with specific acoustic methods, documented contribution, and target-relevant outcome.")
            })).collect::<Vec<_>>()
        };
        let value = json!({
            "schemaVersion":1,
            "name":"Alex Morgan",
            "authorName":"Morgan, A.",
            "tagline":"Target-specific research CV",
            "contact":"verified@example.org",
            "affiliations":"Example Institute",
            "sections":[
                {"title":"Selected Research Outputs","entries":entries("publication", 6)},
                {"title":"Selected Patents","entries":entries("patent", 4)},
                {"title":"Selected Research Projects","entries":entries("project", 3)},
                {"title":"Education & Current Stage","entries":entries("education", 4)},
                {"title":"Technical Capabilities","entries":entries("capability", 5)},
                {"title":"Honors, Teaching & Service","entries":entries("honor", 1)},
                {"title":"References","entries":[
                    {"key":"Prof. Ada One","body":"Prof. Ada One, Professor, University One, ada.one@example.org"},
                    {"key":"Dr. Ben Two","body":"Dr. Ben Two, Associate Professor, University Two, ben.two@example.org"},
                    {"key":"Prof. Cy Three","body":"Prof. Cy Three, Professor, University Three, cy.three@example.org"}
                ]}
            ]
        });
        let error = validate_cv_data(&paths, &value).await.unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("最后一页填充不足"), "{message}");
        Ok(())
    }
}
