//! autostand Tauri v2 desktop app backend.

#![forbid(unsafe_code)]

mod commands;
mod error;
mod state;

pub use error::AppError;

use tracing_subscriber::EnvFilter;

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
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::get_host_slug,
            commands::set_host_slug,
            commands::data_sources::list_data_sources,
            commands::data_sources::toggle_data_source,
            commands::llm::list_llm_providers,
            commands::llm::test_llm_provider,
            commands::llm::store_api_key,
            commands::llm::get_api_key_status,
            commands::llm::detect_cli,
            commands::pipeline::compile_standup,
            commands::pipeline::compile_all,
            commands::pipeline::trigger_run_now,
            commands::pipeline::get_pipeline_status,
            commands::pipeline::preview_gather,
            commands::pipeline::get_scheduler_status,
            commands::pipeline::set_scheduler_schedule,
            commands::standup::read_standup_file,
            commands::standup::add_manual_item,
            commands::standup::list_audit_sidecars,
            commands::standup::read_audit_sidecar,
            commands::repos::discover_repos,
            commands::settings::get_settings_paths,
            commands::settings::validate_paths,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}