//! Cowork Desktop Application
//!
//! This crate provides the Tauri-based desktop application for Cowork.

pub mod agentic_loop;
pub mod chat;
pub mod commands;
pub mod state;
pub mod streaming;

use std::collections::HashMap;
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
        sessions: Arc::new(RwLock::new(HashMap::new())),
        config_manager: Arc::new(RwLock::new(config_manager)),
        loop_handles: Arc::new(RwLock::new(HashMap::new())),
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
            commands::save_settings,
            commands::check_api_key,
            // Chat commands
            commands::create_session,
            commands::list_sessions,
            commands::get_session_messages,
            commands::send_message,
            commands::execute_tool,
            commands::approve_tool_call,
            commands::reject_tool_call,
            commands::delete_session,
            // Agentic loop commands
            commands::start_loop,
            commands::stop_loop,
            commands::approve_loop_tools,
            commands::reject_loop_tools,
            commands::get_loop_state,
            commands::is_loop_active,
            // Streaming commands
            commands::send_message_stream,
            // Skill commands
            commands::execute_skill,
            commands::list_skills,
            commands::execute_command_string,
            // Context management commands
            commands::get_context_usage,
            commands::compact_session,
            commands::clear_session,
            commands::get_memory_hierarchy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
