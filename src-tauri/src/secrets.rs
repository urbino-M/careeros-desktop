use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::OnceLock;
use std::sync::{LazyLock, Mutex, MutexGuard};

const STORE_VERSION: u32 = 1;
const STORE_DIRECTORY: &str = "credentials";
const STORE_FILENAME: &str = "secrets.json";
static STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
#[cfg(windows)]
static WINDOWS_IDENTITY: OnceLock<String> = OnceLock::new();

#[derive(Debug, Default, Deserialize, Serialize)]
struct SecretStore {
    version: u32,
    secrets: HashMap<String, String>,
}

fn store_lock() -> Result<MutexGuard<'static, ()>> {
    STORE_LOCK
        .lock()
        .map_err(|_| anyhow!("CareerOS 凭据文件锁不可用"))
}

fn data_root() -> Result<PathBuf> {
    Ok(if let Some(path) = std::env::var_os("CAREEROS_DATA_DIR") {
        PathBuf::from(path)
    } else {
        ::dirs::data_dir()
            .context("系统应用数据目录不可用")?
            .join("CareerOS")
    })
}

fn store_path() -> Result<PathBuf> {
    ensure_storage(&data_root()?)
}

pub fn ensure_storage(data_root: &Path) -> Result<PathBuf> {
    let directory = data_root.join(STORE_DIRECTORY);
    ensure_private_directory(&directory)?;
    let path = directory.join(STORE_FILENAME);
    if path.exists() {
        reject_symlink(&path)?;
        ensure_private_file(&path)?;
    }
    Ok(path)
}

pub fn set_secret(reference: &str, secret: &str) -> Result<()> {
    let _guard = store_lock()?;
    let path = store_path()?;
    let mut store = read_store(&path)?;
    store
        .secrets
        .insert(reference.to_owned(), secret.to_owned());
    write_store(&path, &store)
}

pub fn get_secret(reference: &str) -> Result<Option<String>> {
    let _guard = store_lock()?;
    let path = store_path()?;
    let store = read_store(&path)?;
    if let Some(secret) = store.secrets.get(reference) {
        return Ok(Some(secret.clone()));
    }

    Ok(None)
}

pub fn delete_secret(reference: &str) -> Result<()> {
    let _guard = store_lock()?;
    let path = store_path()?;
    let mut store = read_store(&path)?;
    if store.secrets.remove(reference).is_some() {
        write_store(&path, &store)?;
    }
    Ok(())
}

fn read_store(path: &Path) -> Result<SecretStore> {
    if !path.exists() {
        return Ok(SecretStore {
            version: STORE_VERSION,
            secrets: HashMap::new(),
        });
    }
    reject_symlink(path)?;
    let store: SecretStore =
        serde_json::from_slice(&fs::read(path).context("无法读取 CareerOS 凭据文件")?)
            .context("CareerOS 凭据文件格式无效")?;
    if store.version != STORE_VERSION {
        return Err(anyhow!("CareerOS 凭据文件版本不受支持"));
    }
    Ok(store)
}

fn write_store(path: &Path, store: &SecretStore) -> Result<()> {
    let parent = path.parent().context("CareerOS 凭据文件路径无效")?;
    ensure_private_directory(parent)?;
    if path.exists() {
        reject_symlink(path)?;
    }

    let encoded = serde_json::to_vec(store)?;
    let temporary = parent.join(format!(
        ".{STORE_FILENAME}.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));

    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options
            .open(&temporary)
            .context("无法创建 CareerOS 临时凭据文件")?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        ensure_private_file(&temporary)?;
        fs::rename(&temporary, path).context("无法原子替换 CareerOS 凭据文件")?;
        ensure_private_file(path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn reject_symlink(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(anyhow!("CareerOS 凭据路径不能是符号链接"));
    }
    Ok(())
}

pub fn ensure_private_directory(path: &Path) -> Result<()> {
    if path.exists() {
        reject_symlink(path)?;
    } else {
        fs::create_dir_all(path)
            .with_context(|| format!("无法创建私有目录：{}", path.display()))?;
    }
    restrict_directory_permissions(path)
}

pub fn ensure_private_file(path: &Path) -> Result<()> {
    reject_symlink(path)?;
    restrict_file_permissions(path)
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .context("无法限制 CareerOS 私有目录权限")
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .context("无法限制 CareerOS 凭据文件权限")
}

#[cfg(windows)]
fn restrict_directory_permissions(path: &Path) -> Result<()> {
    restrict_windows_acl(path, true)
}

#[cfg(windows)]
fn restrict_file_permissions(path: &Path) -> Result<()> {
    restrict_windows_acl(path, false)
}

#[cfg(windows)]
fn restrict_windows_acl(path: &Path, directory: bool) -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let identity = windows_identity()?;
    let permission = if directory {
        format!("{identity}:(OI)(CI)F")
    } else {
        format!("{identity}:F")
    };
    let output = Command::new("icacls")
        .creation_flags(CREATE_NO_WINDOW)
        .arg(path)
        .args(["/inheritance:r", "/grant:r"])
        .arg(permission)
        .output()
        .context("无法调用 Windows ACL 工具")?;
    if !output.status.success() {
        return Err(anyhow!(
            "无法把凭据权限限制为当前 Windows 用户：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn windows_identity() -> Result<&'static str> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    if let Some(identity) = WINDOWS_IDENTITY.get() {
        return Ok(identity);
    }
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let whoami = Command::new("whoami")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("无法识别当前 Windows 用户")?;
    if !whoami.status.success() {
        return Err(anyhow!("无法识别当前 Windows 用户，拒绝保存凭据"));
    }
    let identity = String::from_utf8(whoami.stdout)
        .context("Windows 用户名不是有效文本")?
        .trim()
        .to_owned();
    if identity.is_empty() {
        return Err(anyhow!("Windows 用户名为空，拒绝保存凭据"));
    }
    let _ = WINDOWS_IDENTITY.set(identity);
    WINDOWS_IDENTITY
        .get()
        .map(String::as_str)
        .context("无法缓存当前 Windows 用户")
}

#[cfg(not(any(unix, windows)))]
fn restrict_directory_permissions(_path: &Path) -> Result<()> {
    Err(anyhow!("当前平台不支持安全的凭据目录权限"))
}

#[cfg(not(any(unix, windows)))]
fn restrict_file_permissions(_path: &Path) -> Result<()> {
    Err(anyhow!("当前平台不支持安全的凭据文件权限"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trip() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = ensure_storage(temp.path())?;
        let mut store = read_store(&path)?;
        store
            .secrets
            .insert("provider:test".into(), "secret-value".into());
        write_store(&path, &store)?;

        store
            .secrets
            .insert("provider:test".into(), "replacement".into());
        write_store(&path, &store)?;

        let stored = read_store(&path)?;
        assert_eq!(
            stored.secrets.get("provider:test").map(String::as_str),
            Some("replacement")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path.parent().unwrap())?.permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_store() -> Result<()> {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir()?;
        let target = temp.path().join("target.json");
        fs::write(&target, br#"{"version":1,"secrets":{}}"#)?;
        let link = temp.path().join("linked-secrets.json");
        symlink(&target, &link)?;
        assert!(read_store(&link).is_err());
        Ok(())
    }

}
