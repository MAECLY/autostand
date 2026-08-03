//! autostand Tauri v2 desktop app backend.

#![forbid(unsafe_code)]

mod commands;

use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_host_slug,
            commands::set_host_slug,
            commands::get_config,
            commands::set_config,
            commands::compile_standup,
            commands::add_manual_item,
            commands::trigger_run_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
