//! Simplified Tauri commands for the channel-based loop
//!
//! Just three main commands:
//! - start_loop: Start the loop (called once at app start)
//! - send_message: Send a message to the loop
//! - stop_loop: Stop the loop

use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use cowork_core::provider::create_provider_from_provider_config;
use cowork_core::{ApprovalLevel, ToolApprovalConfig};

use crate::loop_channel::{LoopInput, LoopOutput};
use crate::simple_loop::{LoopInputHandle, SimpleLoop};
use crate::state::AppState;

/// Start the agentic loop
///
/// This should be called once when the app starts or when a new session begins.
/// The loop will run continuously, waiting for input when idle.
#[tauri::command]
pub async fn start_loop(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("start_loop called");

    // Check if loop is already running
    {
        let guard = state.loop_input.read().await;
        if guard.is_some() {
            tracing::warn!("Loop already running");
            return Err("Loop already running".to_string());
        }
    }

    // Get provider from config
    tracing::info!("Getting provider config...");
    let cm = state.config_manager.read().await;
    let provider_config = cm
        .config()
        .get_default_provider()
        .ok_or_else(|| "No provider configured".to_string())?;

    tracing::info!("Creating provider...");
    let provider = create_provider_from_provider_config(provider_config)
        .map_err(|e| e.to_string())?;

    // Get approval config
    let approval_level: ApprovalLevel = cm
        .config()
        .approval
        .auto_approve_level
        .parse()
        .unwrap_or(ApprovalLevel::Low);
    let mut approval_config = ToolApprovalConfig::default();
    approval_config.set_level(approval_level);

    drop(cm);

    // Create channels
    let (input_tx, input_rx) = mpsc::channel::<LoopInput>(32);

    // Create and store the input handle
    let handle = LoopInputHandle::new(input_tx);
    {
        let mut guard = state.loop_input.write().await;
        *guard = Some(handle);
    }

    // Emit Ready and Idle BEFORE spawning (synchronously, so frontend listener catches them)
    tracing::info!("Emitting Ready and Idle events...");
    if let Err(e) = app.emit("loop_output", &LoopOutput::Ready) {
        tracing::error!("Failed to emit Ready: {}", e);
    }
    if let Err(e) = app.emit("loop_output", &LoopOutput::Idle) {
        tracing::error!("Failed to emit Idle: {}", e);
    }

    // Create the loop (after emitting initial events)
    let simple_loop = SimpleLoop::new(
        input_rx,
        app,
        "loop_output".to_string(), // Single event name for all output
        Arc::new(provider),
        state.workspace_path.clone(),
        approval_config,
    );

    // Run the loop in background (it will skip initial Ready/Idle since we emitted them)
    tracing::info!("Spawning loop task...");
    tokio::spawn(async move {
        simple_loop.run_without_initial_events().await;
    });

    tracing::info!("start_loop returning Ok");
    Ok(())
}

/// Send a message to the loop
#[tauri::command]
pub async fn send_message(
    content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let guard = state.loop_input.read().await;
    let handle = guard
        .as_ref()
        .ok_or_else(|| "Loop not started".to_string())?;

    handle.send_message(content).await
}

/// Stop the loop
#[tauri::command]
pub async fn stop_loop(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.loop_input.read().await;
    let handle = guard
        .as_ref()
        .ok_or_else(|| "Loop not started".to_string())?;

    handle.stop().await?;

    // Clear the handle
    drop(guard);
    let mut guard = state.loop_input.write().await;
    *guard = None;

    Ok(())
}

/// Check if loop is running
#[tauri::command]
pub async fn is_loop_running(state: State<'_, AppState>) -> Result<bool, String> {
    let guard = state.loop_input.read().await;
    Ok(guard.is_some())
}

/// Approve a pending tool
#[tauri::command]
pub async fn approve_tool(
    tool_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let guard = state.loop_input.read().await;
    let handle = guard
        .as_ref()
        .ok_or_else(|| "Loop not started".to_string())?;

    handle.send(LoopInput::ApproveTool(tool_id)).await
}

/// Reject a pending tool
#[tauri::command]
pub async fn reject_tool(
    tool_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let guard = state.loop_input.read().await;
    let handle = guard
        .as_ref()
        .ok_or_else(|| "Loop not started".to_string())?;

    handle.send(LoopInput::RejectTool(tool_id)).await
}

// Settings commands are used directly from commands module via lib.rs invoke_handler
