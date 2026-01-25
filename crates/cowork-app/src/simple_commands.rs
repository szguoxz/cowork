//! Tauri commands for the session-based agent loop
//!
//! Commands for interacting with the SessionManager:
//! - start_loop: Initialize the session manager output handler
//! - send_message: Send a message to a session
//! - stop_loop: Stop a session
//! - approve_tool / reject_tool: Handle tool approval
//! - list_sessions: List active sessions
//! - answer_question: Send an answer to a question
//! - add_mcp_server / remove_mcp_server / list_mcp_servers / list_mcp_tools: MCP management
//! - install_skill / remove_skill / list_installed_skills: Skill management
//! - clear_session: Clear conversation history
//! - open_sessions_folder: Open sessions folder in file manager

use std::collections::HashMap;
use tauri::State;

use cowork_core::config::McpServerConfig;
use cowork_core::session::{SessionInput, SessionOutput};
use cowork_core::skills::installer::{InstallLocation, SkillInstaller};

use crate::state::AppState;

/// Signal that the frontend is ready to receive events
///
/// The output handler is automatically started during app setup.
/// This command just emits initial ready/idle events for the default session.
#[tauri::command]
pub async fn start_loop(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    tracing::info!("start_loop called - emitting initial ready/idle");

    // Emit initial ready/idle for the default session
    let ready = SessionOutput::ready();
    let idle = SessionOutput::idle();

    #[derive(serde::Serialize)]
    struct SessionEvent {
        session_id: String,
        #[serde(flatten)]
        output: SessionOutput,
    }

    for output in [ready, idle] {
        let event = SessionEvent {
            session_id: "default".to_string(),
            output,
        };
        if let Err(e) = app.emit("loop_output", &event) {
            tracing::error!("Failed to emit loop_output: {}", e);
        }
    }

    Ok(())
}

/// Send a message to a session
///
/// If session_id is not provided, uses "default".
#[tauri::command]
pub async fn send_message(
    content: String,
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    tracing::info!("send_message to session '{}': {} chars", session_id, content.len());

    state
        .session_manager
        .push_message(&session_id, SessionInput::user_message(content))
        .await
        .map_err(|e| e.to_string())
}

/// Stop a session (or all sessions if no ID provided)
#[tauri::command]
pub async fn stop_loop(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    match session_id {
        Some(id) => {
            tracing::info!("Stopping session: {}", id);
            state
                .session_manager
                .stop_session(&id)
                .map_err(|e| e.to_string())
        }
        None => {
            tracing::info!("Stopping all sessions");
            state
                .session_manager
                .stop_all()
                .map_err(|e| e.to_string())
        }
    }
}

/// Check if any sessions are running
#[tauri::command]
pub async fn is_loop_running(state: State<'_, AppState>) -> Result<bool, String> {
    let count = state.session_manager.session_count();
    Ok(count > 0)
}

/// Approve a pending tool
#[tauri::command]
pub async fn approve_tool(
    tool_id: String,
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    tracing::info!("Approving tool {} in session {}", tool_id, session_id);

    state
        .session_manager
        .push_message(&session_id, SessionInput::approve_tool(tool_id))
        .await
        .map_err(|e| e.to_string())
}

/// Reject a pending tool
#[tauri::command]
pub async fn reject_tool(
    tool_id: String,
    reason: Option<String>,
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    tracing::info!("Rejecting tool {} in session {}: {:?}", tool_id, session_id, reason);

    state
        .session_manager
        .push_message(&session_id, SessionInput::reject_tool(tool_id, reason))
        .await
        .map_err(|e| e.to_string())
}

/// Answer a question from ask_user_question tool
#[tauri::command]
pub async fn answer_question(
    request_id: String,
    answers: HashMap<String, String>,
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    tracing::info!("Answering question {} in session {}", request_id, session_id);

    state
        .session_manager
        .push_message(
            &session_id,
            SessionInput::answer_question(request_id, answers),
        )
        .await
        .map_err(|e| e.to_string())
}

/// Cancel the current turn in a session
#[tauri::command]
pub async fn cancel_session(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    tracing::info!("Cancelling session {}", session_id);

    state
        .session_manager
        .push_message(&session_id, SessionInput::cancel())
        .await
        .map_err(|e| e.to_string())
}

/// List active sessions
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.session_manager.list_sessions())
}

/// Create a new session
#[tauri::command]
pub async fn create_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Creating session: {}", session_id);

    // Sessions are created on-demand when first message is sent
    // This is a no-op but allows frontend to pre-register intent
    if state.session_manager.has_session(&session_id) {
        return Err(format!("Session '{}' already exists", session_id));
    }

    Ok(())
}

/// List saved sessions from disk
#[tauri::command]
pub async fn list_saved_sessions() -> Result<Vec<crate::session_storage::SessionMetadata>, String> {
    let storage = crate::session_storage::SessionStorage::new();
    storage.list().map_err(|e| e.to_string())
}

/// Load a saved session by ID
#[tauri::command]
pub async fn load_saved_session(session_id: String) -> Result<crate::session_storage::SessionData, String> {
    let storage = crate::session_storage::SessionStorage::new();
    storage.load(&session_id).map_err(|e| e.to_string())
}

/// Delete a saved session by ID
#[tauri::command]
pub async fn delete_saved_session(session_id: String) -> Result<(), String> {
    let storage = crate::session_storage::SessionStorage::new();
    storage.delete(&session_id).map_err(|e| e.to_string())
}

// ────────────────────────────────────────────────────────────────────────────────
// MCP Server Commands
// ────────────────────────────────────────────────────────────────────────────────

/// MCP server info for frontend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub command: String,
    pub enabled: bool,
    pub status: String,
    pub tool_count: usize,
    pub error: Option<String>,
}

/// MCP tool info for frontend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub server: String,
}

/// Add an MCP server to configuration
///
/// `command` can be either:
/// - A shell command for stdio transport (e.g., "npx @modelcontextprotocol/server-filesystem")
/// - A URL for HTTP transport (e.g., "https://mcp.example.com")
#[tauri::command]
pub async fn add_mcp_server(
    name: String,
    command: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Adding MCP server '{}': {}", name, command);

    let config = if command.starts_with("http://") || command.starts_with("https://") {
        McpServerConfig::new_http(&command)
    } else {
        // Parse command string into command + args
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err("Command cannot be empty".to_string());
        }
        let cmd = parts[0].to_string();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        McpServerConfig::new(cmd).with_args(args)
    };

    // Update config and save
    {
        let mut cm = state.config_manager.write();
        cm.config_mut().mcp_servers.insert(name, config);
        cm.save().map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Remove an MCP server from configuration
#[tauri::command]
pub async fn remove_mcp_server(name: String, state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("Removing MCP server '{}'", name);

    {
        let mut cm = state.config_manager.write();
        cm.config_mut().mcp_servers.remove(&name);
        cm.save().map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// List configured MCP servers
#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerInfo>, String> {
    let config = state.config();

    let servers: Vec<McpServerInfo> = config
        .mcp_servers
        .iter()
        .map(|(name, cfg)| {
            let command = if cfg.is_http() {
                cfg.url.clone().unwrap_or_default()
            } else if cfg.args.is_empty() {
                cfg.command.clone()
            } else {
                format!("{} {}", cfg.command, cfg.args.join(" "))
            };

            McpServerInfo {
                name: name.clone(),
                command,
                enabled: cfg.enabled,
                status: "stopped".to_string(), // Servers start on-demand
                tool_count: 0,
                error: None,
            }
        })
        .collect();

    Ok(servers)
}

/// List tools from all configured MCP servers
#[tauri::command]
pub async fn list_mcp_tools(_state: State<'_, AppState>) -> Result<Vec<McpToolInfo>, String> {
    // Tools are discovered when servers are started on-demand during a session
    // For now, return empty list - tools will be available when actually used
    Ok(Vec::new())
}

// ────────────────────────────────────────────────────────────────────────────────
// Skill Commands
// ────────────────────────────────────────────────────────────────────────────────

/// Installed skill info for frontend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledSkillInfo {
    pub name: String,
    pub description: String,
    pub location: String,
    pub path: String,
}

/// Install a skill from URL
#[tauri::command]
pub async fn install_skill(
    url: String,
    global: bool,
    force: bool,
    state: State<'_, AppState>,
) -> Result<InstalledSkillInfo, String> {
    tracing::info!("Installing skill from {} (global: {})", url, global);

    let workspace = state.workspace_path.clone();
    let location = if global {
        InstallLocation::Global
    } else {
        InstallLocation::Project
    };

    // Run in blocking task since SkillInstaller uses blocking HTTP
    let result = tokio::task::spawn_blocking(move || {
        let installer = SkillInstaller::new(workspace);
        installer.install_from_url(&url, location, force)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
    .map_err(|e| e.to_string())?;

    Ok(InstalledSkillInfo {
        name: result.name,
        description: result.description,
        location: result.location.to_string(),
        path: result.path.display().to_string(),
    })
}

/// Remove an installed skill
#[tauri::command]
pub async fn remove_skill(
    name: String,
    global: Option<bool>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    tracing::info!("Removing skill '{}'", name);

    let workspace = state.workspace_path.clone();
    let location = global.map(|g| {
        if g {
            InstallLocation::Global
        } else {
            InstallLocation::Project
        }
    });

    let result = tokio::task::spawn_blocking(move || {
        let installer = SkillInstaller::new(workspace);
        installer.uninstall(&name, location)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
    .map_err(|e| e.to_string())?;

    Ok(result.display().to_string())
}

/// List installed skills
#[tauri::command]
pub async fn list_installed_skills(
    state: State<'_, AppState>,
) -> Result<Vec<InstalledSkillInfo>, String> {
    let workspace = state.workspace_path.clone();

    let skills = tokio::task::spawn_blocking(move || {
        let installer = SkillInstaller::new(workspace);
        installer.list_installed()
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    Ok(skills
        .into_iter()
        .map(|s| InstalledSkillInfo {
            name: s.name,
            description: s.description,
            location: s.location.to_string(),
            path: s.path.display().to_string(),
        })
        .collect())
}

// ────────────────────────────────────────────────────────────────────────────────
// Session Management Commands
// ────────────────────────────────────────────────────────────────────────────────

/// Clear conversation history for a session
#[tauri::command]
pub async fn clear_session(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    tracing::info!("Clearing session '{}'", session_id);

    // Stop the session (this clears the conversation)
    state
        .session_manager
        .stop_session(&session_id)
        .map_err(|e| e.to_string())?;

    // Sessions are recreated on next message, so this effectively clears history
    Ok(())
}

/// Open the sessions folder in the system file manager
#[tauri::command]
pub async fn open_sessions_folder() -> Result<String, String> {
    let sessions_dir = dirs::data_dir()
        .map(|d| d.join("cowork").join("sessions"))
        .ok_or_else(|| "Could not determine data directory".to_string())?;

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&sessions_dir)
        .map_err(|e| format!("Failed to create sessions directory: {}", e))?;

    let path_str = sessions_dir.display().to_string();

    // Open in file manager
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&sessions_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&sessions_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&sessions_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(path_str)
}
