//! Cowork Desktop Application
//!
//! This crate provides the Tauri-based desktop application for Cowork.

pub mod commands;
pub mod state;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use cowork_core::{AgentRegistry, Context, Workspace};
use state::AppState;

/// Initialize the application state
pub fn init_state(workspace_path: std::path::PathBuf) -> AppState {
    let workspace = Workspace::new(&workspace_path);
    let context = Context::new(workspace);
    let registry = AgentRegistry::new();

    AppState {
        context: Arc::new(RwLock::new(context)),
        registry: Arc::new(RwLock::new(registry)),
        workspace_path,
    }
}

/// Run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Initialize with default workspace
            let workspace_path = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");

            let state = init_state(workspace_path);
            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_agents,
            commands::execute_task,
            commands::get_task_status,
            commands::cancel_task,
            commands::list_files,
            commands::read_file,
            commands::write_file,
            commands::execute_command,
            commands::get_settings,
            commands::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
