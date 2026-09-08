use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use sha2::{Digest, Sha256};
use serde_json::json;

pub const ONBOARDING_PROFILE_FILE: &str = "onboarding_profile.json";
const MAX_TEXT_CHARS: usize = 4_000;
const MAX_SHORT_CHARS: usize = 200;
const MAX_CV_BYTES: u64 = 25 * 1024 * 1024;
static PROFILE_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn profile_guard() -> Result<std::sync::MutexGuard<'static, ()>> {
    PROFILE_WRITE_LOCK.lock().map_err(|_| anyhow::anyhow!("用户资料正在恢复，请重新打开应用"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingProfile {
    #[serde(default = "schema_version")]
    pub schema_version: u8,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub current_step: u8,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub publication_name: String,
    #[serde(default = "not_specified")]
    pub career_stage: String,
    #[serde(default = "not_specified")]
    pub discipline: String,
    #[serde(default)]
    pub current_situation: String,
    #[serde(default)]
    pub target_roles: String,
    #[serde(default)]
    pub target_regions: String,
    #[serde(default)]
    pub goals: String,
    #[serde(default)]
    pub constraints: String,
    #[serde(default = "default_language")]
    pub preferred_language: String,
    #[serde(default)]
    pub cv_source_file: Option<String>,
    pub updated_at: Option<String>,
}

impl Default for OnboardingProfile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            completed: false,
            current_step: 0,
            full_name: String::new(),
            publication_name: String::new(),
            career_stage: not_specified(),
            discipline: not_specified(),
            current_situation: String::new(),
            target_roles: String::new(),
            target_regions: String::new(),
            goals: String::new(),
            constraints: String::new(),
            preferred_language: default_language(),
            cv_source_file: None,
            updated_at: None,
        }
    }
}

fn schema_version() -> u8 { 1 }
fn not_specified() -> String { "not_specified".into() }
fn default_language() -> String { "bilingual".into() }

pub fn load(paths: &AppPaths) -> Result<OnboardingProfile> {
    if paths.profile.join(".cv-import-journal.json").is_file() {
        let _guard = profile_guard()?;
        recover_profile_import(paths)?;
    }
    let path = paths.profile.join(ONBOARDING_PROFILE_FILE);
    if !path.is_file() {
        return Ok(OnboardingProfile::default())
    }
    let value: OnboardingProfile = serde_json::from_slice(&fs::read(&path)?)
        .with_context(|| format!("开始使用资料无法读取：{}", path.display()))?;
    if value.schema_version != 1 {
        bail!("不支持的开始使用资料版本：{}", value.schema_version)
    }
    Ok(value)
}

pub fn save(paths: &AppPaths, value: OnboardingProfile) -> Result<OnboardingProfile> {
    let _guard = profile_guard()?;
    recover_profile_import(paths)?;
    save_unlocked(paths, value)
}

fn normalize_profile(mut value: OnboardingProfile) -> Result<OnboardingProfile> {
    value.schema_version = 1;
    value.current_step = value.current_step.min(5);
    validate_choice("当前阶段", &value.career_stage, &[
        "not_specified", "undergraduate", "masters", "doctoral", "postdoctoral",
        "research_staff", "faculty", "industry", "career_transition", "other",
    ])?;
    validate_choice("学科领域", &value.discipline, &[
        "not_specified", "humanities_arts", "social_sciences", "natural_sciences",
        "engineering_technology", "medical_life_sciences", "interdisciplinary", "other",
    ])?;
    validate_choice("首选语言", &value.preferred_language, &["zh", "en", "bilingual"])?;
    for (label, field, limit) in [
        ("姓名", &mut value.full_name, MAX_SHORT_CHARS),
        ("论文署名", &mut value.publication_name, MAX_SHORT_CHARS),
        ("当前情况", &mut value.current_situation, MAX_TEXT_CHARS),
        ("目标岗位", &mut value.target_roles, MAX_TEXT_CHARS),
        ("目标地区", &mut value.target_regions, MAX_TEXT_CHARS),
        ("主要目标", &mut value.goals, MAX_TEXT_CHARS),
        ("现实约束", &mut value.constraints, MAX_TEXT_CHARS),
    ] {
        *field = field.trim().to_owned();
        if field.chars().count() > limit {
            bail!("{label}最多允许 {limit} 个字符")
        }
    }
    value.cv_source_file = value.cv_source_file
        .take()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty());
    if let Some(relative) = value.cv_source_file.as_deref() { validate_source_relative(relative)?; }
    value.updated_at = Some(Utc::now().to_rfc3339());
    Ok(value)
}

fn save_unlocked(paths: &AppPaths, value: OnboardingProfile) -> Result<OnboardingProfile> {
    let value = normalize_profile(value)?;
    fs::create_dir_all(&paths.profile)?;
    let live = paths.profile.join(ONBOARDING_PROFILE_FILE);
    backup_existing(paths, &live, ONBOARDING_PROFILE_FILE)?;
    let temporary = paths.profile.join(format!(".{ONBOARDING_PROFILE_FILE}.tmp"));
    fs::write(&temporary, serde_json::to_vec_pretty(&value)?)?;
    fs::rename(&temporary, &live)?;
    Ok(value)
}

pub fn import_cv(paths: &AppPaths, source: &Path) -> Result<String> {
    let source = source.canonicalize()
        .with_context(|| format!("无法读取 CV 文件：{}", source.display()))?;
    if !source.is_file() {
        bail!("请选择一个 CV 文件")
    }
    let extension = source.extension().and_then(|item| item.to_str())
        .map(str::to_lowercase).context("CV 文件缺少扩展名")?;
    if !matches!(extension.as_str(), "pdf" | "docx" | "md" | "txt") {
        bail!("CV 仅支持 PDF、DOCX、Markdown 或纯文本")
    }
    if source.metadata()?.len() > MAX_CV_BYTES {
        bail!("CV 文件不能超过 25 MB")
    }
    let uploads = paths.profile.join("uploads");
    fs::create_dir_all(&uploads)?;
    let filename = format!("source-cv-{}.{}", uuid::Uuid::new_v4().simple(), extension);
    let destination = uploads.join(&filename);
    fs::copy(&source, &destination)
        .with_context(|| format!("无法导入 CV：{}", source.display()))?;
    let relative = format!("uploads/{filename}");
    let extracted = match extension.as_str() {
        "pdf" => extract_pdf(paths, &destination)?,
        "md" | "txt" => fs::read_to_string(&destination).context("CV 文本不是 UTF-8")?,
        _ => String::new(), // DOCX stays available as an original document for the agent.
    };
    if extension == "pdf" && extracted.trim().chars().count() < 30 {
        bail!("未能读取 PDF 文字。原文件已保留，请选择可复制文字的 PDF 或 TXT；不会生成虚构资料。")
    }
    let text_file = format!("{relative}.txt");
    fs::write(paths.profile.join(&text_file), &extracted)?;
    let digest = format!("{:x}", Sha256::digest(fs::read(&destination)?));
    let profile = json!({
        "schemaVersion": 1,
        "source": {"file": relative, "textFile": text_file, "sha256": digest, "provenance": "user_provided"},
        "sourceText": extracted,
        "policy": "Extract career stage, discipline, education, experience, outputs and references from the supplied CV. No manual verification flags required. Unknown or ambiguous fields stay unknown; do not invent missing references or achievements."
    });
    let versions = paths.profile.join("versions");
    fs::create_dir_all(&versions)?;
    let version = versions.join(format!("source-{digest}.json"));
    if !version.exists() { fs::write(&version, serde_json::to_vec_pretty(&profile)?)?; }
    let _guard = profile_guard()?;
    recover_profile_import(paths)?;
    let mut onboarding = load(paths)?;
    onboarding.cv_source_file = Some(relative.clone());
    let onboarding = normalize_profile(onboarding)?;
    publish_profile_import(paths,&serde_json::to_vec_pretty(&profile)?,&serde_json::to_vec_pretty(&onboarding)?)?;
    Ok(relative)
}

// A short journal makes a two-file profile update recoverable after errors or restart.
// Backup names are generated here and validated before any recovery path is used.
pub(crate) fn recover_profile_import(paths: &AppPaths) -> Result<()> {
    let journal = paths.profile.join(".cv-import-journal.json");
    if !journal.is_file() { return Ok(()) }
    let backups: [Option<String>;2] = serde_json::from_slice(&fs::read(&journal)?)?;
    for backup in backups.iter().flatten() {
        if !backup.starts_with("profile-import-") || backup.contains(['/', '\\']) || !backup.ends_with(".bak") { bail!("画像恢复记录无效") }
        if !paths.backups.join(backup).is_file() { bail!("画像恢复备份缺失；未改动当前文件") }
    }
    for (name,backup) in ["master_profile.json",ONBOARDING_PROFILE_FILE].iter().zip(backups.iter()) {
        let live = paths.profile.join(name);
        if let Some(backup) = backup {
            let temporary = paths.profile.join(format!(".restore-{}",uuid::Uuid::new_v4().simple()));
            fs::copy(paths.backups.join(backup),&temporary)?;
            fs::rename(temporary,live)?;
        } else if live.is_file() {
            fs::remove_file(live)?;
        }
    }
    fs::remove_file(journal)?;
    Ok(())
}

fn publish_profile_import(paths: &AppPaths, master: &[u8], onboarding: &[u8]) -> Result<()> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    fs::create_dir_all(&paths.backups)?;
    let mut backups = Vec::new();
    let mut staged = Vec::new();
    for (index,(name,bytes)) in [("master_profile.json",master),(ONBOARDING_PROFILE_FILE,onboarding)].into_iter().enumerate() {
        let live = paths.profile.join(name);
        let backup = if live.exists() {
            let name = format!("profile-import-{id}-{index}.bak");
            fs::copy(&live,paths.backups.join(&name))?;
            Some(name)
        } else { None };
        backups.push(backup);
        let temp = paths.profile.join(format!(".import-{id}-{index}.tmp"));
        fs::write(&temp,bytes)?;
        staged.push((temp,live));
    }
    let journal = paths.profile.join(".cv-import-journal.json");
    let pending = paths.profile.join(format!(".journal-{id}.tmp"));
    fs::write(&pending,serde_json::to_vec(&backups)?)?;
    fs::rename(pending,&journal)?;
    let result = (|| -> Result<()> {
        for (temporary,live) in staged { fs::rename(temporary,live)?; }
        fs::remove_file(&journal)?;
        Ok(())
    })();
    if let Err(error) = result {
        recover_profile_import(paths).context("画像发布失败，恢复尚未完成；恢复记录已保留")?;
        return Err(error.context("画像发布失败；已恢复此前有效画像"))
    }
    Ok(())
}

pub fn copy_into_workspace(paths: &AppPaths, destination: &Path) -> Result<()> {
    let source_profile = paths.profile.join(ONBOARDING_PROFILE_FILE);
    if !source_profile.is_file() {
        return Ok(())
    }
    fs::create_dir_all(destination)?;
    fs::copy(&source_profile, destination.join(ONBOARDING_PROFILE_FILE))?;
    let profile = load(paths)?;
    let Some(relative) = profile.cv_source_file else { return Ok(()) };
    validate_source_relative(&relative)?;
    let source = paths.profile.join(&relative);
    if !source.is_file() {
        return Ok(())
    }
    let profile_root = paths.profile.canonicalize()?;
    let canonical = source.canonicalize()?;
    if !canonical.starts_with(&profile_root) {
        bail!("CV 导入路径超出用户资料目录")
    }
    let target = destination.join(relative);
    if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
    fs::copy(canonical, &target)?;
    let text_source = source.with_extension(format!("{}.txt", source.extension().and_then(|x| x.to_str()).unwrap_or("")));
    if text_source.is_file() {
        fs::copy(text_source, target.with_extension(format!("{}.txt", target.extension().and_then(|x| x.to_str()).unwrap_or(""))))?;
    }
    Ok(())
}

fn validate_source_relative(value: &str) -> Result<()> {
    let components = Path::new(value).components().collect::<Vec<_>>();
    if components.len() != 2
        || components[0].as_os_str() != "uploads"
        || !components.iter().all(|part| matches!(part,std::path::Component::Normal(_))) {
        bail!("CV 来源必须是本机 uploads 目录中的导入副本")
    }
    Ok(())
}

fn extract_pdf(paths: &AppPaths, source: &Path) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;
        let directory = paths.cache.join("pdf-import");
        crate::secrets::ensure_private_directory(&directory)?;
        let helper = directory.join(format!("cv-extract-{}", uuid::Uuid::new_v4().simple()));
        fs::write(&helper, include_bytes!(concat!(env!("OUT_DIR"), "/cv-extract")))?;
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))?;
        let text_path = helper.with_extension("txt");
        let output = fs::File::create(&text_path)?;
        let result = (|| -> Result<String> {
            let mut child = std::process::Command::new(&helper).arg(source)
                .stdout(output).stderr(std::process::Stdio::null()).spawn()?;
            let start = std::time::Instant::now();
            loop {
                if let Some(status) = child.try_wait()? {
                    if !status.success() {
                        match status.code() {
                            Some(2) => bail!("PDF 中没有足够的可识别文字，请选择清晰扫描件或可复制文字的 PDF"),
                            Some(3) => bail!("本机 OCR 无法完成识别，请重试或选择可复制文字的 PDF"),
                            _ => bail!("PDF 无法读取：请确认未加密且不超过 80 页"),
                        }
                    }
                    return fs::read_to_string(&text_path).context("读取 PDF 提取结果失败")
                }
                if start.elapsed().as_secs() >= 120 {
                    let _ = child.kill(); let _ = child.wait();
                    bail!("PDF 文字识别超时，请使用页数较少的 CV 或可复制文字的 PDF")
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })();
        let _ = fs::remove_file(&helper);
        let _ = fs::remove_file(&text_path);
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = paths;
        let document = lopdf::Document::load(source).context("PDF 无法读取")?;
        let pages = document.get_pages().keys().copied().collect::<Vec<_>>();
        if pages.len() > 80 { bail!("CV PDF 不能超过 80 页") }
        document.extract_text(&pages).context("PDF 文字提取失败；扫描件请先转为可复制文字的 PDF")
    }
}

fn validate_choice(label: &str, value: &str, allowed: &[&str]) -> Result<()> {
    if !allowed.contains(&value) {
        bail!("{label}选项无效")
    }
    Ok(())
}

fn backup_existing(paths: &AppPaths, live: &Path, label: &str) -> Result<()> {
    if !live.is_file() { return Ok(()) }
    fs::create_dir_all(&paths.backups)?;
    let backup = paths.backups.join(format!(
        "{}-{label}", uuid::Uuid::new_v4().simple()
    ));
    fs::copy(live, backup)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn invalid_onboarding_and_interrupted_pair_publish_keep_previous_profile() -> Result<()> {
        let temp=TempDir::new()?;
        let paths=test_paths(temp.path());paths.ensure()?;
        fs::write(paths.profile.join("master_profile.json"),b"old master")?;
        fs::write(paths.profile.join(ONBOARDING_PROFILE_FILE),b"invalid onboarding")?;
        let source=temp.path().join("source.txt");fs::write(&source,b"New source CV with a candidate's documented education and research history.")?;
        assert!(import_cv(&paths,&source).is_err());
        assert_eq!(fs::read(paths.profile.join("master_profile.json"))?,b"old master");
        let old=serde_json::to_vec(&OnboardingProfile::default())?;
        fs::write(paths.profile.join(ONBOARDING_PROFILE_FILE),&old)?;
        fs::write(paths.backups.join("profile-import-test-master.bak"),b"old master")?;
        fs::write(paths.backups.join("profile-import-test-onboarding.bak"),&old)?;
        fs::write(paths.profile.join(".cv-import-journal.json"),serde_json::to_vec(&["profile-import-test-master.bak","profile-import-test-onboarding.bak"])?)?;
        fs::write(paths.profile.join("master_profile.json"),b"partially published new master")?;
        load(&paths)?;
        assert_eq!(fs::read(paths.profile.join("master_profile.json"))?,b"old master");
        assert_eq!(fs::read(paths.profile.join(ONBOARDING_PROFILE_FILE))?,old);
        assert!(!paths.profile.join(".cv-import-journal.json").exists());
        fs::write(paths.profile.join("master_profile.json"),b"current master")?;
        fs::write(paths.profile.join(".cv-import-journal.json"),serde_json::to_vec(&["profile-import-test-master.bak","../invalid.bak"])?)?;
        assert!(load(&paths).is_err());
        assert_eq!(fs::read(paths.profile.join("master_profile.json"))?,b"current master");
        Ok(())
    }

    fn test_paths(root: &Path) -> AppPaths {
        AppPaths {
            database: root.join("database/careeros.sqlite3"),
            generated: root.join("generated"),
            profile: root.join("profile"),
            workspaces: root.join("workspaces"),
            codex_home: root.join("codex"),
            backups: root.join("backups"),
            cache: root.join("cache"),
            logs: root.join("logs"),
            runtime: root.join("runtime"),
            data_root: root.to_path_buf(),
        }
    }

    #[test]
    fn onboarding_profile_and_uploaded_cv_are_copied_without_deleting_source() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = test_paths(temp.path());
        paths.ensure()?;
        let original = temp.path().join("candidate.txt");
        fs::write(&original, b"Candidate Example. Doctoral research in social history. Education and teaching experience.")?;
        let imported = import_cv(&paths, &original)?;
        let saved = save(&paths, OnboardingProfile {
            completed: true,
            current_step: 5,
            career_stage: "doctoral".into(),
            discipline: "social_sciences".into(),
            cv_source_file: Some(imported.clone()),
            ..OnboardingProfile::default()
        })?;
        assert!(saved.updated_at.is_some());
        assert!(original.is_file());

        let workspace = paths.workspaces.join("onboarding-copy").join("profile");
        copy_into_workspace(&paths, &workspace)?;
        assert!(workspace.join(ONBOARDING_PROFILE_FILE).is_file());
        assert!(workspace.join(imported).is_file());
        Ok(())
    }
    #[test]
    fn replacing_cv_keeps_old_task_snapshot_and_creates_new_profile_version() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = test_paths(temp.path());
        paths.ensure()?;
        let source = temp.path().join("source.txt");
        fs::write(&source, "Alex Example. History research and university teaching. No references supplied.")?;
        let first = import_cv(&paths, &source)?;
        let workspace = paths.workspaces.join("old-task/profile");
        crate::materials::snapshot_profile(&paths, &workspace)?;
        let old = fs::read(workspace.join("master_profile.json"))?;
        fs::write(&source, "Alex Example. Updated social-science publication and new teaching experience.")?;
        let second = import_cv(&paths, &source)?;
        assert_ne!(first, second);
        crate::materials::snapshot_profile(&paths, &workspace)?;
        assert_eq!(old, fs::read(workspace.join("master_profile.json"))?);
        assert_ne!(old, fs::read(paths.profile.join("master_profile.json"))?);
        assert_eq!(load(&paths)?.cv_source_file, Some(second));
        assert!(paths.profile.join(first).is_file());
        assert_eq!(fs::read_dir(paths.profile.join("versions"))?.count(), 2);
        Ok(())
    }

    #[test]
    fn invalid_pdf_does_not_replace_the_active_profile() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = test_paths(temp.path());
        paths.ensure()?;
        let source = temp.path().join("source.txt");
        fs::write(&source, "A user supplied CV with education and research experience.")?;
        import_cv(&paths, &source)?;
        let before = fs::read(paths.profile.join("master_profile.json"))?;
        let bad = temp.path().join("broken.pdf");
        fs::write(&bad, b"not a PDF")?;
        assert!(import_cv(&paths, &bad).is_err());
        assert_eq!(before, fs::read(paths.profile.join("master_profile.json"))?);
        Ok(())
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn imports_real_text_pdf_and_image_only_scanned_pdf_locally() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = test_paths(temp.path());
        paths.ensure()?;
        let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/typst");
        let text = temp.path().join("source.typ");
        fs::write(&text, "#set page(paper: \"a4\")\n#set text(size: 20pt)\n= Alex Example\nDoctoral research in history. Education and teaching experience. Published historical research.")?;
        let compile = |source: &Path, destination: &Path| -> Result<()> {
            let output = std::process::Command::new(&binary).arg("compile").arg(source).arg(destination).output()?;
            if !output.status.success() { bail!("fixture compile failed: {}", String::from_utf8_lossy(&output.stderr)) }
            Ok(())
        };
        let pdf = temp.path().join("source.pdf");
        compile(&text, &pdf)?;
        import_cv(&paths, &pdf)?;
        let profile: serde_json::Value = serde_json::from_slice(&fs::read(paths.profile.join("master_profile.json"))?)?;
        assert!(profile["sourceText"].as_str().unwrap().contains("Alex Example"));
        compile(&text, &temp.path().join("source.png"))?;
        let scanned = temp.path().join("scanned.typ");
        fs::write(&scanned, "#set page(paper: \"a4\", margin: 0pt)\n#image(\"source.png\", width: 100%)")?;
        let scanned_pdf = temp.path().join("scanned.pdf");
        compile(&scanned, &scanned_pdf)?;
        import_cv(&paths, &scanned_pdf)?;
        let profile: serde_json::Value = serde_json::from_slice(&fs::read(paths.profile.join("master_profile.json"))?)?;
        assert!(profile["sourceText"].as_str().unwrap().contains("Alex Example"));
        assert!(profile["sourceText"].as_str().unwrap().contains("history"));
        Ok(())
    }

    #[test]
    fn imported_cv_paths_cannot_escape_or_point_back_to_source_files() {
        for path in ["/tmp/private.pdf","uploads/../master_profile.json","../uploads/a.pdf","uploads/a/b.pdf"] {
            assert!(validate_source_relative(path).is_err());
        }
        assert!(validate_source_relative("uploads/source.pdf").is_ok());
    }

}
