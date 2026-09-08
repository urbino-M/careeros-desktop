use crate::db;
use crate::models::{GmailDraftInfo, GmailOAuthStart, GmailStatus};
use crate::paths::AppPaths;
use crate::secrets;
use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use reqwest::Client;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;
use uuid::Uuid;

const CLIENT_SECRET_REF: &str = "gmail-oauth-client";
const TOKEN_SECRET_REF: &str = "gmail-oauth-token";
const GMAIL_SCOPE: &str = "https://www.googleapis.com/auth/gmail.compose";

#[derive(Debug, Clone, Deserialize)]
struct ClientFile {
    installed: Option<OAuthClient>,
    web: Option<OAuthClient>,
}

#[derive(Debug, Clone, Deserialize)]
struct OAuthClient {
    client_id: String,
    client_secret: Option<String>,
    auth_uri: Option<String>,
    token_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OAuthToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenExchange {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: Option<String>,
    scope: Option<String>,
}

#[derive(Clone)]
pub struct GmailManager {
    paths: AppPaths,
    http: Client,
    api_base: String,
}

impl GmailManager {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths, http: Client::new(), api_base: "https://gmail.googleapis.com/gmail/v1/users/me".into() }
    }

    pub fn import_client_file(&self, path: &Path) -> Result<()> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("无法读取 OAuth 客户端文件：{}", path.display()))?;
        self.save_client_json(&raw)
    }

    pub fn save_client_json(&self, raw: &str) -> Result<()> {
        let parsed: ClientFile = serde_json::from_str(raw).context("OAuth 客户端 JSON 无效")?;
        if parsed.installed.is_none() && parsed.web.is_some() {
            bail!("这是 Web OAuth 客户端；请在 Google Cloud 创建并下载“桌面应用”客户端 JSON")
        }
        let client = parsed.installed.as_ref().context("请选择 Google Cloud 的“桌面应用”OAuth 客户端 JSON")?;
        if client.client_id.trim().is_empty() {
            bail!("OAuth 客户端 JSON 缺少 client_id")
        }
        secrets::set_secret(CLIENT_SECRET_REF, raw)?;
        Ok(())
    }

    pub fn status(&self) -> Result<GmailStatus> {
        let configured = secrets::get_secret(CLIENT_SECRET_REF)?.is_some();
        let connected = secrets::get_secret(TOKEN_SECRET_REF)?.is_some();
        let conn = db::connect(&self.paths.database)?;
        let account_email: Option<String> = conn
            .query_row("SELECT email FROM gmail_accounts WHERE id='gmail-primary'", [], |row| row.get(0))
            .optional()?;
        let (oauth_status, oauth_message): (String, Option<String>) = conn.query_row(
            "SELECT status,message FROM gmail_oauth_state WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let connection_ok = configured && connected && account_email.is_some();
        Ok(GmailStatus {
            configured,
            connected,
            account_email,
            connection_ok,
            oauth_status,
            oauth_message,
        })
    }

    pub async fn start_oauth(&self) -> Result<GmailOAuthStart> {
        let client = load_client()?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}");
        let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = Uuid::new_v4().simple().to_string();
        let auth_endpoint = client.auth_uri.as_deref().unwrap_or("https://accounts.google.com/o/oauth2/v2/auth");
        let mut url = Url::parse(auth_endpoint)?;
        url.query_pairs_mut()
            .append_pair("client_id", &client.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", GMAIL_SCOPE)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);
        record_oauth_state(&self.paths.database, "pending", Some("等待 Google 授权"))?;

        let manager = self.clone();
        let redirect_for_task = redirect_uri.clone();
        tauri::async_runtime::spawn(async move {
            let result = manager
                .finish_oauth(listener, client, redirect_for_task, verifier, state)
                .await;
            if let Err(error) = result {
                let _ = record_oauth_state(&manager.paths.database, "failed", Some(&format!("{error:#}")));
            }
        });

        Ok(GmailOAuthStart { authorization_url: url.to_string(), redirect_uri })
    }

    async fn finish_oauth(
        &self,
        listener: TcpListener,
        client: OAuthClient,
        redirect_uri: String,
        verifier: String,
        expected_state: String,
    ) -> Result<()> {
        let (mut stream, _) = tokio::time::timeout(std::time::Duration::from_secs(300), listener.accept())
            .await
            .context("等待 Google 授权超时")??;
        let mut buffer = vec![0_u8; 16 * 1024];
        let read = stream.read(&mut buffer).await?;
        let request = String::from_utf8_lossy(&buffer[..read]);
        let request_target = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).context("OAuth 回调格式无效")?;
        let callback = Url::parse(&format!("http://127.0.0.1{request_target}"))?;
        let query: std::collections::HashMap<String, String> = callback.query_pairs().into_owned().collect();
        let state = query.get("state").context("OAuth 回调缺少 state")?;
        if state != &expected_state { bail!("OAuth state 校验失败") }
        if let Some(error) = query.get("error") { bail!("Google 拒绝授权：{error}") }
        let code = query.get("code").context("OAuth 回调缺少授权码")?;

        let page = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><meta charset=utf-8><title>CareerOS</title><style>body{font-family:-apple-system;padding:48px;background:#06171f;color:#ebf4f2}strong{color:#48cdd0}</style><h1>Gmail 已授权</h1><p>CareerOS 正在核验账号。你可以关闭这个页面并返回应用。</p>";
        stream.write_all(page.as_bytes()).await?;
        stream.shutdown().await?;

        let endpoint = client.token_uri.as_deref().unwrap_or("https://oauth2.googleapis.com/token");
        let mut form = vec![
            ("code", code.as_str()),
            ("client_id", client.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier.as_str()),
        ];
        if let Some(secret) = client.client_secret.as_deref() { form.push(("client_secret", secret)); }
        let response = self.http.post(endpoint).form(&form).send().await?;
        if !response.status().is_success() { bail!("Google token 交换失败：{}", response.text().await?) }
        let exchange: TokenExchange = response.json().await?;
        let token = OAuthToken {
            access_token: exchange.access_token,
            refresh_token: exchange.refresh_token,
            expires_at: exchange.expires_in.map(|seconds| (Utc::now() + ChronoDuration::seconds(seconds)).to_rfc3339()),
            token_type: exchange.token_type,
            scope: exchange.scope,
        };
        let profile = self.fetch_profile_with_token(&token.access_token).await?;
        let email = profile.get("emailAddress").and_then(Value::as_str).context("Gmail 未返回账号邮箱")?;
        secrets::set_secret(TOKEN_SECRET_REF, &serde_json::to_string(&token)?)?;
        save_account(&self.paths.database, email)?;
        record_oauth_state(&self.paths.database, "connected", Some("账号已核验"))?;
        Ok(())
    }

    pub async fn create_draft(
        &self,
        target_id: &str,
        recipient: &str,
        subject: &str,
        body: &str,
    ) -> Result<GmailDraftInfo> {
        if !valid_email(recipient) { bail!("收件人邮箱格式无效") }
        if subject.trim().is_empty() || body.trim().is_empty() { bail!("邮件主题和正文不能为空") }
        let conn = db::connect(&self.paths.database)?;
        let (application_id, target_email): (String, Option<String>) = conn.query_row(
            "SELECT application_id,email FROM contact_targets_v2 WHERE id=?1 AND archived_at IS NULL",
            [target_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let target_email = target_email.context("当前联系目标没有已核验邮箱")?;
        if !recipient.trim().eq_ignore_ascii_case(target_email.trim()) {
            bail!("收件人必须与当前联系目标邮箱一致：{target_email}")
        }
        let attachment = approved_attachment(&self.paths, &conn, target_id)?;

        drop(conn);
        let access_token = self.valid_access_token().await?;
        let profile = self.fetch_profile_with_token(&access_token).await?;
        let account = profile.get("emailAddress").and_then(Value::as_str).context("Gmail 未返回当前账号邮箱")?;
        let conn = db::connect(&self.paths.database)?;
        let connected_account: Option<String> = conn.query_row(
            "SELECT email FROM gmail_accounts WHERE id='gmail-primary'", [], |row| row.get(0)
        ).optional()?;
        let connected_account = connected_account.context("Gmail 账号记录缺失，请在设置中重新连接")?;
        if !account.eq_ignore_ascii_case(&connected_account) {
            bail!("Gmail 当前授权账号与已连接账号不一致，请重新连接")
        }
        // Build from the already checked bytes, never reread the path after an await.
        let current = approved_attachment(&self.paths,&conn,target_id)?;
        if current.path != attachment.path || current.hash != attachment.hash { bail!("CV 已换版或审核已失效，请重新审核") }
        let raw = build_mime(recipient, subject, body, &attachment.bytes)?;
        let key = format!("{:x}",Sha256::digest(serde_json::to_vec(&json!([target_id,account,recipient,subject,body,attachment.hash]))?));
        let request = DraftRequest { record_id:format!("gmail-request-{key}"),target_id:target_id.into(),application_id,
            recipient:recipient.trim().into(),subject:subject.trim().into(),cv_path:attachment.path };
        drop(conn);
        self.publish_draft(&access_token,&request,&raw).await
    }

    async fn publish_draft(&self, access_token: &str, request: &DraftRequest, raw: &str) -> Result<GmailDraftInfo> {
        let existing = reserve_draft(&self.paths.database, request)?;
        let draft_id = if let Some(id) = existing {
            if id.starts_with("unconfirmed:") {
                bail!("上次创建请求的结果不确定；为防止重复草稿，本次未再次创建。请先在 Gmail 草稿箱核对并手动处理。")
            }
            id
        } else {
            let response = self.http.post(format!("{}/drafts",self.api_base)).bearer_auth(access_token)
                .timeout(std::time::Duration::from_secs(30)).json(&json!({"message":{"raw":raw}})).send().await
                .context("Gmail 创建请求结果不确定；请先在 Gmail 草稿箱核对，本次请求不会自动重复提交")?;
            if !response.status().is_success() {
                let status = response.status();
                // Explicit rejection is safe to retry; uncertain transport/server failures stay reserved.
                if status.is_client_error() && status.as_u16() != 408 {
                    db::connect(&self.paths.database)?.execute("DELETE FROM gmail_drafts WHERE id=?1 AND gmail_draft_id LIKE 'unconfirmed:%'",[&request.record_id])?;
                }
                bail!("Gmail 创建草稿失败（{status}）；请核对草稿箱后再处理：{}",response.text().await?)
            }
            let draft: Value = response.json().await.context("Gmail 创建结果无法解析；请手动核对草稿箱，不会自动重复提交")?;
            let id = draft.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()).context("Gmail 未返回 draft ID；请手动核对草稿箱")?.to_owned();
            let message = draft.pointer("/message/id").and_then(Value::as_str);
            db::connect(&self.paths.database)?.execute(
                "UPDATE gmail_drafts SET gmail_draft_id=?2,gmail_message_id=?3 WHERE id=?1",
                params![request.record_id,id,message],
            ).with_context(||format!("草稿已创建（ID：{id}），但本地记录更新失败；请保留此 ID 并在 Gmail 核对，不要重复创建"))?;
            id
        };
        // The remote ID is durable before verification. Retrying this request only GETs this ID.
        let verified = match self.http.get(format!("{}/drafts/{}?format=minimal",self.api_base,url_path_segment(&draft_id)))
            .bearer_auth(access_token).timeout(std::time::Duration::from_secs(30)).send().await {
            Ok(response) if response.status().is_success() => response.json::<Value>().await.ok()
                .and_then(|value|value.get("id").and_then(Value::as_str).map(|id|id==draft_id)).unwrap_or(false),
            _ => false,
        };
        db::connect(&self.paths.database)?.execute(
            "UPDATE gmail_draft_targets SET remote_verified=?2,remote_verified_at=CASE WHEN ?2=1 THEN strftime('%Y-%m-%dT%H:%M:%SZ','now') ELSE NULL END WHERE gmail_draft_record_id=?1",
            params![request.record_id,verified as i64],
        )?;
        self.list_drafts(&request.target_id)?.into_iter().find(|item|item.id==request.record_id).context("草稿记录读取失败")
    }

    pub fn approve_cv(&self, target_id: &str, preview_path: &str, preview_sha256: &str) -> Result<String> {
        let database_conn = db::connect(&self.paths.database)?;
        let conn = db::publication_transaction(&database_conn)?;
        let artifact_path: String = conn.query_row(
            "SELECT path FROM contact_target_artifacts
             WHERE target_id=?1 AND artifact_type='cv_pdf' AND language='en'",
            [target_id], |row| row.get(0),
        ).context("当前申请没有 CV PDF")?;
        let resolved = resolve_data_path(&self.paths.data_root, &artifact_path);
        let hash = sha256_file(&resolved)?;
        if resolved != resolve_data_path(&self.paths.data_root,preview_path) || hash != preview_sha256 {
            bail!("CV 已与当前预览不同，请重新载入并审核；未批准未预览的版本")
        }
        conn.execute(
            "INSERT INTO target_artifact_approvals(target_id,artifact_type,language,artifact_path,approved_sha256)
             VALUES(?1,'cv_pdf','en',?2,?3)
             ON CONFLICT(target_id,artifact_type,language) DO UPDATE SET
                artifact_path=excluded.artifact_path,approved_sha256=excluded.approved_sha256,
                approved_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
            params![target_id, artifact_path, hash],
        )?;
        conn.commit()?;
        Ok(hash)
    }

    pub fn cv_approval_status(&self, target_id: &str) -> Result<bool> {
        let conn = db::connect(&self.paths.database)?;
        let value: Option<(String, String)> = conn.query_row(
            "SELECT p.artifact_path,p.approved_sha256
             FROM target_artifact_approvals p
             JOIN contact_target_artifacts a ON a.target_id=p.target_id AND a.artifact_type=p.artifact_type AND a.language=p.language AND a.path=p.artifact_path
             WHERE p.target_id=?1 AND p.artifact_type='cv_pdf' AND p.language='en'",
            [target_id], |row| Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
        let Some((path, expected)) = value else { return Ok(false) };
        let resolved = resolve_data_path(&self.paths.data_root, &path);
        Ok(resolved.exists() && sha256_file(&resolved)? == expected)
    }

    pub fn list_drafts(&self, target_id: &str) -> Result<Vec<GmailDraftInfo>> {
        let conn = db::connect(&self.paths.database)?;
        let mut statement = conn.prepare(
            "SELECT d.id,m.target_id,d.gmail_draft_id,d.gmail_message_id,d.recipient,d.subject,
                    d.cv_path,m.remote_verified,d.created_at
             FROM gmail_drafts d JOIN gmail_draft_targets m ON m.gmail_draft_record_id=d.id
             WHERE m.target_id=?1 ORDER BY d.created_at DESC",
        )?;
        Ok(statement.query_map([target_id], |row| {
            let message_id: Option<String> = row.get(3)?;
            let draft_id: String = row.get(2)?;
            let locator = message_id.clone().unwrap_or_else(|| draft_id.clone());
            Ok(GmailDraftInfo {
                id: row.get(0)?, target_id: row.get(1)?, gmail_draft_id: draft_id,
                gmail_message_id: message_id, recipient: row.get(4)?, subject: row.get(5)?,
                cv_path: row.get(6)?, remote_verified: row.get::<_,i64>(7)? != 0,
                created_at: row.get(8)?,
                gmail_url: if locator.starts_with("unconfirmed:") { "https://mail.google.com/mail/u/0/#drafts".into() } else { format!("https://mail.google.com/mail/u/0/#drafts/{locator}") },
            })
        })?.collect::<std::result::Result<Vec<_>,_>>()?)
    }

    async fn valid_access_token(&self) -> Result<String> {
        let raw = secrets::get_secret(TOKEN_SECRET_REF)?.context("请先连接 Gmail")?;
        let mut token: OAuthToken = serde_json::from_str(&raw).context("CareerOS 凭据文件中的 Gmail 凭据无效")?;
        let valid_until = token.expires_at.as_deref().and_then(|value| DateTime::parse_from_rfc3339(value).ok()).map(|value| value.with_timezone(&Utc));
        if valid_until.map(|value| value > Utc::now() + ChronoDuration::seconds(60)).unwrap_or(false) && !token.access_token.is_empty() {
            return Ok(token.access_token);
        }
        let refresh = token.refresh_token.clone().context("Gmail refresh token 缺失，请重新连接")?;
        let client = load_client()?;
        let endpoint = client.token_uri.as_deref().unwrap_or("https://oauth2.googleapis.com/token");
        let mut form = vec![("client_id",client.client_id.as_str()),("refresh_token",refresh.as_str()),("grant_type","refresh_token")];
        if let Some(secret)=client.client_secret.as_deref(){form.push(("client_secret",secret));}
        let response = self.http.post(endpoint).form(&form).send().await?;
        if !response.status().is_success(){bail!("Gmail 凭据刷新失败：{}",response.text().await?)}
        let exchange: TokenExchange = response.json().await?;
        token.access_token=exchange.access_token;
        token.expires_at=exchange.expires_in.map(|seconds|(Utc::now()+ChronoDuration::seconds(seconds)).to_rfc3339());
        if exchange.refresh_token.is_some(){token.refresh_token=exchange.refresh_token;}
        secrets::set_secret(TOKEN_SECRET_REF,&serde_json::to_string(&token)?)?;
        Ok(token.access_token)
    }

    async fn fetch_profile_with_token(&self, access_token: &str) -> Result<Value> {
        let response = self.http.get("https://gmail.googleapis.com/gmail/v1/users/me/profile").bearer_auth(access_token).send().await?;
        if !response.status().is_success(){bail!("无法核验 Gmail 账号：{}",response.text().await?)}
        Ok(response.json().await?)
    }
}

fn load_client() -> Result<OAuthClient> {
    let raw=secrets::get_secret(CLIENT_SECRET_REF)?.context("请先导入 Google OAuth 桌面客户端 JSON")?;
    let parsed:ClientFile=serde_json::from_str(&raw)?;
    parsed.installed.context("OAuth 客户端必须是桌面应用类型")
}

fn save_account(path:&Path,email:&str)->Result<()> {
    let conn=db::connect(path)?;
    conn.execute("INSERT INTO gmail_accounts(id,email,oauth_client_keychain_ref,oauth_token_keychain_ref,verified_at) VALUES('gmail-primary',?1,?2,?3,strftime('%Y-%m-%dT%H:%M:%SZ','now')) ON CONFLICT(id) DO UPDATE SET email=excluded.email,verified_at=excluded.verified_at",params![email,CLIENT_SECRET_REF,TOKEN_SECRET_REF])?;
    Ok(())
}

fn record_oauth_state(path:&Path,status:&str,message:Option<&str>)->Result<()> {
    let conn=db::connect(path)?;
    conn.execute("UPDATE gmail_oauth_state SET status=?1,message=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id=1",params![status,message])?;
    Ok(())
}

struct ApprovedAttachment { path:String,hash:String,bytes:Vec<u8> }
struct DraftRequest { record_id:String,target_id:String,application_id:String,recipient:String,subject:String,cv_path:String }

fn approved_attachment(paths:&AppPaths,conn:&rusqlite::Connection,target_id:&str)->Result<ApprovedAttachment> {
    let (path,hash):(String,String)=conn.query_row(
        "SELECT p.artifact_path,p.approved_sha256 FROM target_artifact_approvals p
         JOIN contact_target_artifacts a ON a.target_id=p.target_id AND a.artifact_type=p.artifact_type AND a.language=p.language AND a.path=p.artifact_path
         WHERE p.target_id=?1 AND p.artifact_type='cv_pdf' AND p.language='en'",[target_id],|r|Ok((r.get(0)?,r.get(1)?)),
    ).context("请先在 CV 栏目审核并批准当前 PDF")?;
    let bytes=std::fs::read(resolve_data_path(&paths.data_root,&path))?;
    if format!("{:x}",Sha256::digest(&bytes))!=hash { bail!("CV 在批准后发生变化，请重新审核") }
    Ok(ApprovedAttachment {path,hash,bytes})
}

fn reserve_draft(path:&Path,request:&DraftRequest)->Result<Option<String>> {
    let conn=db::connect(path)?;
    let tx=db::publication_transaction(&conn)?;
    let existing:Option<String>=tx.query_row("SELECT gmail_draft_id FROM gmail_drafts WHERE id=?1",[&request.record_id],|r|r.get(0)).optional()?;
    if let Some(id)=existing {return Ok(Some(id))}
    tx.execute("INSERT INTO gmail_drafts(id,application_id,gmail_draft_id,recipient,subject,cv_path) VALUES(?1,?2,?3,?4,?5,?6)",
        params![request.record_id,request.application_id,format!("unconfirmed:{}",request.record_id),request.recipient,request.subject,request.cv_path])?;
    tx.execute("INSERT INTO gmail_draft_targets(gmail_draft_record_id,target_id,remote_verified) VALUES(?1,?2,0)",params![request.record_id,request.target_id])?;
    tx.commit()?;
    Ok(None)
}

fn url_path_segment(value:&str)->String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn valid_email(value:&str)->bool {
    let value=value.trim();
    let Some((local,domain))=value.split_once('@') else{return false};
    !local.is_empty() && domain.contains('.') && !value.contains(char::is_whitespace)
}

fn build_mime(recipient:&str,subject:&str,body:&str,cv_bytes:&[u8])->Result<String> {
    if subject.contains(['\r','\n']) || !valid_email(recipient) { bail!("邮件头包含无效字符") }
    let boundary=format!("careeros-{}",Uuid::new_v4().simple());
    let filename="cv.pdf";
    let encoded_subject=if subject.is_ascii(){subject.to_owned()}else{format!("=?UTF-8?B?{}?=",STANDARD.encode(subject.as_bytes()))};
    let attachment=STANDARD.encode(cv_bytes);
    let wrapped=attachment.as_bytes().chunks(76).map(|chunk|String::from_utf8_lossy(chunk).to_string()).collect::<Vec<_>>().join("\r\n");
    let mime=format!(
        "To: {recipient}\r\nSubject: {encoded_subject}\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n--{boundary}\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{}\r\n\r\n--{boundary}\r\nContent-Type: application/pdf; name=\"{filename}\"\r\nContent-Disposition: attachment; filename=\"{filename}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{wrapped}\r\n--{boundary}--\r\n",
        body.replace('\n',"\r\n")
    );
    Ok(URL_SAFE_NO_PAD.encode(mime.as_bytes()))
}

fn resolve_data_path(data_root:&Path,value:&str)->PathBuf {
    let path=PathBuf::from(value);
    if path.is_absolute(){path}else{data_root.join(path)}
}

fn sha256_file(path:&Path)->Result<String>{
    let data=std::fs::read(path).with_context(||format!("无法读取 {}",path.display()))?;
    Ok(format!("{:x}",Sha256::digest(data)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_is_bound_to_preview_and_mime_keeps_the_captured_bytes() -> Result<()> {
        let temp=tempfile::tempdir()?;
        let paths=crate::materials::tests::publication_fixture(temp.path())?;
        let manager=GmailManager::new(paths.clone());
        let pdf=paths.generated.join("cv.pdf");
        let hash=sha256_file(&pdf)?;
        manager.approve_cv("target",&pdf.display().to_string(),&hash)?;
        let conn=db::connect(&paths.database)?;
        let captured=approved_attachment(&paths,&conn,"target")?;
        std::fs::write(&pdf,b"unreviewed replacement")?;
        assert!(manager.approve_cv("target",&pdf.display().to_string(),&hash).is_err());
        assert!(approved_attachment(&paths,&conn,"target").is_err());
        let raw=build_mime("contact@example.org","Subject","Body",&captured.bytes)?;
        let mime=String::from_utf8(URL_SAFE_NO_PAD.decode(raw)?)?;
        assert!(mime.contains(&STANDARD.encode(b"previous approved PDF")));
        assert!(!mime.contains(&STANDARD.encode(b"unreviewed replacement")));
        assert!(build_mime("contact@example.org","Subject\r\nBcc: unwanted@example.org","Body",&captured.bytes).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn remote_id_survives_failed_verification_and_retry_never_posts_again() -> Result<()> {
        let temp=tempfile::tempdir()?;
        let paths=crate::materials::tests::publication_fixture(temp.path())?;
        let listener=TcpListener::bind("127.0.0.1:0").await?;
        let address=listener.local_addr()?;
        let server=tokio::spawn(async move {
            let mut methods=Vec::new();
            for (status,body) in [("200 OK",r#"{"id":"draft-one","message":{"id":"message-one"}}"#),("503 Service Unavailable","{}"),("200 OK",r#"{"id":"draft-one"}"#)] {
                let (mut socket,_)=listener.accept().await?;
                let mut bytes=Vec::new();
                loop {
                    let mut buffer=[0u8;4096];
                    let count=socket.read(&mut buffer).await?;
                    if count==0 { break }
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end)=bytes.windows(4).position(|part|part==b"\r\n\r\n") {
                        let headers=String::from_utf8_lossy(&bytes[..end]);
                        let length=headers.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);
                        if bytes.len()>=end+4+length { break }
                    }
                }
                methods.push(String::from_utf8_lossy(&bytes).split_whitespace().next().unwrap_or("").to_string());
                socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await?;
            }
            Ok::<_,anyhow::Error>(methods)
        });
        let mut manager=GmailManager::new(paths.clone());
        manager.api_base=format!("http://{address}");
        let request=DraftRequest {record_id:"request-one".into(),target_id:"target".into(),application_id:"app".into(),recipient:"contact@example.org".into(),subject:"Subject".into(),cv_path:"generated/cv.pdf".into()};
        let first=manager.publish_draft("local-test-token",&request,"test-mime").await?;
        assert!(!first.remote_verified);
        assert_eq!(first.gmail_draft_id,"draft-one");
        let second=manager.publish_draft("local-test-token",&request,"test-mime").await?;
        assert!(second.remote_verified);
        assert_eq!(server.await??,vec!["POST","GET","GET"]);
        let conn=db::connect(&paths.database)?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM gmail_drafts",[],|r|r.get::<_,i64>(0))?,1);
        let unknown=DraftRequest { record_id:"unknown-request".into(),..request };
        assert!(reserve_draft(&paths.database,&unknown)?.is_none());
        assert!(manager.publish_draft("unused",&unknown,"unused").await.unwrap_err().to_string().contains("不确定"));
        Ok(())
    }

    #[test]
    fn mime_has_attachment_and_no_send_operation(){
        let dir=tempfile::tempdir().unwrap();
        let pdf=dir.path().join("cv.pdf");
        std::fs::write(&pdf,b"%PDF-test").unwrap();
        let raw=build_mime("pi@example.edu","测试 subject","Hello",&std::fs::read(&pdf).unwrap()).unwrap();
        let decoded=URL_SAFE_NO_PAD.decode(raw).unwrap();
        let text=String::from_utf8(decoded).unwrap();
        assert!(text.contains("Content-Disposition: attachment"));
        assert!(text.contains("pi@example.edu"));
        assert!(!text.to_lowercase().contains("drafts/send"));
    }

    #[test]
    fn recipient_validation_is_conservative(){
        assert!(valid_email("pi@example.edu"));
        assert!(!valid_email("wrong person"));
    }
}
