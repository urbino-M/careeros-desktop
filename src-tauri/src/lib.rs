#![recursion_limit = "256"]

mod codex;
mod cover_letter;
mod cv_schema;
mod db;
mod gmail;
mod internship;
mod materials;
mod migration;
mod models;
mod onboarding;
mod paths;
mod providers;
mod scheduler;
mod secrets;
mod search_channels;
mod typst;
mod workflows;

use codex::CodexManager;
use base64::Engine;
use models::{
    AuthGuide, DashboardData, GmailDraftInfo, GmailOAuthStart, GmailStatus, InboundReplyRequest,
    JobGroups, MigrationReport, ProviderInfo, ReplyItem, SearchCapabilities, SearchChannel,
    SearchSetupResult, TargetCard, TargetDetail, TaskModelDefault,
};
use paths::AppPaths;
use scheduler::{EnqueueRequest, Scheduler};
use providers::ProviderConnectionRequest;
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    paths: AppPaths,
    migration: MigrationReport,
    scheduler: Arc<Scheduler>,
    codex: Arc<CodexManager>,
    gmail: Arc<gmail::GmailManager>,
}

#[tauri::command]
fn get_migration_report(state: tauri::State<'_, AppState>) -> MigrationReport {
    state.migration.clone()
}

#[tauri::command]
fn get_app_paths(state: tauri::State<'_, AppState>) -> AppPaths {
    state.paths.clone()
}

#[tauri::command(rename_all = "camelCase")]
fn get_dashboard(
    state: tauri::State<'_, AppState>,
    career_track: Option<String>,
) -> Result<DashboardData, String> {
    db::dashboard(&state.paths.database, career_track.as_deref().unwrap_or("postdoc"))
        .map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn get_contact_targets(
    state: tauri::State<'_, AppState>,
    status: Option<String>,
    submission_status: Option<String>,
    verification_status: Option<String>,
    career_track: Option<String>,
    search: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<Vec<TargetCard>, String> {
    db::list_targets(
        &state.paths.database,
        career_track.as_deref().unwrap_or("postdoc"),
        status.as_deref(),
        submission_status.as_deref(),
        verification_status.as_deref(),
        search.as_deref(),
        offset.unwrap_or(0),
        limit.unwrap_or(20),
    )
    .map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn get_contact_target(
    state: tauri::State<'_, AppState>,
    target_id: String,
) -> Result<TargetDetail, String> {
    db::target_detail(&state.paths.database, &state.paths.data_root, &target_id)
        .map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn set_contact_status(
    state: tauri::State<'_, AppState>,
    target_id: String,
    status: String,
) -> Result<(), String> {
    db::update_target_status(&state.paths.database, &target_id, &status).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn set_submission_status(
    state: tauri::State<'_, AppState>,
    target_id: String,
    status: String,
) -> Result<(), String> {
    db::update_submission_status(&state.paths.database, &target_id, &status)
        .map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn read_material_text(
    state: tauri::State<'_, AppState>,
    artifact_path: String,
) -> Result<String, String> {
    db::read_artifact(&state.paths, &artifact_path).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn read_pdf_preview(
    state: tauri::State<'_, AppState>,
    artifact_path: String,
) -> Result<String, String> {
    let bytes = db::read_pdf_preview(&state.paths, &artifact_path).map_err(display_error)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
async fn save_manual_material(
    state: tauri::State<'_, AppState>,
    request: materials::ManualRevisionRequest,
) -> Result<materials::RevisionResult, String> {
    let result = materials::save_manual(&state.paths, &request).map_err(display_error)?;
    if request.artifact_type == "cv_data" {
        typst::generate_cv(&state.paths, &request.target_id)
            .await
            .map_err(display_error)?;
    } else if request.artifact_type == "cover_letter_text" {
        cover_letter::regenerate_from_text(&state.paths, &request.target_id)
            .await
            .map_err(display_error)?;
    }
    Ok(result)
}

#[tauri::command]
fn save_inbound_reply(
    state: tauri::State<'_, AppState>,
    request: InboundReplyRequest,
) -> Result<ReplyItem, String> {
    db::save_inbound_reply(&state.paths.database, &request).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
async fn generate_typst_cv(
    state: tauri::State<'_, AppState>,
    target_id: String,
) -> Result<typst::CvGenerationResult, String> {
    typst::generate_cv(&state.paths, &target_id)
        .await
        .map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
async fn generate_cover_letter(
    state: tauri::State<'_, AppState>,
    target_id: String,
) -> Result<cover_letter::CoverLetterGenerationResult, String> {
    cover_letter::generate(&state.paths, &target_id)
        .await
        .map_err(display_error)
}

#[tauri::command]
fn get_model_providers(state: tauri::State<'_, AppState>) -> Result<Vec<ProviderInfo>, String> {
    db::providers(&state.paths.database).map_err(display_error)
}

#[tauri::command]
async fn connect_responses_provider(
    state: tauri::State<'_, AppState>,
    request: ProviderConnectionRequest,
) -> Result<ProviderInfo, String> {
    let discovery = providers::discover_responses_provider(&request)
        .await
        .map_err(display_error)?;
    let previous = secrets::get_secret(&discovery.secret_reference).map_err(display_error)?;
    secrets::set_secret(&discovery.secret_reference, request.api_key.trim())
        .map_err(display_error)?;
    if let Err(error) = db::upsert_response_provider(&state.paths.database, &discovery) {
        let rollback = match previous {
            Some(secret) => secrets::set_secret(&discovery.secret_reference, &secret),
            None => secrets::delete_secret(&discovery.secret_reference),
        };
        if let Err(rollback_error) = rollback {
            return Err(format!(
                "保存模型服务失败：{error:#}；凭据文件回滚也失败：{rollback_error:#}"
            ));
        }
        return Err(display_error(error));
    }
    state.codex.invalidate_provider(&discovery.id).await;
    db::providers(&state.paths.database)
        .map_err(display_error)?
        .into_iter()
        .find(|provider| provider.id == discovery.id)
        .ok_or_else(|| "模型服务已保存，但无法重新读取连接状态".into())
}

#[tauri::command(rename_all = "camelCase")]
async fn disconnect_responses_provider(
    state: tauri::State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    let secret_reference = db::disable_response_provider(&state.paths.database, &provider_id)
        .map_err(display_error)?;
    state.codex.invalidate_provider(&provider_id).await;
    secrets::delete_secret(&secret_reference).map_err(display_error)
}

#[tauri::command]
fn get_task_model_defaults(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TaskModelDefault>, String> {
    db::task_defaults(&state.paths.database).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn save_task_model_default(
    state: tauri::State<'_, AppState>,
    value: TaskModelDefault,
) -> Result<(), String> {
    db::save_task_default(&state.paths.database, &value).map_err(display_error)
}

#[tauri::command]
fn get_cv_customization(
    state: tauri::State<'_, AppState>,
) -> Result<materials::CvCustomizationSettings, String> {
    materials::load_cv_customization(&state.paths).map_err(display_error)
}

#[tauri::command]
fn save_cv_customization(
    state: tauri::State<'_, AppState>,
    value: materials::CvCustomizationSettings,
) -> Result<materials::CvCustomizationSettings, String> {
    materials::save_cv_customization(&state.paths, value).map_err(display_error)
}

#[tauri::command]
fn get_onboarding_profile(
    state: tauri::State<'_, AppState>,
) -> Result<onboarding::OnboardingProfile, String> {
    onboarding::load(&state.paths).map_err(display_error)
}

#[tauri::command]
fn save_onboarding_profile(
    state: tauri::State<'_, AppState>,
    value: onboarding::OnboardingProfile,
) -> Result<onboarding::OnboardingProfile, String> {
    onboarding::save(&state.paths, value).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn import_onboarding_cv(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    onboarding::import_cv(&state.paths, std::path::Path::new(&path)).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn import_internship_cv(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    internship::import_cv(&state.paths, std::path::Path::new(&path)).map_err(display_error)
}

#[tauri::command]
fn get_internship_profile(
    state: tauri::State<'_, AppState>,
) -> Result<internship::InternshipProfile, String> {
    internship::load(&state.paths).map_err(display_error)
}

#[tauri::command]
fn save_internship_profile(
    state: tauri::State<'_, AppState>,
    value: internship::InternshipProfile,
) -> Result<internship::InternshipProfile, String> {
    internship::save(&state.paths, value).map_err(display_error)
}

#[tauri::command]
fn get_search_capabilities(
    state: tauri::State<'_, AppState>,
) -> Result<SearchCapabilities, String> {
    search_channels::capabilities(&state.paths).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
async fn setup_search_capabilities(
    state: tauri::State<'_, AppState>,
    channels: Option<Vec<SearchChannel>>,
) -> Result<SearchSetupResult, String> {
    search_channels::setup(&state.paths, channels)
        .await
        .map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn begin_search_channel_auth(channel: SearchChannel) -> Result<AuthGuide, String> {
    search_channels::auth_guide(channel).map_err(display_error)
}

#[tauri::command]
fn get_jobs(
    state: tauri::State<'_, AppState>,
    page_size: Option<usize>,
) -> Result<JobGroups, String> {
    db::job_groups(&state.paths.database, page_size.unwrap_or(5)).map_err(display_error)
}

#[tauri::command]
fn enqueue_job(
    state: tauri::State<'_, AppState>,
    request: EnqueueRequest,
) -> Result<String, String> {
    state.scheduler.enqueue(request).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
async fn cancel_job(state: tauri::State<'_, AppState>, job_id: String) -> Result<(), String> {
    state.scheduler.cancel(&job_id).await.map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn retry_job(state: tauri::State<'_, AppState>, job_id: String) -> Result<(), String> {
    state.scheduler.retry(&job_id).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn approve_job(state: tauri::State<'_, AppState>, job_id: String) -> Result<(), String> {
    state.scheduler.approve(&job_id).map_err(display_error)
}

#[tauri::command]
async fn get_codex_account(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.codex.account_status().await.map_err(display_error)
}

#[tauri::command]
async fn get_codex_models(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.codex.model_list().await.map_err(display_error)
}

#[tauri::command]
async fn connect_chatgpt(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    state.codex.start_chatgpt_login().await.map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
async fn wait_for_chatgpt_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    login_id: String,
) -> Result<Value, String> {
    let account = state
        .codex
        .wait_for_chatgpt_login(&login_id)
        .await
        .map_err(display_error)?;
    db::set_openai_auth_kind(&state.paths.database, "chatgpt_oauth").map_err(display_error)?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(account)
}

#[tauri::command]
fn get_gmail_status(state: tauri::State<'_, AppState>) -> Result<GmailStatus, String> {
    state.gmail.status().map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn import_gmail_client(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state
        .gmail
        .import_client_file(std::path::Path::new(&path))
        .map_err(display_error)
}

#[tauri::command]
async fn start_gmail_oauth(
    state: tauri::State<'_, AppState>,
) -> Result<GmailOAuthStart, String> {
    state.gmail.start_oauth().await.map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn approve_cv_for_gmail(
    state: tauri::State<'_, AppState>,
    target_id: String,
) -> Result<String, String> {
    state.gmail.approve_cv(&target_id).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn get_cv_approval(
    state: tauri::State<'_, AppState>,
    target_id: String,
) -> Result<bool, String> {
    state.gmail.cv_approval_status(&target_id).map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
async fn create_gmail_draft(
    state: tauri::State<'_, AppState>,
    target_id: String,
    recipient: String,
    subject: String,
    body: String,
) -> Result<GmailDraftInfo, String> {
    state
        .gmail
        .create_draft(&target_id, &recipient, &subject, &body)
        .await
        .map_err(display_error)
}

#[tauri::command(rename_all = "camelCase")]
fn list_gmail_drafts(
    state: tauri::State<'_, AppState>,
    target_id: String,
) -> Result<Vec<GmailDraftInfo>, String> {
    state.gmail.list_drafts(&target_id).map_err(display_error)
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let resource_dir = app.path().resource_dir()?;
            let paths = AppPaths::resolve(Some(&resource_dir))?;
            let migration = migration::initialize(&paths)?;
            let codex = Arc::new(CodexManager::new(paths.clone()));
            let gmail = Arc::new(gmail::GmailManager::new(paths.clone()));
            let scheduler = Arc::new(Scheduler::new(
                paths.clone(),
                codex.clone(),
                Some(app.handle().clone()),
            ));
            scheduler.start()?;
            app.manage(AppState {
                paths,
                migration,
                scheduler,
                codex,
                gmail,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_migration_report,
            get_app_paths,
            get_dashboard,
            get_contact_targets,
            get_contact_target,
            set_contact_status,
            set_submission_status,
            read_material_text,
            read_pdf_preview,
            save_manual_material,
            save_inbound_reply,
            generate_typst_cv,
            generate_cover_letter,
            get_model_providers,
            connect_responses_provider,
            disconnect_responses_provider,
            get_task_model_defaults,
            save_task_model_default,
            get_cv_customization,
            save_cv_customization,
            get_onboarding_profile,
            save_onboarding_profile,
            import_onboarding_cv,
            import_internship_cv,
            get_internship_profile,
            save_internship_profile,
            get_search_capabilities,
            setup_search_capabilities,
            begin_search_channel_auth,
            get_jobs,
            enqueue_job,
            cancel_job,
            retry_job,
            approve_job,
            get_codex_account,
            get_codex_models,
            connect_chatgpt,
            wait_for_chatgpt_login,
            get_gmail_status,
            import_gmail_client,
            start_gmail_oauth,
            approve_cv_for_gmail,
            get_cv_approval,
            create_gmail_draft,
            list_gmail_drafts,
        ])
        .build(tauri::generate_context!())
        .expect("CareerOS 启动失败");
    app.run(|app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
        ) {
            app_handle.state::<AppState>().scheduler.shutdown();
        }
    });
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
