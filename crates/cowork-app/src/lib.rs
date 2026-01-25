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

use cowork_core::orchestration::SystemPrompt;
use cowork_core::prompt::TemplateVars;
use cowork_core::session::{OutputReceiver, SessionConfig, SessionManager, SessionOutput};
use cowork_core::{ApprovalLevel, ConfigManager, Context, Workspace};
use state::AppState;

const REPO_OWNER: &str = "szguoxz";
const REPO_NAME: &str = "cowork";

/// Check for updates in the background (same approach as CLI).
/// Returns Some(version) if an update is available, None otherwise.
fn check_for_update_background() -> Option<String> {
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

/// Build the system prompt with all template variables properly substituted
fn build_system_prompt(workspace: &std::path::Path, model_info: Option<&str>) -> String {
    let mut vars = TemplateVars::default();
    vars.working_directory = workspace.display().to_string();
    vars.is_git_repo = workspace.join(".git").exists();

    // Get git status if in a repo
    if vars.is_git_repo {
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--short", "--branch"])
            .current_dir(workspace)
            .output()
        {
            vars.git_status = String::from_utf8_lossy(&output.stdout).to_string();
        }

        // Get current branch
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(workspace)
            .output()
        {
            vars.current_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }

        // Get recent commits for style reference
        if let Ok(output) = std::process::Command::new("git")
            .args(["log", "--oneline", "-5"])
            .current_dir(workspace)
            .output()
        {
            vars.recent_commits = String::from_utf8_lossy(&output.stdout).to_string();
        }
    }

    if let Some(info) = model_info {
        vars.model_info = info.to_string();
    }

    SystemPrompt::new()
        .with_template_vars(vars)
        .build()
}

/// Build a SessionConfig from current ConfigManager settings
fn build_session_config(
    workspace_path: &std::path::Path,
    config_manager: &Arc<RwLock<ConfigManager>>,
) -> SessionConfig {
    let cm = config_manager.read();
    let config = cm.config();

    // Get provider config
    let default_provider = config.get_default_provider().cloned();
    let approval_level: ApprovalLevel = config
        .approval
        .auto_approve_level
        .parse()
        .unwrap_or(ApprovalLevel::Low);

    // Build system prompt with template variables
    let model_info = default_provider.as_ref().map(|p| p.model.as_str());
    let system_prompt = build_system_prompt(workspace_path, model_info);

    // Create session config
    let mut tool_approval_config = cowork_core::ToolApprovalConfig::default();
    tool_approval_config.set_level(approval_level);

    let mut session_config = SessionConfig::new(workspace_path.to_path_buf())
        .with_approval_config(tool_approval_config)
        .with_system_prompt(system_prompt);

    // Use configured provider if available
    if let Some(ref provider_config) = default_provider {
        let provider_type: cowork_core::provider::ProviderType = provider_config
            .provider_type
            .parse()
            .unwrap_or(cowork_core::provider::ProviderType::Anthropic);

        session_config = session_config.with_provider(provider_type);
        session_config = session_config.with_model(&provider_config.model);
        if let Some(api_key) = provider_config.get_api_key() {
            session_config = session_config.with_api_key(api_key);
        }
    }

    session_config
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

    // Create config provider that reads fresh config for each new session
    let cm_clone = config_manager.clone();
    let ws_clone = workspace_path.clone();
    let config_provider = Box::new(move || build_session_config(&ws_clone, &cm_clone));

    // Create session manager with config provider
    let (session_manager, output_rx) = SessionManager::with_config_provider(config_provider);

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
            tracing::debug!(
                "Received output for session {}: {:?}",
                session_id,
                std::mem::discriminant(&output)
            );

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
            simple_commands::is_loop_running,
            simple_commands::approve_tool,
            simple_commands::reject_tool,
            simple_commands::answer_question,
            simple_commands::list_sessions,
            simple_commands::create_session,
            // Saved session commands
            simple_commands::list_saved_sessions,
            simple_commands::load_saved_session,
            simple_commands::delete_saved_session,
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

