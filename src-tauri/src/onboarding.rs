use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const ONBOARDING_PROFILE_FILE: &str = "onboarding_profile.json";
const MAX_TEXT_CHARS: usize = 4_000;
const MAX_SHORT_CHARS: usize = 200;
const MAX_CV_BYTES: u64 = 25 * 1024 * 1024;

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

pub fn save(paths: &AppPaths, mut value: OnboardingProfile) -> Result<OnboardingProfile> {
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
    value.updated_at = Some(Utc::now().to_rfc3339());
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
    let filename = format!("source-cv-{}.{}", Utc::now().format("%Y%m%dT%H%M%SZ"), extension);
    let destination = uploads.join(&filename);
    fs::copy(&source, &destination)
        .with_context(|| format!("无法导入 CV：{}", source.display()))?;
    Ok(format!("uploads/{filename}"))
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
    fs::copy(canonical, target)?;
    Ok(())
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
        "{}-{label}", Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    fs::copy(live, backup)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
        let original = temp.path().join("candidate.pdf");
        fs::write(&original, b"sample cv")?;
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
}
