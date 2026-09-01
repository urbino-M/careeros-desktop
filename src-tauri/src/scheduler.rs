use crate::codex::{CodexManager, CodexTaskRequest};
use crate::db;
use crate::materials;
use crate::models::JobSummary;
use crate::paths::AppPaths;
use crate::typst;
use crate::workflows;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Notify, Semaphore};
use uuid::Uuid;

const CAPACITY: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueRequest {
    pub job_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub prompt: Option<String>,
    pub payload: Option<Value>,
    pub provider_id: Option<String>,
    pub account_id: Option<String>,
    pub model_id: Option<String>,
    pub reasoning: Option<String>,
    pub thread_id: Option<String>,
}

pub struct Scheduler {
    db_path: PathBuf,
    paths: AppPaths,
    semaphore: Arc<Semaphore>,
    notify: Arc<Notify>,
    shutdown: Arc<AtomicBool>,
    codex: Arc<CodexManager>,
    app: Option<AppHandle>,
}

impl Scheduler {
    pub fn new(paths: AppPaths, codex: Arc<CodexManager>, app: Option<AppHandle>) -> Self {
        Self {
            db_path: paths.database.clone(),
            paths,
            semaphore: Arc::new(Semaphore::new(CAPACITY)),
            notify: Arc::new(Notify::new()),
            shutdown: Arc::new(AtomicBool::new(false)),
            codex,
            app,
        }
    }

    pub fn start(self: &Arc<Self>) -> Result<()> {
        self.shutdown.store(false, Ordering::SeqCst);
        recover_interrupted_jobs(&self.db_path)?;
        let scheduler = self.clone();
        tauri::async_runtime::spawn(async move {
            scheduler.dispatch_loop().await;
        });
        Ok(())
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn enqueue(&self, request: EnqueueRequest) -> Result<String> {
        let conn = db::connect(&self.db_path)?;
        let (provider_id, account_id, model_id, reasoning) = resolve_model_snapshot(&conn, &request)?;
        let id = format!("job-native-{}", Uuid::new_v4().simple());
        let mut payload = request.payload.unwrap_or_else(|| json!({}));
        if !payload.is_object() {
            payload = json!({"value":payload});
        }
        if let Some(prompt) = request.prompt {
            payload["prompt"] = Value::String(prompt);
        }
        conn.execute(
            "INSERT INTO native_jobs(
                id, job_type, target_type, target_id, status, progress, message,
                provider_id, account_id, model_id, reasoning, thread_id, payload_json
             ) VALUES(?1,?2,?3,?4,'queued',0,'等待执行',?5,?6,?7,?8,?9,?10)",
            params![
                id,
                request.job_type,
                request.target_type,
                request.target_id,
                provider_id,
                account_id,
                model_id,
                reasoning,
                request.thread_id,
                serde_json::to_string(&payload)?,
            ],
        )?;
        insert_event(&conn, &id, "queued", Some(0), "任务已加入队列", json!({}))?;
        self.notify.notify_one();
        self.emit_changed();
        Ok(id)
    }

    pub async fn cancel(&self, job_id: &str) -> Result<()> {
        let conn = db::connect(&self.db_path)?;
        let changed = conn.execute(
            "UPDATE native_jobs
             SET cancel_requested=1,
                 status=CASE WHEN status='queued' THEN 'cancelled' ELSE status END,
                 message=CASE WHEN status='queued' THEN '已取消' ELSE '正在取消' END,
                 finished_at=CASE WHEN status='queued' THEN strftime('%Y-%m-%dT%H:%M:%SZ','now') ELSE finished_at END,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE id=?1 AND status IN ('queued','running')",
            [job_id],
        )?;
        if changed == 0 {
            bail!("任务不存在，或当前状态不可取消")
        }
        insert_event(&conn, job_id, "cancel_requested", None, "已请求取消", json!({}))?;
        let running: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT j.thread_id,r.turn_id
                 FROM native_jobs j LEFT JOIN native_job_runtime r ON r.job_id=j.id
                 WHERE j.id=?1 AND j.status='running' AND j.thread_id IS NOT NULL",
                [job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        drop(conn);
        if let Some((thread_id, turn_id)) = running {
            let _ = self.codex.interrupt(&thread_id, turn_id.as_deref()).await;
        }
        self.notify.notify_waiters();
        self.emit_changed();
        Ok(())
    }

    pub fn retry(&self, job_id: &str) -> Result<()> {
        let conn = db::connect(&self.db_path)?;
        let (job_type, payload_raw): (String, String) = conn.query_row(
            "SELECT job_type,payload_json FROM native_jobs WHERE id=?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).context("任务不存在")?;
        let mut payload: Value = serde_json::from_str(&payload_raw)?;
        let reuse_output = has_reusable_output(&self.paths, job_id, &job_type);
        if let Some(object) = payload.as_object_mut() {
            if reuse_output { object.insert("_reuseExistingOutput".into(), Value::Bool(true)); }
            else { object.remove("_reuseExistingOutput"); }
        }
        let message = if reuse_output { "等待重新导入已有结果" } else { "等待重试" };
        let changed = conn.execute(
            "UPDATE native_jobs
             SET status='queued', progress=0, message=?2, error=NULL, payload_json=?3,
                 cancel_requested=0, attempt=attempt+1, started_at=NULL, finished_at=NULL,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE id=?1 AND status IN ('failed','cancelled','needs_review','completed')",
            params![job_id,message,serde_json::to_string(&payload)?],
        )?;
        if changed == 0 {
            bail!("任务不存在，或当前状态不可重试")
        }
        insert_event(
            &conn,
            job_id,
            "retried",
            Some(0),
            if reuse_output { "任务已重新加入队列；将直接校验并导入已有结果" } else { "任务已重新加入队列；将恢复原线程" },
            json!({"reuseExistingOutput":reuse_output}),
        )?;
        self.notify.notify_one();
        self.emit_changed();
        Ok(())
    }

    pub fn approve(&self, job_id: &str) -> Result<()> {
        let conn = db::connect(&self.db_path)?;
        let changed = conn.execute(
            "UPDATE native_jobs
             SET status='completed', message='结果已审核',
                 finished_at=COALESCE(finished_at,strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                 updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE id=?1 AND status='needs_review'",
            [job_id],
        )?;
        if changed != 1 {
            bail!("任务不存在，或当前不在待审核状态")
        }
        insert_event(&conn, job_id, "completed", Some(100), "用户已确认审核结果", json!({}))?;
        self.emit_changed();
        Ok(())
    }

    async fn dispatch_loop(self: Arc<Self>) {
        while !self.shutdown.load(Ordering::SeqCst) {
            let mut dispatched = false;
            while !self.shutdown.load(Ordering::SeqCst) {
                let Ok(permit) = self.semaphore.clone().try_acquire_owned() else {
                    break;
                };
                let next = match claim_next_job(&self.db_path) {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!("PostdocOS scheduler claim failed: {error:#}");
                        drop(permit);
                        break;
                    }
                };
                let Some(job) = next else {
                    drop(permit);
                    break;
                };
                dispatched = true;
                self.emit_changed();
                let scheduler = self.clone();
                tauri::async_runtime::spawn(async move {
                    let _permit = permit;
                    scheduler.execute_job(job).await;
                    scheduler.notify.notify_one();
                });
            }
            if !dispatched && !self.shutdown.load(Ordering::SeqCst) {
                tokio::select! {
                    _ = self.notify.notified() => {},
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                }
            }
        }
    }

    async fn execute_job(&self, job: JobSummary) {
        let result = self.execute_job_inner(&job).await;
        let finish_result = match result {
            Ok(result) => finish_job(&self.db_path, &job.id, "needs_review", "执行完成，请审核结果", Some(result), None),
            Err(error) if error.to_string() == "cancelled" => {
                finish_job(&self.db_path, &job.id, "cancelled", "已取消", None, None)
            }
            Err(error) => finish_job(
                &self.db_path,
                &job.id,
                "failed",
                "任务失败",
                None,
                Some(format!("{error:#}")),
            ),
        };
        if let Err(error) = finish_result {
            eprintln!("PostdocOS scheduler finish failed: {error:#}");
        }
        self.emit_changed();
    }

    async fn execute_job_inner(&self, job: &JobSummary) -> Result<Value> {
        let payload = load_payload(&self.db_path, &job.id)?;
        if let Some(delay) = payload.get("testDelayMs").and_then(Value::as_u64) {
            return self.run_test_delay(job, delay).await;
        }
        if job.job_type == "preference_rebuild" {
            update_progress(&self.db_path, &job.id, 20, "正在汇总自动记录的修订偏好")?;
            let result = workflows::rebuild_preferences(&self.paths)?;
            update_progress(&self.db_path, &job.id, 100, "偏好已重建，等待查看摘要")?;
            return Ok(result);
        }
        let prompt = payload
            .get("prompt")
            .and_then(Value::as_str)
            .context("任务缺少 Agent 指令")?;
        let workspace = self.paths.workspaces.join(&job.id);
        std::fs::create_dir_all(&workspace)?;
        if payload.get("_reuseExistingOutput").and_then(Value::as_bool) == Some(true) {
            let result = self.import_existing_output(job, &payload, &workspace).await?;
            clear_reuse_output_flag(&self.db_path, &job.id)?;
            return Ok(result)
        }
        let output = workspace.join("output");
        if output.exists() {
            std::fs::remove_dir_all(&output)?;
        }
        update_progress(&self.db_path, &job.id, 4, "正在准备独立任务工作区")?;
        let contract = if matches!(job.job_type.as_str(), "revision_request" | "material_revision") {
            let target_id = job.target_id.as_deref().context("材料修订缺少联系人 ID")?;
            let artifact_type = payload
                .get("artifactType")
                .and_then(Value::as_str)
                .context("材料修订缺少材料类型")?;
            materials::prepare_revision_workspace(
                &self.paths,
                &workspace,
                target_id,
                artifact_type,
                payload.get("instruction").and_then(Value::as_str),
            )?
        } else {
            materials::prepare_general_workspace(
                &self.paths,
                &workspace,
                job.target_id.as_deref(),
                &job.job_type,
                &payload,
            )?
        };
        update_progress(&self.db_path, &job.id, 8, "正在启动 Codex")?;
        let (checkpoint_tx, mut checkpoint_rx) = mpsc::unbounded_channel();
        let run = self.codex.run_task(CodexTaskRequest {
                prompt: format!("{prompt}{contract}"),
                workspace: workspace.clone(),
                model: job
                    .model_id
                    .as_deref()
                    .unwrap_or("openai:gpt-5.6-sol")
                    .trim_start_matches("openai:")
                    .to_owned(),
                reasoning: job.reasoning.clone().unwrap_or_else(|| "xhigh".into()),
                thread_id: job.thread_id.clone(),
            }, Some(checkpoint_tx));
        tokio::pin!(run);
        let result = loop {
            tokio::select! {
                outcome = &mut run => break outcome?,
                checkpoint = checkpoint_rx.recv() => {
                    if let Some(checkpoint) = checkpoint {
                        save_runtime_checkpoint(
                            &self.db_path,
                            &job.id,
                            &checkpoint.thread_id,
                            checkpoint.turn_id.as_deref(),
                        )?;
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(250)) => {
                    if is_cancel_requested(&self.db_path, &job.id)? {
                        if let Some((thread_id, turn_id)) = load_runtime_checkpoint(&self.db_path, &job.id)? {
                            let _ = self.codex.interrupt(&thread_id, turn_id.as_deref()).await;
                        }
                        bail!("cancelled")
                    }
                }
            }
        };
        save_runtime_checkpoint(
            &self.db_path,
            &job.id,
            &result.thread_id,
            result.turn_id.as_deref(),
        )?;
        let mut output_result = json!({
            "threadId":result.thread_id,
            "turnId":result.turn_id,
            "finalEvent":result.final_event
        });
        if matches!(job.job_type.as_str(), "revision_request" | "material_revision") {
            update_progress(&self.db_path, &job.id, 92, "正在校验并保存修订版本")?;
            let target_id = job.target_id.as_deref().context("材料修订缺少联系人 ID")?;
            let artifact_type = payload.get("artifactType").and_then(Value::as_str).context("材料修订缺少材料类型")?;
            let revision = materials::apply_agent_revision(
                &self.paths,
                &workspace,
                target_id,
                artifact_type,
                &job.id,
                &job.provider_id,
                job.model_id.as_deref(),
                job.reasoning.as_deref(),
                payload.get("instruction").and_then(Value::as_str),
            )?;
            output_result["revision"] = serde_json::to_value(revision)?;
            if artifact_type == "cv_data" {
                update_progress(&self.db_path, &job.id, 96, "正在用内置 Typst 重新生成 PDF")?;
                match typst::generate_cv(&self.paths, target_id).await {
                    Ok(result) => output_result["typst"] = serde_json::to_value(result)?,
                    Err(error) => output_result["typstWarning"] = Value::String(format!("修订已保存，PDF 待重新生成：{error:#}")),
                }
            } else if artifact_type == "cover_letter_text" {
                update_progress(&self.db_path, &job.id, 96, "正在重新排版 Cover Letter PDF")?;
                match crate::cover_letter::regenerate_from_text(&self.paths, target_id).await {
                    Ok(result) => output_result["coverLetter"] = serde_json::to_value(result)?,
                    Err(error) => output_result["coverLetterWarning"] = Value::String(format!("正文修订已保存，Cover Letter PDF 待重新生成：{error:#}")),
                }
            }
        } else {
            update_progress(&self.db_path, &job.id, 92, "正在校验并导入业务结果")?;
            output_result["businessResult"] = workflows::import_job_result(
                &self.paths,
                job,
                &payload,
                &workspace,
            ).await?;
        }
        update_progress(&self.db_path, &job.id, 100, "Codex 已完成，等待审核")?;
        Ok(output_result)
    }

    async fn import_existing_output(&self, job:&JobSummary, payload:&Value, workspace:&Path) -> Result<Value> {
        update_progress(&self.db_path, &job.id, 92, "正在重新校验并导入已有结果")?;
        let mut output_result = json!({"recoveredExistingOutput":true});
        if matches!(job.job_type.as_str(), "revision_request" | "material_revision") {
            let target_id = job.target_id.as_deref().context("材料修订缺少联系人 ID")?;
            let artifact_type = payload.get("artifactType").and_then(Value::as_str).context("材料修订缺少材料类型")?;
            let revision = materials::apply_agent_revision(
                &self.paths,workspace,target_id,artifact_type,&job.id,&job.provider_id,
                job.model_id.as_deref(),job.reasoning.as_deref(),payload.get("instruction").and_then(Value::as_str),
            )?;
            output_result["revision"] = serde_json::to_value(revision)?;
            if artifact_type == "cv_data" {
                update_progress(&self.db_path, &job.id, 96, "正在用内置 Typst 重新生成 PDF")?;
                match typst::generate_cv(&self.paths,target_id).await {
                    Ok(result) => output_result["typst"] = serde_json::to_value(result)?,
                    Err(error) => output_result["typstWarning"] = Value::String(format!("修订已保存，PDF 待重新生成：{error:#}")),
                }
            } else if artifact_type == "cover_letter_text" {
                update_progress(&self.db_path, &job.id, 96, "正在重新排版 Cover Letter PDF")?;
                match crate::cover_letter::regenerate_from_text(&self.paths,target_id).await {
                    Ok(result) => output_result["coverLetter"] = serde_json::to_value(result)?,
                    Err(error) => output_result["coverLetterWarning"] = Value::String(format!("正文修订已保存，Cover Letter PDF 待重新生成：{error:#}")),
                }
            }
        } else {
            output_result["businessResult"] = workflows::import_job_result(&self.paths,job,payload,workspace).await?;
        }
        update_progress(&self.db_path, &job.id, 100, "已有结果已恢复，等待审核")?;
        Ok(output_result)
    }

    async fn run_test_delay(&self, job: &JobSummary, delay_ms: u64) -> Result<Value> {
        let steps = 20_u64;
        for index in 0..steps {
            tokio::time::sleep(Duration::from_millis((delay_ms / steps).max(5))).await;
            if is_cancel_requested(&self.db_path, &job.id)? {
                bail!("cancelled")
            }
            let progress = (((index + 1) * 100) / steps) as i64;
            update_progress(&self.db_path, &job.id, progress, "正在执行并发验证")?;
        }
        Ok(json!({"test":true,"durationMs":delay_ms}))
    }

    fn emit_changed(&self) {
        if let Some(app) = &self.app {
            let _ = app.emit("postdocos://jobs-changed", ());
        }
    }
}

fn has_reusable_output(paths:&AppPaths, job_id:&str, job_type:&str) -> bool {
    let output = paths.workspaces.join(job_id).join("output");
    match job_type {
        "revision_request" | "material_revision" => {
            output.join("change-set.json").is_file()
                && fs::read_dir(&output).ok().into_iter().flatten().flatten()
                    .any(|entry| entry.path().is_file() && entry.file_name().to_string_lossy() != "change-set.json")
        }
        "internship_search" => output.join("internship-search-results.json").is_file(),
        "full_run" | "full_search" | "research_pi" => output.join("search-results.json").is_file(),
        "reply_followup" => output.join("reply-followup.json").is_file(),
        "checklist_refresh" => output.join("checklist.json").is_file(),
        "follow_up_scan" => output.join("follow-up-scan.json").is_file(),
        "opportunity_health" | "pi_verification" => output.join("verification.json").is_file(),
        _ => false,
    }
}

fn clear_reuse_output_flag(path:&Path, job_id:&str) -> Result<()> {
    let conn = db::connect(path)?;
    let raw:String = conn.query_row("SELECT payload_json FROM native_jobs WHERE id=?1",[job_id],|row|row.get(0))?;
    let mut payload:Value = serde_json::from_str(&raw)?;
    if let Some(object) = payload.as_object_mut() { object.remove("_reuseExistingOutput"); }
    conn.execute("UPDATE native_jobs SET payload_json=?2 WHERE id=?1",params![job_id,serde_json::to_string(&payload)?])?;
    Ok(())
}

fn resolve_model_snapshot(
    conn: &rusqlite::Connection,
    request: &EnqueueRequest,
) -> Result<(String, Option<String>, String, String)> {
    let default_type = match request.job_type.as_str() {
        "internship_search" => "full_search",
        "full_run" | "full_search" => "full_search",
        "research_pi" => "research_pi",
        "revision_request" | "material_revision" => "material_revision",
        "reply_followup" => "reply_followup",
        _ => "maintenance",
    };
    let defaults: (String, Option<String>, String, String) = conn.query_row(
        "SELECT provider_id, account_id, model_id, reasoning
         FROM task_model_defaults WHERE task_type=?1",
        [default_type],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let provider_id = request.provider_id.clone().unwrap_or(defaults.0);
    let account_id = request.account_id.clone().or(defaults.1);
    let model_id = request.model_id.clone().unwrap_or(defaults.2);
    let reasoning = request.reasoning.clone().unwrap_or(defaults.3);
    let enabled: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM model_providers p JOIN provider_models m ON m.provider_id=p.id
            WHERE p.id=?1 AND m.id=?2 AND p.enabled=1 AND m.enabled=1
         )",
        params![provider_id, model_id],
        |row| row.get(0),
    )?;
    if !enabled {
        bail!("所选服务商或模型尚未启用")
    }
    Ok((provider_id, account_id, model_id, reasoning))
}

fn recover_interrupted_jobs(path: &Path) -> Result<()> {
    let conn = db::connect(path)?;
    conn.execute(
        "UPDATE native_jobs
         SET status='queued', message='应用重启，正在恢复原线程',
             started_at=NULL, updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE status='running'",
        [],
    )?;
    Ok(())
}

fn claim_next_job(path: &Path) -> Result<Option<JobSummary>> {
    let mut conn = db::connect(path)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let id: Option<String> = tx
        .query_row(
            "SELECT id FROM native_jobs WHERE status='queued' AND cancel_requested=0 ORDER BY created_at, id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(id) = id else {
        tx.commit()?;
        return Ok(None);
    };
    let changed = tx.execute(
        "UPDATE native_jobs
         SET status='running', progress=1, message='正在启动',
             started_at=COALESCE(started_at,strftime('%Y-%m-%dT%H:%M:%SZ','now')),
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND status='queued'",
        [&id],
    )?;
    if changed != 1 {
        tx.commit()?;
        return Ok(None);
    }
    tx.execute(
        "INSERT INTO native_job_events(job_id,event_type,progress,message) VALUES(?1,'running',1,'开始执行')",
        [&id],
    )?;
    let job = tx.query_row(
        "SELECT id, job_type, target_id, status, progress, message, provider_id,
                account_id, model_id, reasoning, thread_id, error, created_at, started_at, finished_at
         FROM native_jobs WHERE id=?1",
        [&id],
        |row| {
            Ok(JobSummary {
                id: row.get(0)?, job_type: row.get(1)?, target_id: row.get(2)?,
                result_target_ids: Vec::new(),
                status: row.get(3)?, progress: row.get(4)?, message: row.get(5)?,
                provider_id: row.get(6)?, account_id: row.get(7)?, model_id: row.get(8)?, reasoning: row.get(9)?,
                thread_id: row.get(10)?, error: row.get(11)?, created_at: row.get(12)?,
                started_at: row.get(13)?, finished_at: row.get(14)?,
            })
        },
    )?;
    tx.commit()?;
    Ok(Some(job))
}

fn load_payload(path: &Path, job_id: &str) -> Result<Value> {
    let conn = db::connect(path)?;
    let payload: String = conn.query_row("SELECT payload_json FROM native_jobs WHERE id=?1", [job_id], |row| row.get(0))?;
    Ok(serde_json::from_str(&payload)?)
}

fn is_cancel_requested(path: &Path, job_id: &str) -> Result<bool> {
    let conn = db::connect(path)?;
    Ok(conn.query_row("SELECT cancel_requested FROM native_jobs WHERE id=?1", [job_id], |row| row.get::<_, i64>(0))? != 0)
}

fn update_progress(path: &Path, job_id: &str, progress: i64, message: &str) -> Result<()> {
    let conn = db::connect(path)?;
    conn.execute(
        "UPDATE native_jobs SET progress=?2,message=?3,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND status='running'",
        params![job_id, progress.clamp(0, 100), message],
    )?;
    insert_event(&conn, job_id, "progress", Some(progress), message, json!({}))?;
    Ok(())
}

fn save_runtime_checkpoint(
    path: &Path,
    job_id: &str,
    thread_id: &str,
    turn_id: Option<&str>,
) -> Result<()> {
    let conn = db::connect(path)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE native_jobs SET thread_id=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id=?1",
        params![job_id, thread_id],
    )?;
    tx.execute(
        "INSERT INTO native_job_runtime(job_id,turn_id) VALUES(?1,?2)
         ON CONFLICT(job_id) DO UPDATE SET turn_id=excluded.turn_id,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
        params![job_id, turn_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn load_runtime_checkpoint(path: &Path, job_id: &str) -> Result<Option<(String, Option<String>)>> {
    let conn = db::connect(path)?;
    Ok(conn
        .query_row(
            "SELECT j.thread_id,r.turn_id
             FROM native_jobs j LEFT JOIN native_job_runtime r ON r.job_id=j.id
             WHERE j.id=?1 AND j.thread_id IS NOT NULL",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn finish_job(
    path: &Path,
    job_id: &str,
    status: &str,
    message: &str,
    result: Option<Value>,
    error: Option<String>,
) -> Result<()> {
    let conn = db::connect(path)?;
    conn.execute(
        "UPDATE native_jobs
         SET status=?2, progress=CASE WHEN ?2 IN ('needs_review','completed','failed','cancelled') THEN 100 ELSE progress END,
             message=?3, result_json=?4, error=?5,
             finished_at=strftime('%Y-%m-%dT%H:%M:%SZ','now'),
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1",
        params![job_id, status, message, result.map(|value| value.to_string()), error],
    )?;
    insert_event(&conn, job_id, status, Some(100), message, json!({}))?;
    Ok(())
}

fn insert_event(
    conn: &rusqlite::Connection,
    job_id: &str,
    event_type: &str,
    progress: Option<i64>,
    message: &str,
    payload: Value,
) -> Result<()> {
    conn.execute(
        "INSERT INTO native_job_events(job_id,event_type,progress,message,payload_json)
         VALUES(?1,?2,?3,?4,?5)",
        params![job_id, event_type, progress, message, payload.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_paths(temp: &TempDir) -> AppPaths {
        let root = temp.path().to_path_buf();
        AppPaths {
            database: root.join("database/postdocos.sqlite3"),
            generated: root.join("generated"), profile: root.join("profile"),
            workspaces: root.join("workspaces"), codex_home: root.join("codex"),
            backups: root.join("backups"), cache: root.join("cache"), logs: root.join("logs"),
            runtime: root.join("runtime"),
            data_root: root,
        }
    }

    fn setup_test_scheduler(temp: &TempDir) -> Result<Arc<Scheduler>> {
        let paths = test_paths(temp);
        paths.ensure()?;
        let conn = db::connect(&paths.database)?;
        conn.execute_batch(include_str!("../../../postdoc-os/postdoc_os/schema.sql"))?;
        conn.execute_batch(include_str!("../migrations/0008_native_desktop.sql"))?;
        drop(conn);
        let codex = Arc::new(CodexManager::new(paths.clone()));
        Ok(Arc::new(Scheduler::new(paths, codex, None)))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn six_jobs_run_as_five_plus_one() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let paths = scheduler.paths.clone();
        scheduler.start()?;
        for _ in 0..6 {
            scheduler.enqueue(EnqueueRequest {
                job_type: "test_delay".into(), target_type: None, target_id: None,
                prompt: None, payload: Some(json!({"testDelayMs":1200})), provider_id: None,
                account_id: None, model_id: None, reasoning: None, thread_id: None,
            })?;
        }
        tokio::time::sleep(Duration::from_millis(180)).await;
        let conn = db::connect(&paths.database)?;
        let running: i64 = conn.query_row("SELECT COUNT(*) FROM native_jobs WHERE status='running'", [], |row| row.get(0))?;
        let queued: i64 = conn.query_row("SELECT COUNT(*) FROM native_jobs WHERE status='queued'", [], |row| row.get(0))?;
        assert_eq!((running, queued), (5, 1));
        tokio::time::sleep(Duration::from_millis(1400)).await;
        let review_id: String = conn.query_row(
            "SELECT id FROM native_jobs WHERE status='needs_review' LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        drop(conn);
        scheduler.approve(&review_id)?;
        let conn = db::connect(&paths.database)?;
        let approved: String = conn.query_row(
            "SELECT status FROM native_jobs WHERE id=?1",
            [&review_id],
            |row| row.get(0),
        )?;
        assert_eq!(approved, "completed");
        drop(conn);
        let mut active = 1_i64;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let conn = db::connect(&paths.database)?;
            active = conn.query_row(
                "SELECT COUNT(*) FROM native_jobs WHERE status IN ('queued','running')",
                [],
                |row| row.get(0),
            )?;
            if active == 0 {
                break;
            }
        }
        assert_eq!(active, 0, "并发验证任务没有在时限内结束");
        scheduler.shutdown();
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn running_job_can_be_cancelled_and_retried_without_losing_thread() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let paths = scheduler.paths.clone();
        let job_id = scheduler.enqueue(EnqueueRequest {
            job_type: "test_delay".into(),
            target_type: None,
            target_id: None,
            prompt: None,
            payload: Some(json!({"testDelayMs":1200})),
            provider_id: None,
            account_id: None,
            model_id: None,
            reasoning: None,
            thread_id: None,
        })?;
        let claimed = claim_next_job(&paths.database)?.context("测试任务没有进入运行态")?;
        let running_scheduler = scheduler.clone();
        let running_job = tokio::spawn(async move {
            running_scheduler.execute_job(claimed).await;
        });
        tokio::time::sleep(Duration::from_millis(120)).await;
        scheduler.cancel(&job_id).await?;
        running_job.await?;

        let conn = db::connect(&paths.database)?;
        let status: String = conn.query_row(
            "SELECT status FROM native_jobs WHERE id=?1",
            [&job_id],
            |row| row.get(0),
        )?;
        assert_eq!(status, "cancelled");
        drop(conn);

        save_runtime_checkpoint(
            &paths.database,
            &job_id,
            "thread-preserved-on-retry",
            Some("turn-preserved-on-retry"),
        )?;
        scheduler.retry(&job_id)?;
        let conn = db::connect(&paths.database)?;
        let (status, attempt, thread_id): (String, i64, Option<String>) = conn.query_row(
            "SELECT status,attempt,thread_id FROM native_jobs WHERE id=?1",
            [&job_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert!(matches!(status.as_str(), "queued" | "running"));
        assert_eq!(attempt, 1);
        assert_eq!(thread_id.as_deref(), Some("thread-preserved-on-retry"));
        Ok(())
    }

    #[test]
    fn crash_recovery_requeues_job_and_keeps_runtime_checkpoint() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let paths = scheduler.paths.clone();
        let job_id = scheduler.enqueue(EnqueueRequest {
            job_type: "test_delay".into(),
            target_type: None,
            target_id: None,
            prompt: None,
            payload: Some(json!({"testDelayMs":50})),
            provider_id: None,
            account_id: None,
            model_id: None,
            reasoning: None,
            thread_id: Some("thread-before-crash".into()),
        })?;
        let conn = db::connect(&paths.database)?;
        conn.execute(
            "UPDATE native_jobs SET status='running',message='interrupted' WHERE id=?1",
            [&job_id],
        )?;
        drop(conn);
        save_runtime_checkpoint(&paths.database, &job_id, "thread-before-crash", Some("turn-before-crash"))?;

        recover_interrupted_jobs(&paths.database)?;
        let checkpoint = load_runtime_checkpoint(&paths.database, &job_id)?;
        assert_eq!(checkpoint, Some(("thread-before-crash".into(), Some("turn-before-crash".into()))));
        let claimed = claim_next_job(&paths.database)?.context("恢复后的任务没有重新进入队列")?;
        assert_eq!(claimed.id, job_id);
        assert_eq!(claimed.thread_id.as_deref(), Some("thread-before-crash"));
        assert_eq!(claimed.status, "running");
        Ok(())
    }

    #[test]
    fn retry_reuses_complete_agent_output_instead_of_calling_model_again() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let paths = scheduler.paths.clone();
        let job_id = scheduler.enqueue(EnqueueRequest {
            job_type:"checklist_refresh".into(),target_type:Some("contact_target".into()),target_id:Some("target-test".into()),
            prompt:Some("refresh".into()),payload:Some(json!({})),provider_id:None,account_id:None,model_id:None,reasoning:None,thread_id:None,
        })?;
        let output = paths.workspaces.join(&job_id).join("output");
        fs::create_dir_all(&output)?;
        fs::write(output.join("checklist.json"), br#"{"schemaVersion":1,"items":[]}"#)?;
        let conn = db::connect(&paths.database)?;
        conn.execute("UPDATE native_jobs SET status='failed',error='semantic import error' WHERE id=?1",[&job_id])?;
        drop(conn);
        scheduler.retry(&job_id)?;
        let conn = db::connect(&paths.database)?;
        let (payload,message):(String,String)=conn.query_row(
            "SELECT payload_json,message FROM native_jobs WHERE id=?1",[&job_id],|row|Ok((row.get(0)?,row.get(1)?)),
        )?;
        assert_eq!(serde_json::from_str::<Value>(&payload)?["_reuseExistingOutput"], true);
        assert!(message.contains("重新导入"));
        Ok(())
    }
}
