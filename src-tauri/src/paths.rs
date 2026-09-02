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
    pub fn resolve(resource_dir: Option<&Path>) -> Result<Self> {
        let data_root = if let Some(path) = std::env::var_os("CAREEROS_DATA_DIR") {
            PathBuf::from(path)
        } else {
            ::dirs::data_dir()
                .context("系统应用数据目录不可用")?
                .join("CareerOS")
        };
        let cache = ::dirs::cache_dir()
            .context("系统缓存目录不可用")?
            .join("CareerOS");
        #[cfg(target_os = "macos")]
        let logs = ::dirs::home_dir()
            .context("用户目录不可用")?
            .join("Library/Logs/CareerOS");
        #[cfg(not(target_os = "macos"))]
        let logs = data_root.join("logs");

        Ok(Self {
            database: data_root.join("database/careeros.sqlite3"),
            generated: data_root.join("generated"),
            profile: data_root.join("profile"),
            workspaces: data_root.join("workspaces"),
            codex_home: data_root.join("codex"),
            backups: data_root.join("backups"),
            data_root,
            cache,
            logs,
            runtime: locate_runtime_dir(resource_dir),
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
        crate::secrets::ensure_storage(&self.data_root)?;
        crate::secrets::ensure_private_directory(&self.codex_home)?;
        let codex_auth = self.codex_home.join("auth.json");
        if codex_auth.exists() {
            crate::secrets::ensure_private_file(&codex_auth)?;
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
            toml::Value::String("file".into()),
        );
        fs::write(path, toml::to_string_pretty(&table)?)?;
        Ok(())
    }

    fn install_builtin_skills(&self) -> Result<()> {
        let skills = self
            .runtime
            .parent()
            .context("内置运行时目录无效")?
            .join("skills");
        for name in ["postdoc-application-agent", "internship-application-agent"] {
            let source = skills.join(name);
            if !source.is_dir() {
                continue;
            }
            let destination = self.codex_home.join("skills").join(name);
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
        }
        Ok(())
    }
}

fn locate_runtime_dir(resource_dir: Option<&Path>) -> PathBuf {
    if let Some(path) = std::env::var_os("CAREEROS_RUNTIME_DIR") {
        return PathBuf::from(path);
    }
    if let Some(resources) = resource_dir {
        for bundled in [
            resources.join("resources/runtime"),
            resources.join("runtime"),
        ] {
            if bundled.is_dir() {
                return bundled;
            }
        }
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(executable_dir) = executable.parent() {
            let bundle_resources = executable_dir
                .parent()
                .map(|contents| contents.join("Resources"));
            let mut candidates = vec![
                executable_dir.join("resources/runtime"),
                executable_dir.join("runtime"),
            ];
            if let Some(resources) = bundle_resources {
                candidates.push(resources.join("runtime"));
                candidates.push(resources.join("resources/runtime"));
            }
            for bundled in candidates {
                if bundled.is_dir() {
                    return bundled;
                }
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime")
}

pub fn runtime_binary(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}
