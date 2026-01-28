//! Cowork Desktop Application
//!
//! This crate provides the Tauri-based desktop application for Cowork.

pub mod commands;
pub mod session_storage;
pub mod simple_commands;
pub mod state;

use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;
use parking_lot::RwLock;

use cowork_core::session::{OutputReceiver, SessionManager, SessionOutput};
use cowork_core::{ConfigManager, Context, Workspace};
use state::AppState;

const REPO_OWNER: &str = "szguoxz";
const REPO_NAME: &str = "cowork";

/// True if built by GitHub CI, false for local builds.
const IS_CI_BUILD: bool = option_env!("GITHUB_ACTIONS").is_some();

/// Check for updates in the background (same approach as CLI).
/// Returns Some(version) if an update is available, None otherwise.
/// Only runs for CI builds.
fn check_for_update_background() -> Option<String> {
    if !IS_CI_BUILD {
        return None;
    }

    let current = env!("CARGO_PKG_VERSION");

    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .ok()?
        .fetch()
        .ok()?;

    let latest = releases.first()?;
    let latest_version = latest.version.trim_start_matches('v');

    if self_update::version::bump_is_greater(current, latest_version).unwrap_or(false) {
        Some(latest_version.to_string())
    } else {
        None
    }
}

/// Initialize the application state
///
/// Returns the state and the output receiver (to be consumed by the output handler).
pub fn init_state(
    workspace_path: std::path::PathBuf,
    config_manager: ConfigManager,
) -> (AppState, OutputReceiver) {
    let workspace = Workspace::new(&workspace_path);
    let context = Context::new(workspace);

    // Wrap config manager in Arc<RwLock> for shared access
    let config_manager = Arc::new(RwLock::new(config_manager));

    // Create session manager - reads config from disk for each new session
    let (session_manager, output_rx) = SessionManager::new(workspace_path.clone());

    let state = AppState {
        context: Arc::new(RwLock::new(context)),
        workspace_path,
        config_manager,
        session_manager: Arc::new(session_manager),
    };

    (state, output_rx)
}

/// Spawn the output handler that forwards session outputs to the frontend
fn spawn_output_handler(app_handle: tauri::AppHandle, mut output_rx: OutputReceiver) {
    use tauri::Emitter;

    tauri::async_runtime::spawn(async move {
        tracing::info!("Session output handler started");

        while let Some((session_id, output)) = output_rx.recv().await {
            // Warn if session_id looks like a subagent UUID (should be forwarded with parent_id)
            let is_uuid = session_id.len() == 36 && session_id.chars().filter(|c| *c == '-').count() == 4;
            if is_uuid {
                tracing::warn!(
                    "Received event with UUID session_id (possible subagent leak): {} {:?}",
                    session_id,
                    std::mem::discriminant(&output)
                );
            } else {
                tracing::debug!(
                    "Received output for session {}: {:?}",
                    session_id,
                    std::mem::discriminant(&output)
                );
            }

            // Emit as a tagged event with session ID
            #[derive(serde::Serialize)]
            struct SessionEvent {
                session_id: String,
                #[serde(flatten)]
                output: SessionOutput,
            }

            let event = SessionEvent {
                session_id: session_id.clone(),
                output: output.clone(),
            };

            // Emit to the general channel
            if let Err(e) = app_handle.emit("loop_output", &event) {
                tracing::error!("Failed to emit loop_output: {}", e);
            }

            // Also emit to session-specific channel
            let channel = format!("session_output:{}", session_id);
            if let Err(e) = app_handle.emit(&channel, &output) {
                tracing::error!("Failed to emit to {}: {}", channel, e);
            }
        }

        tracing::info!("Session output handler ended");
    });
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
            // Use current working directory as workspace
            let workspace_path = std::env::current_dir()
                .unwrap_or_else(|_| app.path().app_data_dir().expect("Failed to get app data dir"));

            tracing::info!("Using workspace: {:?}", workspace_path);

            // Ensure config directory exists (required for Windows)
            if let Some(config_dir) = dirs::config_dir() {
                let cowork_config_dir = config_dir.join("cowork");
                if let Err(e) = std::fs::create_dir_all(&cowork_config_dir) {
                    tracing::warn!("Failed to create config directory: {}", e);
                }
            }

            // Ensure data directory exists for sessions
            if let Some(data_dir) = dirs::data_dir() {
                let sessions_dir = data_dir.join("cowork").join("sessions");
                if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
                    tracing::warn!("Failed to create sessions directory: {}", e);
                }
            }

            // Initialize config manager, falling back to default if it fails
            let config_manager = ConfigManager::new().unwrap_or_default();

            let (state, output_rx) = init_state(workspace_path, config_manager);
            app.manage(state);

            // Spawn output handler to forward session outputs to frontend
            spawn_output_handler(app.handle().clone(), output_rx);

            // Background update check using same approach as CLI (no private key needed)
            tauri::async_runtime::spawn(async move {
                // Delay to avoid blocking startup
                tokio::time::sleep(Duration::from_secs(10)).await;

                // Use spawn_blocking for the sync self_update calls
                let result = tokio::task::spawn_blocking(|| {
                    check_for_update_background()
                }).await;

                match result {
                    Ok(Some(version)) => {
                        tracing::info!("Update available: {}", version);
                    }
                    Ok(None) => {
                        tracing::debug!("No update available or already up to date");
                    }
                    Err(e) => {
                        tracing::debug!("Update check failed: {:?}", e);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Session commands (unified architecture)
            simple_commands::start_loop,
            simple_commands::send_message,
            simple_commands::stop_loop,
            simple_commands::cancel_session,
            simple_commands::is_loop_running,
            simple_commands::approve_tool,
            simple_commands::reject_tool,
            simple_commands::answer_question,
            simple_commands::list_sessions,
            simple_commands::create_session,
            simple_commands::clear_session,
            // Saved session commands
            simple_commands::list_saved_sessions,
            simple_commands::load_saved_session,
            simple_commands::delete_saved_session,
            simple_commands::open_sessions_folder,
            // Config commands
            simple_commands::get_config_path,
            simple_commands::open_config_folder,
            // MCP server commands
            simple_commands::add_mcp_server,
            simple_commands::remove_mcp_server,
            simple_commands::list_mcp_servers,
            simple_commands::list_mcp_tools,
            // Skill commands
            simple_commands::install_skill,
            simple_commands::remove_skill,
            simple_commands::list_installed_skills,
            // Settings commands
            commands::get_settings,
            commands::update_settings,
            commands::save_settings,
            commands::check_api_key,
            commands::test_api_connection,
            commands::is_setup_complete,
            commands::fetch_provider_models,
            // Component registry commands
            commands::get_component_summary,
            commands::list_agents,
            commands::list_commands,
            commands::list_skills,
            commands::list_plugins,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

