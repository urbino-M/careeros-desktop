use anyhow::{anyhow, Context, Result};
use keyring::Entry;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

const SERVICE: &str = "com.postdocos.desktop";
static SECRET_CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache() -> Result<MutexGuard<'static, HashMap<String, Option<String>>>> {
    SECRET_CACHE
        .lock()
        .map_err(|_| anyhow!("PostdocOS 凭据缓存不可用"))
}

pub fn set_secret(reference: &str, secret: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, reference).context("无法访问系统凭据库")?;
    entry
        .set_password(secret)
        .context("无法把凭据保存到系统凭据库")?;
    cache()?.insert(reference.to_owned(), Some(secret.to_owned()));
    Ok(())
}

pub fn get_secret(reference: &str) -> Result<Option<String>> {
    if let Some(value) = cache()?.get(reference).cloned() {
        return Ok(value);
    }
    let entry = Entry::new(SERVICE, reference).context("无法访问系统凭据库")?;
    let value = match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("无法从系统凭据库读取凭据"),
    }?;
    cache()?.insert(reference.to_owned(), value.clone());
    Ok(value)
}

pub fn delete_secret(reference: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, reference).context("无法访问系统凭据库")?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {
            cache()?.insert(reference.to_owned(), None);
            Ok(())
        }
        Err(error) => Err(error).context("无法从系统凭据库删除凭据"),
    }
}
