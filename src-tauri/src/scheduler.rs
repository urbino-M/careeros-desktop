use crate::codex::{CodexManager, CodexTaskRequest, CodexTaskResult};
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
use sha2::{Digest, Sha256};
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
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const LEASE_DURATION_SECONDS: i64 = 120;
const LEASE_REAP_INTERVAL: Duration = Duration::from_secs(15);
const TIMEOUT_ERROR: &str = "task_timeout";
const MAX_SEARCH_FINALIZATION_TURNS: usize = 2;

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
    worker_id: String,
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
            worker_id: format!("scheduler-{}", Uuid::new_v4().simple()),
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
        let mut conn = db::connect(&self.db_path)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (provider_id, account_id, model_id, reasoning) = resolve_model_snapshot(&tx, &request)?;
        let id = format!("job-native-{}", Uuid::new_v4().simple());
        let mut payload = request.payload.clone().unwrap_or_else(|| json!({}));
        if !payload.is_object() {
            payload = json!({"value":payload});
        }
        if let Some(prompt) = request.prompt.as_ref() {
            payload["prompt"] = Value::String(prompt.clone());
        }
        let active_key = active_key_for(&request, &payload)?;
        if let Some(key) = active_key.as_deref() {
            let existing: Option<String> = tx.query_row(
                "SELECT id FROM native_jobs
                 WHERE active_key=?1 AND status IN ('queued','running')
                 ORDER BY created_at,id LIMIT 1",
                [key],
                |row| row.get(0),
            ).optional()?;
            if let Some(existing) = existing {
                bail!("相同任务已在队列或运行中：{existing}")
            }
        }
        let timeout_seconds = timeout_seconds_for(&request.job_type);
        tx.execute(
            "INSERT INTO native_jobs(
                id, job_type, target_type, target_id, status, progress, message,
                provider_id, account_id, model_id, reasoning, thread_id, payload_json,
                active_key, timeout_seconds
             ) VALUES(?1,?2,?3,?4,'queued',0,'等待执行',?5,?6,?7,?8,?9,?10,?11,?12)",
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
                active_key,
                timeout_seconds,
            ],
        )?;
        insert_event(&tx, &id, "queued", Some(0), "任务已加入队列", json!({
            "activeKey": active_key,
            "timeoutSeconds": timeout_seconds,
        }))?;
        tx.commit()?;
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
        let running: Option<(String, Option<String>, String, Option<String>)> = conn
            .query_row(
                "SELECT j.thread_id,r.turn_id,j.provider_id,j.account_id
                 FROM native_jobs j LEFT JOIN native_job_runtime r ON r.job_id=j.id
                 WHERE j.id=?1 AND j.status='running' AND j.thread_id IS NOT NULL",
                [job_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        drop(conn);
        if let Some((thread_id, turn_id, provider_id, account_id)) = running {
            let _ = self.codex.interrupt(
                &provider_id,
                account_id.as_deref(),
                &thread_id,
                turn_id.as_deref(),
            ).await;
        }
        self.notify.notify_waiters();
        self.emit_changed();
        Ok(())
    }

    pub fn retry(&self, job_id: &str) -> Result<()> {
        let mut conn = db::connect(&self.db_path)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (job_type, payload_raw, active_key, last_error): (String, String, Option<String>, Option<String>) = tx.query_row(
            "SELECT job_type,payload_json,active_key,error FROM native_jobs WHERE id=?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).context("任务不存在")?;
        if let Some(key) = active_key.as_deref() {
            let existing: Option<String> = tx.query_row(
                "SELECT id FROM native_jobs
                 WHERE active_key=?1 AND id<>?2 AND status IN ('queued','running')
                 ORDER BY created_at,id LIMIT 1",
                params![key,job_id],
                |row| row.get(0),
            ).optional()?;
            if let Some(existing) = existing {
                bail!("相同任务已在队列或运行中，不能重试：{existing}")
            }
        }
        let mut payload: Value = serde_json::from_str(&payload_raw)?;
        let reuse_output = has_reusable_output(&self.paths, job_id, &job_type);
        let repair_error = last_error.filter(|error| {
            reuse_output && is_search_job_type(&job_type) && is_cv_preflight_error(error)
        });
        if let Some(object) = payload.as_object_mut() {
            object.remove("_reuseExistingOutput");
            object.remove("_repairExistingOutputError");
            if let Some(error) = repair_error.as_ref() {
                object.insert("_repairExistingOutputError".into(), Value::String(error.clone()));
            } else if reuse_output {
                object.insert("_reuseExistingOutput".into(), Value::Bool(true));
            }
        }
        let message = if repair_error.is_some() {
            "等待定向修复已有检索结果"
        } else if reuse_output {
            "等待重新导入已有结果"
        } else {
            "等待重试"
        };
        let changed = tx.execute(
            "UPDATE native_jobs
             SET status='queued', progress=0, message=?2, error=NULL, payload_json=?3,
                 cancel_requested=0, attempt=attempt+1, started_at=NULL, finished_at=NULL,
                 timeout_at=NULL, heartbeat_at=NULL, lease_owner=NULL, lease_expires_at=NULL,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE id=?1 AND status IN ('failed','cancelled','needs_review','completed')",
            params![job_id,message,serde_json::to_string(&payload)?],
        )?;
        if changed == 0 {
            bail!("任务不存在，或当前状态不可重试")
        }
        insert_event(
            &tx,
            job_id,
            "retried",
            Some(0),
            if reuse_output { "任务已重新加入队列；将直接校验并导入已有结果" } else { "任务已重新加入队列；将恢复原线程" },
            json!({"reuseExistingOutput":reuse_output}),
        )?;
        tx.commit()?;
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
        let mut last_lease_reap = tokio::time::Instant::now() - LEASE_REAP_INTERVAL;
        while !self.shutdown.load(Ordering::SeqCst) {
            if last_lease_reap.elapsed() >= LEASE_REAP_INTERVAL {
                if let Err(error) = reclaim_expired_jobs(&self.db_path, &self.worker_id) {
                    eprintln!("PostdocOS scheduler lease recovery failed: {error:#}");
                }
                last_lease_reap = tokio::time::Instant::now();
            }
            let mut dispatched = false;
            while !self.shutdown.load(Ordering::SeqCst) {
                let Ok(permit) = self.semaphore.clone().try_acquire_owned() else {
                    break;
                };
                let next = match claim_next_job(&self.db_path, &self.worker_id) {
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
        let result = match load_timeout_seconds(&self.db_path, &job.id) {
            Ok(timeout_seconds) => {
                let run = self.execute_job_inner(&job);
                let deadline = tokio::time::sleep(Duration::from_secs(timeout_seconds));
                let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
                heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                tokio::pin!(run);
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        outcome = &mut run => break outcome,
                        _ = &mut deadline => {
                            if let Ok(Some((thread_id, turn_id))) = load_runtime_checkpoint(&self.db_path, &job.id) {
                                let _ = self.codex.interrupt(
                                    &job.provider_id,
                                    job.account_id.as_deref(),
                                    &thread_id,
                                    turn_id.as_deref(),
                                ).await;
                            }
                            break Err(anyhow::anyhow!(TIMEOUT_ERROR));
                        }
                        _ = heartbeat.tick() => {
                            if let Err(error) = refresh_job_lease(&self.db_path, &job.id, &self.worker_id) {
                                break Err(error);
                            }
                        }
                    }
                }
            }
            Err(error) => Err(error),
        };
        let finish_result = match result {
            Ok(result) => finish_job(&self.db_path, &job.id, "needs_review", "执行完成，请审核结果", Some(result), None),
            Err(error) if error.to_string() == "cancelled" => {
                finish_job(&self.db_path, &job.id, "cancelled", "已取消", None, None)
            }
            Err(error) if error.to_string() == TIMEOUT_ERROR => finish_job(
                &self.db_path,
                &job.id,
                "failed",
                "任务超时，已中止",
                None,
                Some("任务超过允许的最长执行时间；可检查范围后重试".into()),
            ),
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
        let is_search_job = is_search_job_type(&job.job_type);
        let repair_existing_output_error = payload
            .get("_repairExistingOutputError")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if payload.get("_reuseExistingOutput").and_then(Value::as_bool) == Some(true) {
            let result = self.import_existing_output(job, &payload, &workspace).await?;
            clear_output_retry_flags(&self.db_path, &job.id)?;
            return Ok(result)
        }
        let output = workspace.join("output");
        if output.exists() && repair_existing_output_error.is_none() {
            std::fs::remove_dir_all(&output)?;
        }
        update_progress(&self.db_path, &job.id, 4, "正在准备独立任务工作区")?;
        let (contract, revision_base_sha256) = if matches!(job.job_type.as_str(), "revision_request" | "material_revision") {
            let target_id = job.target_id.as_deref().context("材料修订缺少联系人 ID")?;
            let artifact_type = payload
                .get("artifactType")
                .and_then(Value::as_str)
                .context("材料修订缺少材料类型")?;
            let prepared = materials::prepare_revision_workspace(
                &self.paths,
                &workspace,
                target_id,
                artifact_type,
                payload.get("instruction").and_then(Value::as_str),
            )?;
            persist_revision_base_sha256(&self.db_path, &job.id, &prepared.base_sha256)?;
            (prepared.prompt_suffix, Some(prepared.base_sha256))
        } else {
            (materials::prepare_general_workspace(
                &self.paths,
                &workspace,
                job.target_id.as_deref(),
                &job.job_type,
                &payload,
            )?, None)
        };
        update_progress(&self.db_path, &job.id, 8, "正在启动 Codex")?;
        let model_slug: String = db::connect(&self.db_path)?.query_row(
            "SELECT model_slug FROM provider_models WHERE id=?1 AND provider_id=?2",
            params![
                job.model_id.as_deref().unwrap_or("openai:gpt-5.6-sol"),
                job.provider_id,
            ],
            |row| row.get(0),
        ).context("任务快照引用的模型不存在")?;
        let job_attempt = load_job_attempt(&self.db_path, &job.id)?;
        let resume_for_finalization = is_search_job
            && job_attempt > 0
            && job.thread_id.is_some()
            && !has_reusable_output(&self.paths, &job.id, &job.job_type);
        let mut finalization_turns = usize::from(resume_for_finalization);
        let initial_prompt = if let Some(error) = repair_existing_output_error.as_deref() {
            update_activity(&self.db_path, &job.id, "正在按本地两页预检结果定向修复 CV")?;
            self.emit_changed();
            search_result_repair_prompt(error)
        } else if resume_for_finalization {
            self.compact_search_thread(job, job.thread_id.as_deref().context("收尾任务缺少原线程 ID")?).await?;
            let message = format!(
                "原线程已有检索证据，正在执行结构化收尾（{finalization_turns}/{MAX_SEARCH_FINALIZATION_TURNS}）"
            );
            update_activity(&self.db_path, &job.id, &message)?;
            self.emit_changed();
            search_finalization_prompt(finalization_turns, MAX_SEARCH_FINALIZATION_TURNS)
        } else {
            format!("{prompt}{contract}")
        };
        let mut result = self.run_codex_turn(
            job,
            &workspace,
            &model_slug,
            initial_prompt,
            job.thread_id.clone(),
        ).await?;

        while is_search_job
            && !has_reusable_output(&self.paths, &job.id, &job.job_type)
            && finalization_turns < MAX_SEARCH_FINALIZATION_TURNS
        {
            finalization_turns += 1;
            self.compact_search_thread(job, &result.thread_id).await?;
            let message = format!(
                "模型已结束检索但尚未交付结果，正在自动收尾（{finalization_turns}/{MAX_SEARCH_FINALIZATION_TURNS}）"
            );
            update_activity(&self.db_path, &job.id, &message)?;
            self.emit_changed();
            result = self.run_codex_turn(
                job,
                &workspace,
                &model_slug,
                search_finalization_prompt(finalization_turns, MAX_SEARCH_FINALIZATION_TURNS),
                Some(result.thread_id.clone()),
            ).await?;
        }
        if is_search_job && !has_reusable_output(&self.paths, &job.id, &job.job_type) {
            bail!(
                "模型连续结束 turn，但没有生成 output/search-results.json；该模型或 Responses API 在长工具链后未完成结构化结果交付"
            )
        }
        let mut output_result = json!({
            "threadId":result.thread_id,
            "turnId":result.turn_id,
            "finalEvent":result.final_event
        });
        if matches!(job.job_type.as_str(), "revision_request" | "material_revision") {
            update_progress(&self.db_path, &job.id, 92, "正在校验并保存修订版本")?;
            let target_id = job.target_id.as_deref().context("材料修订缺少联系人 ID")?;
            let artifact_type = payload.get("artifactType").and_then(Value::as_str).context("材料修订缺少材料类型")?;
            let base_sha256 = revision_base_sha256.as_deref().context("材料修订缺少基线 SHA-256")?;
            let revision = materials::apply_agent_revision(
                &self.paths,
                &workspace,
                target_id,
                artifact_type,
                base_sha256,
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
        clear_output_retry_flags(&self.db_path, &job.id)?;
        Ok(output_result)
    }

    async fn run_codex_turn(
        &self,
        job: &JobSummary,
        workspace: &Path,
        model_slug: &str,
        prompt: String,
        thread_id: Option<String>,
    ) -> Result<CodexTaskResult> {
        let (checkpoint_tx, mut checkpoint_rx) = mpsc::unbounded_channel();
        let (activity_tx, mut activity_rx) = mpsc::unbounded_channel();
        let run = self.codex.run_task(
            CodexTaskRequest {
                prompt,
                workspace: workspace.to_path_buf(),
                provider_id: job.provider_id.clone(),
                account_id: job.account_id.clone(),
                model: model_slug.to_owned(),
                reasoning: job.reasoning.clone().unwrap_or_else(|| "xhigh".into()),
                thread_id,
            },
            Some(checkpoint_tx),
            Some(activity_tx),
        );
        tokio::pin!(run);
        let result = loop {
            tokio::select! {
                biased;
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
                Some(message) = activity_rx.recv() => {
                    update_activity(&self.db_path, &job.id, &message)?;
                    self.emit_changed();
                }
                _ = tokio::time::sleep(Duration::from_millis(250)) => {
                    if is_cancel_requested(&self.db_path, &job.id)? {
                        if let Some((thread_id, turn_id)) = load_runtime_checkpoint(&self.db_path, &job.id)? {
                            let _ = self.codex.interrupt(
                                &job.provider_id,
                                job.account_id.as_deref(),
                                &thread_id,
                                turn_id.as_deref(),
                            ).await;
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
        Ok(result)
    }

    async fn compact_search_thread(&self, job: &JobSummary, thread_id: &str) -> Result<()> {
        update_activity(&self.db_path, &job.id, "检索上下文过长，正在压缩后继续")?;
        self.emit_changed();
        self.codex
            .compact_thread(&job.provider_id, job.account_id.as_deref(), thread_id)
            .await
            .context("无法压缩完整检索线程；该 Responses API 可能不支持 Codex 上下文压缩")?;
        update_activity(&self.db_path, &job.id, "上下文已压缩，正在生成结构化结果")?;
        self.emit_changed();
        Ok(())
    }

    async fn import_existing_output(&self, job:&JobSummary, payload:&Value, workspace:&Path) -> Result<Value> {
        update_progress(&self.db_path, &job.id, 92, "正在重新校验并导入已有结果")?;
        let mut output_result = json!({"recoveredExistingOutput":true});
        if matches!(job.job_type.as_str(), "revision_request" | "material_revision") {
            let target_id = job.target_id.as_deref().context("材料修订缺少联系人 ID")?;
            let artifact_type = payload.get("artifactType").and_then(Value::as_str).context("材料修订缺少材料类型")?;
            let base_sha256 = revision_base_sha256(payload)?;
            let revision = materials::apply_agent_revision(
                &self.paths,workspace,target_id,artifact_type,&base_sha256,&job.id,&job.provider_id,
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

fn revision_base_sha256(payload: &Value) -> Result<String> {
    payload
        .get("_baseSha256")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("材料修订缺少可信基线 SHA-256，请重新发起任务")
}

fn persist_revision_base_sha256(path: &Path, job_id: &str, base_sha256: &str) -> Result<()> {
    let conn = db::connect(path)?;
    let raw: String = conn.query_row(
        "SELECT payload_json FROM native_jobs WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )?;
    let mut payload: Value = serde_json::from_str(&raw)?;
    payload["_baseSha256"] = Value::String(base_sha256.to_owned());
    let changed = conn.execute(
        "UPDATE native_jobs SET payload_json=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND status='running'",
        params![job_id,serde_json::to_string(&payload)?],
    )?;
    if changed != 1 {
        bail!("材料修订任务已不在运行状态，不能记录基线 SHA-256")
    }
    Ok(())
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

fn is_search_job_type(job_type: &str) -> bool {
    matches!(job_type, "full_run" | "full_search" | "research_pi")
}

fn is_cv_preflight_error(error: &str) -> bool {
    error.contains("目标定制 CV 未通过两页预检")
}

fn load_job_attempt(path: &Path, job_id: &str) -> Result<i64> {
    let conn = db::connect(path)?;
    Ok(conn.query_row(
        "SELECT attempt FROM native_jobs WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )?)
}

fn search_finalization_prompt(attempt: usize, total: usize) -> String {
    format!(
        "Your previous turn ended before producing the required structured search result. \
This is finalization turn {attempt} of {total}. Do not call web search, open URLs, or gather any new evidence. \
Use only the evidence already present in this thread and workspace. Finish the task now: write \
output/search-results.json so it conforms exactly to resultContract in POSTDOCOS_TASK.json, include only \
evidence-supported candidates, create all required reviewable package fields, and validate the JSON file before ending. \
The turn is not complete until output/search-results.json exists and is valid."
    )
}

fn search_result_repair_prompt(error: &str) -> String {
    format!(
        "The existing output/search-results.json passed research finalization but failed the local Typst CV preflight:\n{error}\n\n\
Do not call web search, open URLs, or gather new evidence. Repair the existing JSON file in place using only its verified content. \
For every failing contact, preserve target-specific prioritization while making the CV exactly two well-filled pages: keep 36 to 38 distinct \
high-value entries, keep no more than 8 sections where feasible by merging low-priority sections, shorten verbose bodies instead of shrinking typography, \
keep every entry key as a short label of preferably 18 characters or fewer so the key column does not wrap, and remove only the least relevant evidence. \
Keep publications/research outputs and patents before projects, preserve each contact's cvData.authorName so the renderer can bold the candidate's verified author form, \
and preserve verified career-stage wording. Do not change fit scores, sources, or facts to evade validation. Validate output/search-results.json before ending."
    )
}

fn clear_output_retry_flags(path:&Path, job_id:&str) -> Result<()> {
    let conn = db::connect(path)?;
    let raw:String = conn.query_row("SELECT payload_json FROM native_jobs WHERE id=?1",[job_id],|row|row.get(0))?;
    let mut payload:Value = serde_json::from_str(&raw)?;
    if let Some(object) = payload.as_object_mut() {
        object.remove("_reuseExistingOutput");
        object.remove("_repairExistingOutputError");
    }
    conn.execute("UPDATE native_jobs SET payload_json=?2 WHERE id=?1",params![job_id,serde_json::to_string(&payload)?])?;
    Ok(())
}

fn active_key_for(request: &EnqueueRequest, payload: &Value) -> Result<Option<String>> {
    let job_type = match request.job_type.as_str() {
        "full_run" => "full_search",
        "revision_request" => "material_revision",
        value => value,
    };
    if job_type == "material_revision" {
        let target_id = request.target_id.as_deref().context("材料修订缺少联系人 ID")?;
        let artifact_type = payload.get("artifactType").and_then(Value::as_str)
            .context("材料修订缺少材料类型")?;
        return Ok(Some(format!("material_revision:{target_id}:{artifact_type}")))
    }
    if let Some(target_id) = request.target_id.as_deref() {
        let target_type = request.target_type.as_deref().unwrap_or("target");
        return Ok(Some(format!("{job_type}:{target_type}:{target_id}")))
    }
    match job_type {
        "full_search" | "research_pi" | "opportunity_health" => {
            let query = payload.get("query").and_then(Value::as_str).unwrap_or("").trim();
            let normalized_query = query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
            let digest = format!("{:x}", Sha256::digest(normalized_query.as_bytes()));
            Ok(Some(format!("{job_type}:query:{}", &digest[..24])))
        }
        "follow_up_scan" | "preference_rebuild" => Ok(Some(format!("{job_type}:global"))),
        _ => Ok(None),
    }
}

fn timeout_seconds_for(job_type: &str) -> i64 {
    match job_type {
        "full_run" | "full_search" | "research_pi" => 2 * 60 * 60,
        "revision_request" | "material_revision" => 60 * 60,
        "reply_followup" | "checklist_refresh" | "follow_up_scan"
        | "opportunity_health" | "pi_verification" => 45 * 60,
        "preference_rebuild" => 10 * 60,
        _ => 60 * 60,
    }
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
    let provider_id = request.provider_id.clone().unwrap_or_else(|| defaults.0.clone());
    let account_id = if let Some(account_id) = request.account_id.clone() {
        Some(account_id)
    } else if provider_id == defaults.0 {
        defaults.1.clone()
    } else {
        conn.query_row(
            "SELECT id FROM provider_accounts
             WHERE provider_id=?1 AND enabled=1 ORDER BY updated_at DESC,id LIMIT 1",
            [&provider_id],
            |row| row.get(0),
        ).optional()?
    };
    let model_id = if let Some(model_id) = request.model_id.clone() {
        model_id
    } else if provider_id == defaults.0 {
        defaults.2.clone()
    } else {
        conn.query_row(
            "SELECT id FROM provider_models
             WHERE provider_id=?1 AND enabled=1 ORDER BY display_name,id LIMIT 1",
            [&provider_id],
            |row| row.get(0),
        ).context("所选服务商没有已启用模型")?
    };
    let reasoning = request.reasoning.clone().unwrap_or_else(|| {
        if provider_id == defaults.0 { defaults.3.clone() } else { "high".into() }
    });
    let enabled: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM model_providers p JOIN provider_models m ON m.provider_id=p.id
            WHERE p.id=?1 AND m.id=?2 AND p.enabled=1 AND m.enabled=1
              AND (p.id='openai' OR EXISTS(
                  SELECT 1 FROM provider_accounts a
                  WHERE a.provider_id=p.id AND a.enabled=1
                    AND (?3 IS NULL OR a.id=?3)
              ))
         )",
        params![provider_id, model_id, account_id],
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
             started_at=NULL, timeout_at=NULL, heartbeat_at=NULL,
             lease_owner=NULL, lease_expires_at=NULL,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE status='running'",
        [],
    )?;
    Ok(())
}

fn reclaim_expired_jobs(path: &Path, current_worker_id: &str) -> Result<usize> {
    let mut conn = db::connect(path)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let expired = {
        let mut statement = tx.prepare(
            "SELECT id,
                    CASE WHEN timeout_at IS NOT NULL
                              AND timeout_at<=strftime('%Y-%m-%dT%H:%M:%SZ','now')
                         THEN 1 ELSE 0 END
             FROM native_jobs
             WHERE status='running'
               AND lease_expires_at IS NOT NULL
               AND lease_expires_at<=strftime('%Y-%m-%dT%H:%M:%SZ','now')
               AND (lease_owner IS NULL OR lease_owner<>?1)",
        )?;
        statement.query_map([current_worker_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
        })?.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (job_id, timed_out) in &expired {
        if *timed_out {
            tx.execute(
                "UPDATE native_jobs
                 SET status='failed',progress=100,message='失联任务已超时',
                     error='任务租约失效且超过最长执行时间',finished_at=strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                     heartbeat_at=NULL,lease_owner=NULL,lease_expires_at=NULL,
                     updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
                 WHERE id=?1 AND status='running'",
                [job_id],
            )?;
            insert_event(&tx, job_id, "timed_out", Some(100), "失联任务已超时", json!({}))?;
        } else {
            tx.execute(
                "UPDATE native_jobs
                 SET status='queued',progress=0,message='任务租约失联，已回收到队列',
                     attempt=attempt+1,started_at=NULL,timeout_at=NULL,heartbeat_at=NULL,
                     lease_owner=NULL,lease_expires_at=NULL,
                     updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
                 WHERE id=?1 AND status='running'",
                [job_id],
            )?;
            insert_event(&tx, job_id, "lease_recovered", Some(0), "失联任务已回收到队列", json!({}))?;
        }
    }
    tx.commit()?;
    Ok(expired.len())
}

fn claim_next_job(path: &Path, worker_id: &str) -> Result<Option<JobSummary>> {
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
             timeout_at=strftime('%Y-%m-%dT%H:%M:%SZ','now','+' || timeout_seconds || ' seconds'),
             heartbeat_at=strftime('%Y-%m-%dT%H:%M:%SZ','now'),
             lease_owner=?2,
             lease_expires_at=strftime('%Y-%m-%dT%H:%M:%SZ','now','+' || ?3 || ' seconds'),
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND status='queued'",
        params![id,worker_id,LEASE_DURATION_SECONDS],
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

fn load_timeout_seconds(path: &Path, job_id: &str) -> Result<u64> {
    let conn = db::connect(path)?;
    let value: i64 = conn.query_row(
        "SELECT timeout_seconds FROM native_jobs WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )?;
    u64::try_from(value).ok().filter(|value| *value > 0)
        .context("任务超时配置无效")
}

fn refresh_job_lease(path: &Path, job_id: &str, worker_id: &str) -> Result<()> {
    let conn = db::connect(path)?;
    let changed = conn.execute(
        "UPDATE native_jobs
         SET heartbeat_at=strftime('%Y-%m-%dT%H:%M:%SZ','now'),
             lease_expires_at=strftime('%Y-%m-%dT%H:%M:%SZ','now','+' || ?3 || ' seconds'),
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND status='running' AND lease_owner=?2",
        params![job_id,worker_id,LEASE_DURATION_SECONDS],
    )?;
    if changed != 1 {
        bail!("任务租约已失效，停止旧执行器")
    }
    Ok(())
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

fn update_activity(path: &Path, job_id: &str, message: &str) -> Result<()> {
    let conn = db::connect(path)?;
    let changed = conn.execute(
        "UPDATE native_jobs SET message=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND status='running' AND COALESCE(message,'')<>?2",
        params![job_id, message],
    )?;
    if changed == 1 {
        insert_event(&conn, job_id, "activity", None, message, json!({}))?;
    }
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
    let changed = conn.execute(
        "UPDATE native_jobs
         SET status=?2, progress=CASE WHEN ?2 IN ('needs_review','completed','failed','cancelled') THEN 100 ELSE progress END,
             message=?3, result_json=?4, error=?5,
             finished_at=strftime('%Y-%m-%dT%H:%M:%SZ','now'),
             heartbeat_at=NULL,lease_owner=NULL,lease_expires_at=NULL,
             updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id=?1 AND status='running'",
        params![job_id, status, message, result.map(|value| value.to_string()), error],
    )?;
    if changed == 1 {
        insert_event(&conn, job_id, status, Some(100), message, json!({}))?;
    }
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
        conn.execute_batch(include_str!("../migrations/0001_legacy_foundation.sql"))?;
        conn.execute_batch(include_str!("../migrations/0008_native_desktop.sql"))?;
        conn.execute_batch(include_str!("../migrations/0011_scheduler_leases.sql"))?;
        drop(conn);
        let codex = Arc::new(CodexManager::new(paths.clone()));
        Ok(Arc::new(Scheduler::new(paths, codex, None)))
    }

    #[test]
    fn explicit_provider_uses_that_providers_account_and_model() -> Result<()> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE task_model_defaults(
                task_type TEXT PRIMARY KEY,provider_id TEXT NOT NULL,account_id TEXT,
                model_id TEXT NOT NULL,reasoning TEXT NOT NULL
             );
             CREATE TABLE model_providers(id TEXT PRIMARY KEY,enabled INTEGER NOT NULL);
             CREATE TABLE provider_models(
                id TEXT PRIMARY KEY,provider_id TEXT NOT NULL,display_name TEXT NOT NULL,
                enabled INTEGER NOT NULL
             );
             CREATE TABLE provider_accounts(
                id TEXT PRIMARY KEY,provider_id TEXT NOT NULL,enabled INTEGER NOT NULL,
                updated_at TEXT NOT NULL
             );
             INSERT INTO task_model_defaults VALUES(
                'maintenance','openai','openai-active','openai:gpt-default','xhigh'
             );
             INSERT INTO model_providers VALUES('openai',1),('deepseek',1);
             INSERT INTO provider_models VALUES
                ('openai:gpt-default','openai','GPT Default',1),
                ('deepseek:deepseek-v4-flash','deepseek','DeepSeek V4 Flash',1);
             INSERT INTO provider_accounts VALUES
                ('openai-active','openai',1,'2026-01-01'),
                ('deepseek-active','deepseek',1,'2026-01-02');",
        )?;
        let snapshot = resolve_model_snapshot(
            &conn,
            &EnqueueRequest {
                job_type: "maintenance".into(),
                target_type: None,
                target_id: None,
                prompt: None,
                payload: None,
                provider_id: Some("deepseek".into()),
                account_id: None,
                model_id: None,
                reasoning: None,
                thread_id: None,
            },
        )?;
        assert_eq!(snapshot.0, "deepseek");
        assert_eq!(snapshot.1.as_deref(), Some("deepseek-active"));
        assert_eq!(snapshot.2, "deepseek:deepseek-v4-flash");
        assert_eq!(snapshot.3, "high");
        Ok(())
    }

    #[test]
    fn live_activity_updates_message_without_faking_progress_or_duplicate_events() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let job_id = scheduler.enqueue(EnqueueRequest {
            job_type: "test_delay".into(), target_type: None, target_id: None,
            prompt: None, payload: Some(json!({"testDelayMs":100})), provider_id: None,
            account_id: None, model_id: None, reasoning: None, thread_id: None,
        })?;
        claim_next_job(&scheduler.paths.database, &scheduler.worker_id)?.context("任务没有进入运行态")?;

        update_activity(&scheduler.paths.database, &job_id, "正在搜索网页：target field")?;
        update_activity(&scheduler.paths.database, &job_id, "正在搜索网页：target field")?;

        let conn = db::connect(&scheduler.paths.database)?;
        let (message, progress): (String, i64) = conn.query_row(
            "SELECT message,progress FROM native_jobs WHERE id=?1",
            [&job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let event_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM native_job_events WHERE job_id=?1 AND event_type='activity'",
            [&job_id],
            |row| row.get(0),
        )?;
        assert_eq!(message, "正在搜索网页：target field");
        assert_eq!(progress, 1);
        assert_eq!(event_count, 1);
        Ok(())
    }

    #[test]
    fn search_finalization_stops_new_research_and_requires_the_contract_file() -> Result<()> {
        let prompt = search_finalization_prompt(1, MAX_SEARCH_FINALIZATION_TURNS);
        assert!(prompt.contains("Do not call web search"));
        assert!(prompt.contains("output/search-results.json"));
        assert!(prompt.contains("validate the JSON file"));

        let temp = TempDir::new()?;
        let paths = test_paths(&temp);
        let job_id = "job-finalize";
        assert!(!has_reusable_output(&paths, job_id, "full_search"));
        let output = paths.workspaces.join(job_id).join("output");
        fs::create_dir_all(&output)?;
        fs::write(output.join("search-results.json"), b"{}")?;
        assert!(has_reusable_output(&paths, job_id, "full_search"));
        let repair = search_result_repair_prompt("PI：目标定制 CV 未通过两页预检（当前为 3 页）");
        assert!(repair.contains("Do not call web search"));
        assert!(repair.contains("36 to 38"));
        assert!(repair.contains("exactly two well-filled pages"));
        Ok(())
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

    #[test]
    fn duplicate_active_material_task_is_rejected_until_the_first_finishes() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let request = EnqueueRequest {
            job_type: "material_revision".into(),
            target_type: Some("contact_target".into()),
            target_id: Some("target-1".into()),
            prompt: Some("revise".into()),
            payload: Some(json!({"artifactType":"cv_data"})),
            provider_id: None,
            account_id: None,
            model_id: None,
            reasoning: None,
            thread_id: None,
        };
        let first = scheduler.enqueue(request.clone())?;
        let claimed = claim_next_job(&scheduler.paths.database, &scheduler.worker_id)?
            .context("材料任务没有进入运行态")?;
        assert_eq!(claimed.id, first);
        persist_revision_base_sha256(&scheduler.paths.database, &first, "trusted-base-hash")?;
        let duplicate = scheduler.enqueue(request.clone()).unwrap_err();
        assert!(duplicate.to_string().contains(&first));
        let conn = db::connect(&scheduler.paths.database)?;
        let (active_key,payload_raw): (String,String) = conn.query_row(
            "SELECT active_key,payload_json FROM native_jobs WHERE id=?1",
            [&first],
            |row| Ok((row.get(0)?,row.get(1)?)),
        )?;
        assert_eq!(active_key, "material_revision:target-1:cv_data");
        assert_eq!(serde_json::from_str::<Value>(&payload_raw)?["_baseSha256"], "trusted-base-hash");
        conn.execute("UPDATE native_jobs SET status='completed' WHERE id=?1", [&first])?;
        drop(conn);
        assert_ne!(scheduler.enqueue(request)?, first);
        Ok(())
    }

    #[tokio::test]
    async fn whole_job_timeout_stops_execution_and_marks_failure() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let job_id = scheduler.enqueue(EnqueueRequest {
            job_type: "test_delay".into(), target_type: None, target_id: None,
            prompt: None, payload: Some(json!({"testDelayMs":2500})), provider_id: None,
            account_id: None, model_id: None, reasoning: None, thread_id: None,
        })?;
        let conn = db::connect(&scheduler.paths.database)?;
        conn.execute("UPDATE native_jobs SET timeout_seconds=1 WHERE id=?1", [&job_id])?;
        drop(conn);
        let claimed = claim_next_job(&scheduler.paths.database, &scheduler.worker_id)?
            .context("超时测试任务没有进入运行态")?;
        scheduler.execute_job(claimed).await;
        let conn = db::connect(&scheduler.paths.database)?;
        let (status,message,lease_owner):(String,String,Option<String>) = conn.query_row(
            "SELECT status,message,lease_owner FROM native_jobs WHERE id=?1",
            [&job_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        )?;
        assert_eq!(status, "failed");
        assert!(message.contains("超时"));
        assert!(lease_owner.is_none());
        Ok(())
    }

    #[test]
    fn expired_foreign_lease_is_requeued_or_failed_when_already_timed_out() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let job_id = scheduler.enqueue(EnqueueRequest {
            job_type: "test_delay".into(), target_type: None, target_id: None,
            prompt: None, payload: Some(json!({"testDelayMs":50})), provider_id: None,
            account_id: None, model_id: None, reasoning: None, thread_id: None,
        })?;
        claim_next_job(&scheduler.paths.database, "dead-worker")?
            .context("租约测试任务没有进入运行态")?;
        let conn = db::connect(&scheduler.paths.database)?;
        conn.execute(
            "UPDATE native_jobs
             SET lease_expires_at='2000-01-01T00:00:00Z',timeout_at='2999-01-01T00:00:00Z'
             WHERE id=?1",
            [&job_id],
        )?;
        drop(conn);
        assert_eq!(reclaim_expired_jobs(&scheduler.paths.database, &scheduler.worker_id)?, 1);
        let conn = db::connect(&scheduler.paths.database)?;
        let (status,attempt):(String,i64) = conn.query_row(
            "SELECT status,attempt FROM native_jobs WHERE id=?1",
            [&job_id],
            |row| Ok((row.get(0)?,row.get(1)?)),
        )?;
        assert_eq!((status,attempt), ("queued".into(),1));
        conn.execute(
            "UPDATE native_jobs
             SET status='running',lease_owner='dead-worker',
                 lease_expires_at='2000-01-01T00:00:00Z',timeout_at='2000-01-01T00:00:00Z'
             WHERE id=?1",
            [&job_id],
        )?;
        drop(conn);
        assert_eq!(reclaim_expired_jobs(&scheduler.paths.database, &scheduler.worker_id)?, 1);
        let conn = db::connect(&scheduler.paths.database)?;
        let status: String = conn.query_row(
            "SELECT status FROM native_jobs WHERE id=?1",
            [&job_id],
            |row| row.get(0),
        )?;
        assert_eq!(status, "failed");
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
        let claimed = claim_next_job(&paths.database, &scheduler.worker_id)?.context("测试任务没有进入运行态")?;
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
        let claimed = claim_next_job(&paths.database, &scheduler.worker_id)?.context("恢复后的任务没有重新进入队列")?;
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

    #[test]
    fn retry_repairs_search_output_that_failed_two_page_preflight() -> Result<()> {
        let temp = TempDir::new()?;
        let scheduler = setup_test_scheduler(&temp)?;
        let paths = scheduler.paths.clone();
        let job_id = scheduler.enqueue(EnqueueRequest {
            job_type:"full_search".into(), target_type:None, target_id:None,
            prompt:Some("search".into()), payload:Some(json!({})), provider_id:None,
            account_id:None, model_id:None, reasoning:None, thread_id:Some("thread-search".into()),
        })?;
        let output = paths.workspaces.join(&job_id).join("output");
        fs::create_dir_all(&output)?;
        fs::write(output.join("search-results.json"), br#"{"schemaVersion":1,"opportunities":[]}"#)?;
        let error = "PI：目标定制 CV 未通过两页预检（CV 必须正好为 2 页，当前为 3 页）";
        let conn = db::connect(&paths.database)?;
        conn.execute(
            "UPDATE native_jobs SET status='failed',error=?2 WHERE id=?1",
            params![&job_id,error],
        )?;
        drop(conn);

        scheduler.retry(&job_id)?;
        let conn = db::connect(&paths.database)?;
        let (payload,message):(String,String)=conn.query_row(
            "SELECT payload_json,message FROM native_jobs WHERE id=?1",[&job_id],|row|Ok((row.get(0)?,row.get(1)?)),
        )?;
        let payload:Value = serde_json::from_str(&payload)?;
        assert_eq!(payload["_repairExistingOutputError"], error);
        assert!(payload.get("_reuseExistingOutput").is_none());
        assert!(message.contains("定向修复"));
        Ok(())
    }
}
