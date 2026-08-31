use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub data_root: PathBuf,
    pub database: PathBuf,
    pub generated: PathBuf,
    pub profile: PathBuf,
    pub workspaces: PathBuf,
    pub codex_home: PathBuf,
    pub backups: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    pub runtime: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self> {
        let data_root = if let Some(path) = std::env::var_os("POSTDOCOS_DATA_DIR") {
            PathBuf::from(path)
        } else {
            ::dirs::data_dir()
                .context("macOS Application Support 目录不可用")?
                .join("PostdocOS")
        };
        let cache = ::dirs::cache_dir()
            .context("macOS Cache 目录不可用")?
            .join("PostdocOS");
        let logs = ::dirs::home_dir()
            .context("用户目录不可用")?
            .join("Library/Logs/PostdocOS");

        Ok(Self {
            database: data_root.join("database/postdocos.sqlite3"),
            generated: data_root.join("generated"),
            profile: data_root.join("profile"),
            workspaces: data_root.join("workspaces"),
            codex_home: data_root.join("codex"),
            backups: data_root.join("backups"),
            data_root,
            cache,
            logs,
            runtime: locate_runtime_dir(),
        })
    }

    pub fn ensure(&self) -> Result<()> {
        for path in [
            &self.data_root,
            self.database.parent().expect("database parent"),
            &self.generated,
            &self.profile,
            &self.workspaces,
            &self.codex_home,
            &self.backups,
            &self.cache,
            &self.logs,
        ] {
            std::fs::create_dir_all(path)
                .with_context(|| format!("无法创建目录 {}", path.display()))?;
        }
        self.ensure_codex_config()?;
        self.install_builtin_skills()?;
        Ok(())
    }

    fn ensure_codex_config(&self) -> Result<()> {
        let path = self.codex_home.join("config.toml");
        let mut table: toml::Table = if path.exists() {
            toml::from_str(&fs::read_to_string(&path)?)?
        } else {
            toml::Table::new()
        };
        table.insert(
            "cli_auth_credentials_store".into(),
            toml::Value::String("keyring".into()),
        );
        fs::write(path, toml::to_string_pretty(&table)?)?;
        Ok(())
    }

    fn install_builtin_skills(&self) -> Result<()> {
        let source = self
            .runtime
            .parent()
            .context("内置运行时目录无效")?
            .join("skills/postdoc-application-agent");
        if !source.is_dir() {
            return Ok(())
        }
        let destination = self.codex_home.join("skills/postdoc-application-agent");
        for entry in WalkDir::new(&source) {
            let entry = entry?;
            let relative = entry.path().strip_prefix(&source)?;
            let target = destination.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)?;
            } else {
                if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
                fs::copy(entry.path(), target)?;
            }
        }
        Ok(())
    }
}

fn locate_runtime_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("POSTDOCOS_RUNTIME_DIR") {
        return PathBuf::from(path);
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(contents) = executable.parent().and_then(Path::parent) {
            let resources = contents.join("Resources");
            for bundled in [resources.join("runtime"), resources.join("resources/runtime")] {
                if bundled.is_dir() {
                    return bundled;
                }
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime")
}

pub fn locate_legacy_root() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("POSTDOCOS_LEGACY_ROOT") {
        let path = PathBuf::from(path);
        if is_legacy_root(&path) {
            return Some(path);
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_candidate = manifest
        .parent()
        .and_then(Path::parent)
        .map(|path| path.join("postdoc-os"));
    workspace_candidate.filter(|path| is_legacy_root(path))
}

fn is_legacy_root(path: &Path) -> bool {
    path.join("data/postdoc.db").is_file() && path.join("generated").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_tree_finds_legacy_root() {
        if let Some(root) = locate_legacy_root() {
            assert!(root.join("data/postdoc.db").is_file());
        }
    }
}
