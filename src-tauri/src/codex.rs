use crate::paths::AppPaths;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

#[derive(Debug, Clone)]
pub struct CodexTaskRequest {
    pub prompt: String,
    pub workspace: PathBuf,
    pub model: String,
    pub reasoning: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CodexTaskResult {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub final_event: Value,
}

#[derive(Debug, Clone)]
pub struct CodexTaskCheckpoint {
    pub thread_id: String,
    pub turn_id: Option<String>,
}

pub struct CodexManager {
    paths: AppPaths,
    client: Mutex<Option<Arc<CodexClient>>>,
    login_receivers: Mutex<HashMap<String, broadcast::Receiver<Value>>>,
}

impl CodexManager {
    pub fn new(paths: AppPaths) -> Self {
        Self {
            paths,
            client: Mutex::new(None),
            login_receivers: Mutex::new(HashMap::new()),
        }
    }

    async fn client(&self) -> Result<Arc<CodexClient>> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = Arc::new(CodexClient::start(&self.paths).await?);
        client.initialize().await?;
        *guard = Some(client.clone());
        Ok(client)
    }

    pub async fn account_status(&self) -> Result<Value> {
        self.client().await?.request("account/read", json!({})).await
    }

    pub async fn model_list(&self) -> Result<Value> {
        self.client().await?.request("model/list", json!({})).await
    }

    pub async fn start_chatgpt_login(&self) -> Result<Value> {
        let client = self.client().await?;
        // Subscribe before starting OAuth so a fast browser callback cannot race past us.
        let receiver = client.subscribe();
        let result = client
            .request(
                "account/login/start",
                json!({
                    "type":"chatgpt",
                    "useHostedLoginSuccessPage": true,
                    "appBrand": "chatgpt"
                }),
            )
            .await?;
        if let Some(login_id) = find_key_string(&result, "loginId") {
            self.login_receivers.lock().await.insert(login_id, receiver);
        }
        Ok(result)
    }

    pub async fn wait_for_chatgpt_login(&self, login_id: &str) -> Result<Value> {
        let client = self.client().await?;
        let saved_receiver = self.login_receivers.lock().await.remove(login_id);
        let receiver_was_saved = saved_receiver.is_some();
        let mut events = saved_receiver.unwrap_or_else(|| client.subscribe());
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
        let mut completion_seen = false;

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                bail!("等待 ChatGPT 授权超时。请关闭浏览器授权页后重试")
            }

            // If the app was restarted after the browser callback, account/read is the
            // durable source of truth. During a live login, wait for this login's own
            // completion event before accepting the account to avoid stale-account races.
            if completion_seen || !receiver_was_saved {
                let account = client.request("account/read", json!({})).await?;
                if account_is_chatgpt(&account) {
                    return Ok(account);
                }
            }

            let wait = std::cmp::min(
                deadline.saturating_duration_since(now),
                std::time::Duration::from_millis(800),
            );
            match tokio::time::timeout(wait, events.recv()).await {
                Ok(Ok(event)) => {
                    let method = event.get("method").and_then(Value::as_str).unwrap_or("");
                    if method == "account/login/completed" {
                        if let Some(event_login_id) = find_key_string(&event, "loginId") {
                            if event_login_id != login_id {
                                continue;
                            }
                        }
                        if find_key_bool(&event, "success") == Some(false) {
                            let reason = find_key_string(&event, "error")
                                .unwrap_or_else(|| "ChatGPT 授权未完成".to_owned());
                            bail!("{reason}")
                        }
                        completion_seen = true;
                    } else if method == "account/updated" && completion_seen {
                        let account = client.request("account/read", json!({})).await?;
                        if account_is_chatgpt(&account) {
                            return Ok(account);
                        }
                    }
                }
                Ok(Err(broadcast::error::RecvError::Lagged(_))) | Err(_) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    bail!("Codex 登录事件流已结束，请重新连接")
                }
            }
        }
    }

    pub async fn login_with_api_key(&self, api_key: &str) -> Result<Value> {
        self.client()
            .await?
            .request("account/login/start", json!({"type":"apiKey","apiKey":api_key}))
            .await
    }

    pub async fn logout(&self) -> Result<Value> {
        self.client().await?.request("account/logout", json!({})).await
    }

    pub async fn run_task(
        &self,
        request: CodexTaskRequest,
        checkpoints: Option<mpsc::UnboundedSender<CodexTaskCheckpoint>>,
    ) -> Result<CodexTaskResult> {
        let client = self.client().await?;
        let thread_id = if let Some(thread_id) = request.thread_id {
            let _ = client
                .request("thread/resume", json!({"threadId": thread_id}))
                .await?;
            thread_id
        } else {
            let result = client
                .request(
                    "thread/start",
                    json!({
                        "cwd": request.workspace,
                        "model": request.model,
                        "approvalPolicy": "never",
                        "sandbox": "workspace-write",
                        "experimentalRawEvents": false
                    }),
                )
                .await?;
            find_string(&result, &["thread", "id"])
                .or_else(|| find_string(&result, &["threadId"]))
                .context("Codex 没有返回 thread_id")?
        };

        if let Some(sender) = &checkpoints {
            let _ = sender.send(CodexTaskCheckpoint {
                thread_id: thread_id.clone(),
                turn_id: None,
            });
        }

        let mut events = client.subscribe();
        let turn_result = client
            .request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{"type":"text","text":request.prompt}],
                    "model": request.model,
                    "effort": request.reasoning
                }),
            )
            .await?;
        let turn_id = find_string(&turn_result, &["turn", "id"])
            .or_else(|| find_string(&turn_result, &["turnId"]));
        if let Some(sender) = &checkpoints {
            let _ = sender.send(CodexTaskCheckpoint {
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
            });
        }

        loop {
            let event = events.recv().await.context("Codex 事件流已结束")?;
            if !event_mentions_thread(&event, &thread_id) {
                continue;
            }
            let method = event.get("method").and_then(Value::as_str).unwrap_or("");
            if matches!(method, "turn/completed" | "turn/complete") {
                return Ok(CodexTaskResult {
                    thread_id,
                    turn_id,
                    final_event: event,
                });
            }
            // App Server reports transient transport failures through `error` while it is
            // already reconnecting. Do not turn those retry notifications into failed jobs.
            if method == "error" && find_key_bool(&event, "willRetry") == Some(true) {
                continue;
            }
            if matches!(method, "turn/failed" | "error") {
                bail!("Codex 任务失败：{event}")
            }
        }
    }

    pub async fn interrupt(&self, thread_id: &str, turn_id: Option<&str>) -> Result<()> {
        let mut params = json!({"threadId":thread_id});
        if let Some(turn_id) = turn_id {
            params["turnId"] = Value::String(turn_id.to_owned());
        }
        self.client().await?.request("turn/interrupt", params).await?;
        Ok(())
    }
}

struct CodexClient {
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    events: broadcast::Sender<Value>,
    next_id: AtomicU64,
    _child: Mutex<Child>,
}

impl CodexClient {
    async fn start(paths: &AppPaths) -> Result<Self> {
        let binary = locate_codex_binary(paths)?;
        let mut child = Command::new(&binary)
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .env("CODEX_HOME", &paths.codex_home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("无法启动 Codex App Server：{}", binary.display()))?;
        let stdin = child.stdin.take().context("Codex stdin 不可用")?;
        let stdout = child.stdout.take().context("Codex stdout 不可用")?;
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let reader_pending = pending.clone();
        let (events, _) = broadcast::channel(2048);
        let reader_events = events.clone();

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(id) = value.get("id").and_then(Value::as_u64) {
                    if let Some(sender) = reader_pending.lock().await.remove(&id) {
                        let _ = sender.send(value);
                        continue;
                    }
                }
                let _ = reader_events.send(value);
            }
        });

        Ok(Self {
            stdin: Mutex::new(stdin),
            pending,
            events,
            next_id: AtomicU64::new(1),
            _child: Mutex::new(child),
        })
    }

    async fn initialize(&self) -> Result<()> {
        self.request(
            "initialize",
            json!({
                "clientInfo": {"name":"PostdocOS","title":"PostdocOS","version":env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true}
            }),
        )
        .await?;
        self.notify("initialized", json!({})).await?;
        Ok(())
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        let payload = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        if let Err(error) = self.write(&payload).await {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        let response = tokio::time::timeout(std::time::Duration::from_secs(60), receiver)
            .await
            .context("Codex App Server 响应超时")?
            .context("Codex App Server 连接中断")?;
        if let Some(error) = response.get("error") {
            bail!("Codex App Server 返回错误：{error}")
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.write(&json!({"jsonrpc":"2.0","method":method,"params":params})).await
    }

    async fn write(&self, value: &Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(serde_json::to_string(value)?.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
}

fn locate_codex_binary(paths: &AppPaths) -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("POSTDOCOS_CODEX_BIN") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
    }
    for path in [
        paths.runtime.join("codex"),
        paths.data_root.join("runtime/codex"),
        PathBuf::from("/usr/local/bin/codex"),
        PathBuf::from("/opt/homebrew/bin/codex"),
    ] {
        if path.is_file() {
            return Ok(path);
        }
    }
    bail!("没有找到内置 Codex。开发模式可设置 POSTDOCOS_CODEX_BIN")
}

fn find_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(str::to_owned)
}

fn find_key_string(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(object) => {
            if let Some(value) = object.get(key).and_then(Value::as_str) {
                return Some(value.to_owned());
            }
            object.values().find_map(|value| find_key_string(value, key))
        }
        Value::Array(values) => values.iter().find_map(|value| find_key_string(value, key)),
        _ => None,
    }
}

fn find_key_bool(value: &Value, key: &str) -> Option<bool> {
    match value {
        Value::Object(object) => {
            if let Some(value) = object.get(key).and_then(Value::as_bool) {
                return Some(value);
            }
            object.values().find_map(|value| find_key_bool(value, key))
        }
        Value::Array(values) => values.iter().find_map(|value| find_key_bool(value, key)),
        _ => None,
    }
}

fn account_is_chatgpt(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let direct_match = object
                .get("authMode")
                .or_else(|| object.get("type"))
                .and_then(Value::as_str)
                .is_some_and(|value| matches!(value, "chatgpt" | "chatgpt_oauth"));
            direct_match || object.values().any(account_is_chatgpt)
        }
        Value::Array(values) => values.iter().any(account_is_chatgpt),
        _ => false,
    }
}

fn event_mentions_thread(value: &Value, thread_id: &str) -> bool {
    value.pointer("/params/threadId").and_then(Value::as_str) == Some(thread_id)
        || value.pointer("/params/thread/id").and_then(Value::as_str) == Some(thread_id)
        || value.to_string().contains(thread_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_chatgpt_account_read_result() {
        let account = json!({
            "account": {
                "type": "chatgpt",
                "email": "person@example.com",
                "planType": "plus"
            },
            "requiresOpenaiAuth": false
        });
        assert!(account_is_chatgpt(&account));
    }

    #[test]
    fn reads_login_completion_and_retry_fields_recursively() {
        let completion = json!({
            "method": "account/login/completed",
            "params": {"loginId": "login-1", "success": true}
        });
        let retry = json!({"method": "error", "params": {"willRetry": true}});
        assert_eq!(find_key_string(&completion, "loginId").as_deref(), Some("login-1"));
        assert_eq!(find_key_bool(&completion, "success"), Some(true));
        assert_eq!(find_key_bool(&retry, "willRetry"), Some(true));
    }
}
