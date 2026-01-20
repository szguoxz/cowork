//! Simplified Tauri commands for the channel-based loop
//!
//! Just three main commands:
//! - start_loop: Start the loop (called once at app start)
//! - send_message: Send a message to the loop
//! - stop_loop: Stop the loop

use std::sync::Arc;
use tauri::{AppHandle, State};
use tokio::sync::mpsc;

use cowork_core::provider::create_provider_from_provider_config;
use cowork_core::{ApprovalLevel, ToolApprovalConfig};

use crate::loop_channel::LoopInput;
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
    // Check if loop is already running
    {
        let guard = state.loop_input.read().await;
        if guard.is_some() {
            return Err("Loop already running".to_string());
        }
    }

    // Get provider from config
    let cm = state.config_manager.read().await;
    let provider_config = cm
        .config()
        .get_default_provider()
        .ok_or_else(|| "No provider configured".to_string())?;

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

    // Create the loop
    let simple_loop = SimpleLoop::new(
        input_rx,
        app,
        "loop_output".to_string(), // Single event name for all output
        Arc::new(provider),
        state.workspace_path.clone(),
        approval_config,
    );

    // Run the loop in background
    tokio::spawn(async move {
        simple_loop.run().await;
    });

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
