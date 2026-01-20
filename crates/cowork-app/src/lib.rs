//! Cowork Desktop Application
//!
//! This crate provides the Tauri-based desktop application for Cowork.

pub mod commands;
pub mod session_storage;
pub mod simple_commands;
pub mod state;
pub mod streaming;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use cowork_core::session::{OutputReceiver, SessionConfig, SessionManager, SessionOutput};
use cowork_core::{AgentRegistry, ApprovalLevel, ConfigManager, Context, Workspace};
use state::AppState;

/// Initialize the application state
///
/// Returns the state and the output receiver (to be consumed by the output handler).
pub fn init_state(
    workspace_path: std::path::PathBuf,
    config_manager: ConfigManager,
) -> (AppState, OutputReceiver) {
    let workspace = Workspace::new(&workspace_path);
    let context = Context::new(workspace);
    let registry = AgentRegistry::new();

    // Get provider config for session creation - clone everything we need
    let default_provider = config_manager.config().get_default_provider().cloned();
    let approval_level: ApprovalLevel = config_manager
        .config()
        .approval
        .auto_approve_level
        .parse()
        .unwrap_or(ApprovalLevel::Low);

    // Create session manager with config factory
    let workspace_for_factory = workspace_path.clone();
    let (session_manager, output_rx) = SessionManager::new(move || {
        let mut approval_config = cowork_core::ToolApprovalConfig::default();
        approval_config.set_level(approval_level);

        let mut config = SessionConfig::new(workspace_for_factory.clone())
            .with_approval_config(approval_config);

        // Use configured provider if available
        if let Some(ref provider_config) = default_provider {
            let provider_type = match provider_config.provider_type.as_str() {
                "openai" => cowork_core::provider::ProviderType::OpenAI,
                "anthropic" => cowork_core::provider::ProviderType::Anthropic,
                "ollama" => cowork_core::provider::ProviderType::Ollama,
                "gemini" => cowork_core::provider::ProviderType::Gemini,
                _ => cowork_core::provider::ProviderType::Anthropic,
            };
            config = config.with_provider(provider_type);
            config = config.with_model(&provider_config.model);
            if let Some(api_key) = provider_config.get_api_key() {
                config = config.with_api_key(api_key);
            }
        }

        config
    });

    let state = AppState {
        context: Arc::new(RwLock::new(context)),
        registry: Arc::new(RwLock::new(registry)),
        workspace_path,
        config_manager: Arc::new(RwLock::new(config_manager)),
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
                // Also create sessions subdirectory
                let sessions_dir = cowork_config_dir.join("sessions");
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

