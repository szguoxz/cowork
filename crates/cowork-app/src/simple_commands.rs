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

/// Start the session manager output handler
///
/// This initializes the event emitter that forwards session outputs to the frontend.
/// Called once at app startup. The actual loop is created per-session on first message.
#[tauri::command]
pub async fn start_loop(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("start_loop called - initializing output handler");

    // Take the output receiver (can only be done once)
    let output_rx = {
        let mut rx_guard = state.output_rx.lock().await;
        rx_guard.take()
    };

    let output_rx = match output_rx {
        Some(rx) => rx,
        None => {
            tracing::info!("Output handler already initialized");
            // Already initialized - emit ready/idle for the frontend
            emit_output(&app, "default", SessionOutput::ready());
            emit_output(&app, "default", SessionOutput::idle());
            return Ok(());
        }
    };

    // Emit initial ready/idle for the default session
    emit_output(&app, "default", SessionOutput::ready());
    emit_output(&app, "default", SessionOutput::idle());

    // Spawn the output handler
    tokio::spawn(async move {
        let mut rx = output_rx;
        tracing::info!("Session output handler started");

        while let Some((session_id, output)) = rx.recv().await {
            tracing::debug!("Received output for session {}: {:?}", session_id, std::mem::discriminant(&output));
            emit_output(&app, &session_id, output);
        }

        tracing::info!("Session output handler ended");
    });

    tracing::info!("start_loop completed");
    Ok(())
}

/// Emit a session output to the frontend
fn emit_output(app: &tauri::AppHandle, session_id: &str, output: SessionOutput) {
    use tauri::Emitter;

    // Emit as a tagged event with session ID
    #[derive(serde::Serialize)]
    struct SessionEvent {
        session_id: String,
        #[serde(flatten)]
        output: SessionOutput,
    }

    let event = SessionEvent {
        session_id: session_id.to_string(),
        output: output.clone(),
    };

    // Emit to the general channel (for backwards compatibility)
    if let Err(e) = app.emit("loop_output", &event) {
        tracing::error!("Failed to emit loop_output: {}", e);
    }

    // Also emit to session-specific channel
    let channel = format!("session_output:{}", session_id);
    if let Err(e) = app.emit(&channel, &output) {
        tracing::error!("Failed to emit to {}: {}", channel, e);
    }
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
