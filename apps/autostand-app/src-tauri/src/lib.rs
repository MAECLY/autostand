//! autostand Tauri v2 desktop app backend.

#![forbid(unsafe_code)]

// The IPC surface is the crate's API: `main.rs` links this rlib, and the DTOs in
// `commands::types` appear in the public signatures of `state::PipelineStatus`.
// Keeping these modules private would make the whole state machine unreachable
// (dead code) and leak private types through public items.
pub mod commands;
pub mod error;
pub mod format_presets;
pub mod gather;
pub mod git_ops;
pub mod notifications;
pub mod pipeline_runner;
pub mod render;
pub mod run_log;
pub mod scheduler_runtime;
pub mod state;

pub use error::AppError;

use tracing_subscriber::EnvFilter;

/// Build and run the Tauri application, registering every IPC command from
/// `docs/tauri/02-ipc-contracts.md`.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state::AppState::new())
        // The in-process scheduler must start with the app, not on first IPC
        // call: nothing in the UI is required for a cron boundary to come due.
        .setup(|app| {
            scheduler_runtime::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::get_host_slug,
            commands::set_host_slug,
            commands::data_sources::list_data_sources,
            commands::data_sources::toggle_data_source,
            commands::llm::list_llm_providers,
            commands::llm::test_llm_provider,
            commands::llm::get_provider_health,
            commands::llm::refresh_provider_health,
            commands::llm::store_api_key,
            commands::llm::get_api_key_status,
            commands::llm::detect_cli,
            commands::llm::list_provider_models,
            commands::local_models::list_local_models,
            commands::local_models::download_local_model,
            commands::local_models::cancel_local_model_download,
            commands::local_models::delete_local_model,
            commands::local_models::select_local_model,
            commands::local_models::accept_local_model_terms,
            commands::local_models::unload_local_models,
            commands::os::open_in_file_manager,
            commands::pipeline::compile_standup,
            commands::pipeline::compile_all,
            commands::pipeline::trigger_run_now,
            commands::pipeline::get_pipeline_status,
            commands::pipeline::get_filing_target,
            commands::pipeline::preview_gather,
            commands::pipeline::get_scheduler_status,
            commands::pipeline::set_scheduler_schedule,
            commands::pipeline::set_scheduler_enabled,
            commands::regeneration::preview_regeneration,
            commands::regeneration::apply_regeneration,
            commands::standup::read_standup_file,
            commands::standup::add_manual_item,
            commands::standup::list_standup_dates,
            commands::standup::list_audit_sidecars,
            commands::standup::read_audit_sidecar,
            commands::sync::detect_cloud_folders,
            commands::sync::configure_cloud_sync,
            commands::sync::get_repo_sync_status,
            commands::sync::setup_repo_sync,
            commands::dependencies::get_dependency_status,
            commands::dependencies::run_dependency_remediation,
            commands::repos::discover_repos,
            commands::readiness::get_standup_readiness,
            commands::settings::get_settings_paths,
            commands::settings::validate_paths,
            notifications::get_notification_status,
            notifications::request_notification_permission,
            notifications::send_test_notification,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
