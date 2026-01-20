//! Tauri commands for the session-based agent loop
//!
//! Commands for interacting with the SessionManager:
//! - start_loop: Initialize the session manager output handler
//! - send_message: Send a message to a session
//! - stop_loop: Stop a session
//! - approve_tool / reject_tool: Handle tool approval
//! - list_sessions: List active sessions
//! - answer_question: Send an answer to a question

use std::collections::HashMap;
use tauri::State;

use cowork_core::session::{SessionInput, SessionOutput};

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
                .await
                .map_err(|e| e.to_string())
        }
        None => {
            tracing::info!("Stopping all sessions");
            state
                .session_manager
                .stop_all()
                .await
                .map_err(|e| e.to_string())
        }
    }
}

/// Check if any sessions are running
#[tauri::command]
pub async fn is_loop_running(state: State<'_, AppState>) -> Result<bool, String> {
    let count = state.session_manager.session_count().await;
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

/// List active sessions
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.session_manager.list_sessions().await)
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
    if state.session_manager.has_session(&session_id).await {
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
