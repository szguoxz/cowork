//! Cowork Desktop Application
//!
//! This crate provides the Tauri-based desktop application for Cowork.

pub mod agentic_loop;
pub mod chat;
pub mod commands;
pub mod loop_channel;
pub mod session_storage;
pub mod simple_commands;
pub mod simple_loop;
pub mod state;
pub mod streaming;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use cowork_core::{AgentRegistry, ConfigManager, Context, Workspace};
use state::AppState;

/// Initialize the application state
pub fn init_state(workspace_path: std::path::PathBuf) -> AppState {
    let workspace = Workspace::new(&workspace_path);
    let context = Context::new(workspace);
    let registry = AgentRegistry::new();

    // Initialize config manager, falling back to default if it fails
    let config_manager = ConfigManager::new().unwrap_or_default();

    AppState {
        context: Arc::new(RwLock::new(context)),
        registry: Arc::new(RwLock::new(registry)),
        workspace_path,
        config_manager: Arc::new(RwLock::new(config_manager)),
        loop_input: Arc::new(RwLock::new(None)),
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

            // Ensure the workspace directory exists (required for Windows)
            if let Err(e) = std::fs::create_dir_all(&workspace_path) {
                tracing::warn!("Failed to create workspace directory: {}", e);
            }

            // Ensure config directory exists (required for Windows)
            if let Some(config_dir) = dirs::config_dir() {
                let cowork_config_dir = config_dir.join("cowork");
                if let Err(e) = std::fs::create_dir_all(&cowork_config_dir) {
                    tracing::warn!("Failed to create config directory: {}", e);
                }
                // Also create sessions subdirectory
                let sessions_dir = cowork_config_dir.join("sessions");
                if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
                    tracing::warn!("Failed to create sessions directory: {}", e);
                }
            }

            let state = init_state(workspace_path);
            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Simple loop commands (new architecture)
            simple_commands::start_loop,
            simple_commands::send_message,
            simple_commands::stop_loop,
            simple_commands::is_loop_running,
            simple_commands::approve_tool,
            simple_commands::reject_tool,
            // Settings commands
            commands::get_settings,
            commands::update_settings,
            commands::save_settings,
            commands::check_api_key,
            commands::test_api_connection,
            commands::is_setup_complete,
            commands::fetch_provider_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

