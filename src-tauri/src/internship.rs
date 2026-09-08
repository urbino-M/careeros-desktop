use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const INTERNSHIP_PROFILE_FILE: &str = "internship.json";
const MAX_FIELD_CHARS: usize = 4_000;
const MAX_CV_PATH_CHARS: usize = 500;
const MAX_RSS_FEEDS: usize = 20;
const MAX_CV_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternshipProfile {
    #[serde(default = "schema_version")]
    pub schema_version: u8,
    #[serde(default)]
    pub target_roles: String,
    #[serde(default)]
    pub industries: String,
    #[serde(default)]
    pub regions: String,
    #[serde(default)]
    pub work_mode: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub work_authorization: String,
    #[serde(default)]
    pub enrollment_status: String,
    #[serde(default)]
    pub constraints: String,
    #[serde(default)]
    pub cv_path: Option<String>,
    #[serde(default)]
    pub rss_feeds: Vec<String>,
    pub updated_at: Option<String>,
}

impl Default for InternshipProfile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            target_roles: String::new(),
            industries: String::new(),
            regions: String::new(),
            work_mode: String::new(),
            start_date: String::new(),
            duration: String::new(),
            work_authorization: String::new(),
            enrollment_status: String::new(),
            constraints: String::new(),
            cv_path: None,
            rss_feeds: Vec::new(),
            updated_at: None,
        }
    }
}

fn schema_version() -> u8 { 1 }

pub fn load(paths: &AppPaths) -> Result<InternshipProfile> {
    let path = paths.profile.join(INTERNSHIP_PROFILE_FILE);
    if !path.is_file() {
        return Ok(InternshipProfile::default());
    }
    let profile: InternshipProfile = serde_json::from_slice(&fs::read(&path)?)
        .with_context(|| format!("Internship 画像无法读取：{}", path.display()))?;
    if profile.schema_version != 1 {
        bail!("不支持的 Internship 画像版本：{}", profile.schema_version);
    }
    validate(profile)
}

pub fn save(paths: &AppPaths, mut profile: InternshipProfile) -> Result<InternshipProfile> {
    let _guard = crate::onboarding::profile_guard()?;
    profile.schema_version = 1;
    profile = validate(profile)?;
    profile.updated_at = Some(Utc::now().to_rfc3339());
    fs::create_dir_all(&paths.profile)?;
    let live = paths.profile.join(INTERNSHIP_PROFILE_FILE);
    if live.is_file() {
        fs::create_dir_all(&paths.backups)?;
        let backup = paths.backups.join(format!(
            "{}-{INTERNSHIP_PROFILE_FILE}",
            Utc::now().format("%Y%m%dT%H%M%SZ")
        ));
        fs::copy(&live, backup)?;
    }
    let temporary = paths.profile.join(format!(".{INTERNSHIP_PROFILE_FILE}.tmp"));
    fs::write(&temporary, serde_json::to_vec_pretty(&profile)?)?;
    fs::rename(&temporary, &live)?;
    Ok(profile)
}

pub fn import_cv(paths: &AppPaths, source: &Path) -> Result<String> {
    let source = source
        .canonicalize()
        .with_context(|| format!("无法读取 CV 文件：{}", source.display()))?;
    if !source.is_file() {
        bail!("请选择一个 CV 文件");
    }
    let extension = source
        .extension()
        .and_then(|item| item.to_str())
        .map(str::to_lowercase)
        .context("CV 文件缺少扩展名")?;
    if !matches!(extension.as_str(), "pdf" | "docx" | "md" | "txt") {
        bail!("CV 仅支持 PDF、DOCX、Markdown 或纯文本");
    }
    if source.metadata()?.len() > MAX_CV_BYTES {
        bail!("CV 文件不能超过 25 MB");
    }
    let uploads = paths.profile.join("uploads");
    fs::create_dir_all(&uploads)?;
    let filename = format!(
        "internship-cv-{}.{}",
        uuid::Uuid::new_v4().simple(),
        extension
    );
    let destination = uploads.join(&filename);
    fs::copy(&source, &destination)
        .with_context(|| format!("无法导入 CV：{}", source.display()))?;
    Ok(format!("uploads/{filename}"))
}

fn validate(mut profile: InternshipProfile) -> Result<InternshipProfile> {
    for (label, field) in [
        ("目标岗位", &mut profile.target_roles),
        ("目标行业", &mut profile.industries),
        ("目标地区", &mut profile.regions),
        ("工作方式", &mut profile.work_mode),
        ("开始时间", &mut profile.start_date),
        ("实习时长", &mut profile.duration),
        ("工作许可", &mut profile.work_authorization),
        ("在读状态", &mut profile.enrollment_status),
        ("限制条件", &mut profile.constraints),
    ] {
        *field = field.trim().to_owned();
        if field.chars().count() > MAX_FIELD_CHARS {
            bail!("{label}最多允许 {MAX_FIELD_CHARS} 个字符");
        }
    }
    profile.cv_path = profile
        .cv_path
        .take()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if let Some(path) = profile.cv_path.as_deref() {
        if path.chars().count() > MAX_CV_PATH_CHARS || !is_safe_relative_path(path) {
            bail!("CV 路径必须是 profile 目录内的相对路径");
        }
    }
    let mut feeds = Vec::new();
    let mut seen = HashSet::new();
    for feed in profile.rss_feeds.drain(..) {
        let feed = feed.trim().to_owned();
        if feed.is_empty() {
            continue;
        }
        if !is_http_url(&feed) {
            bail!("RSS 地址必须是 http 或 https URL");
        }
        if seen.insert(feed.clone()) {
            feeds.push(feed);
        }
        if feeds.len() > MAX_RSS_FEEDS {
            bail!("最多配置 {MAX_RSS_FEEDS} 个 RSS 地址");
        }
    }
    profile.rss_feeds = feeds;
    Ok(profile)
}

pub fn copy_into_workspace(paths: &AppPaths, destination: &Path) -> Result<()> {
    let _guard = crate::onboarding::profile_guard()?;
    if destination.join(".internship-snapshot-complete").is_file() { return Ok(()); }
    if destination.is_dir() && destination.read_dir()?.next().is_some() {
        bail!("旧 Internship 工作区没有独立画像快照，请新建任务；原资料保留")
    }
    let staging = destination.with_file_name(format!(".internship-profile-{}",uuid::Uuid::new_v4().simple()));
    copy_profile_files(paths, &staging)?;
    fs::write(staging.join(".internship-snapshot-complete"), b"1")?;
    if destination.is_dir() { fs::remove_dir(destination)?; } // Only the empty directory checked above.
    fs::rename(staging,destination)?;
    Ok(())
}

fn copy_profile_files(paths: &AppPaths, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let profile = load(paths)?;
    fs::write(
        destination.join(INTERNSHIP_PROFILE_FILE),
        serde_json::to_vec_pretty(&profile)?,
    )?;
    let Some(relative) = profile.cv_path else {
        return Ok(());
    };
    let source = paths.profile.join(&relative);
    if !source.is_file() {
        return Ok(());
    }
    let profile_root = paths.profile.canonicalize()?;
    let canonical = source.canonicalize()?;
    if !canonical.starts_with(&profile_root) {
        bail!("Internship CV 路径超出用户资料目录");
    }
    let target = destination.join(PathBuf::from(&relative));
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(canonical, target)?;
    Ok(())
}

fn is_safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute()
        && path.components().all(|component| {
            !matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
}

fn is_http_url(value: &str) -> bool {
    url::Url::parse(value)
        .map(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or(false)
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
    fn internship_profile_isolated_from_postdoc_profile() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = test_paths(temp.path());
        paths.ensure()?;
        fs::write(paths.profile.join("master_profile.json"), b"postdoc facts")?;
        let cv = paths.profile.join("internship-cv.pdf");
        fs::write(&cv, b"cv")?;
        let profile = save(&paths, InternshipProfile {
            target_roles: "ML Engineer Intern".into(),
            cv_path: Some("internship-cv.pdf".into()),
            ..InternshipProfile::default()
        })?;
        assert_eq!(profile.target_roles, "ML Engineer Intern");
        let workspace = paths.workspaces.join("internship").join("profile");
        copy_into_workspace(&paths, &workspace)?;
        assert!(workspace.join(INTERNSHIP_PROFILE_FILE).is_file());
        assert!(workspace.join("internship-cv.pdf").is_file());
        assert!(!workspace.join("master_profile.json").exists());
        save(&paths, InternshipProfile { target_roles:"Changed later".into(), ..InternshipProfile::default() })?;
        copy_into_workspace(&paths,&workspace)?;
        let snapshot: InternshipProfile = serde_json::from_slice(&fs::read(workspace.join(INTERNSHIP_PROFILE_FILE))?)?;
        assert_eq!(snapshot.target_roles,"ML Engineer Intern");
        assert!(workspace.join("internship-cv.pdf").is_file());
        Ok(())
    }

    #[test]
    fn profile_rejects_paths_outside_profile() {
        assert!(!is_safe_relative_path("../candidate.pdf"));
        assert!(!is_safe_relative_path("/tmp/candidate.pdf"));
        assert!(is_safe_relative_path("uploads/candidate.pdf"));
    }

    #[test]
    fn internship_cv_import_copies_supported_file_without_deleting_source() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = test_paths(temp.path());
        paths.ensure()?;
        let original = temp.path().join("candidate.PDF");
        fs::write(&original, b"sample cv")?;

        let imported = import_cv(&paths, &original)?;

        assert!(original.is_file());
        assert!(paths.profile.join(imported).is_file());
        Ok(())
    }
}
