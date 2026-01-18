//! Tauri commands exposed to the frontend

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::agentic_loop::{
    AgenticLoop, ApprovalConfig, ApprovalLevel, DefaultToolExecutor,
    LoopHandle, LoopState,
};
use crate::chat::{create_provider_from_config, ChatMessage, ChatSession, ToolCallInfo, ToolCallStatus};
use crate::state::{AppState, Settings, TaskState, TaskStatus};

/// Agent information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tool_count: usize,
}

/// Get list of available agents
#[tauri::command]
pub async fn get_agents(state: State<'_, AppState>) -> Result<Vec<AgentInfo>, String> {
    let registry = state.registry.read().await;
    let agents = registry
        .list()
        .into_iter()
        .map(|a| AgentInfo {
            id: a.id,
            name: a.name,
            description: a.description,
            tool_count: a.tool_count,
        })
        .collect();
    Ok(agents)
}

/// Task execution request
#[derive(Debug, Deserialize)]
pub struct TaskRequest {
    pub description: String,
    pub agent_id: Option<String>,
    pub context: Option<serde_json::Value>,
}

/// Execute a task
#[tauri::command]
pub async fn execute_task(
    request: TaskRequest,
    _state: State<'_, AppState>,
) -> Result<TaskState, String> {
    let task_id = uuid::Uuid::new_v4().to_string();

    // In a real implementation, this would:
    // 1. Create a Task
    // 2. Plan it using TaskPlanner
    // 3. Execute it using TaskExecutor with appropriate agent
    // 4. Stream updates via events

    Ok(TaskState {
        id: task_id,
        description: request.description,
        status: TaskStatus::Pending,
        progress: 0.0,
        started_at: chrono::Utc::now(),
        completed_at: None,
    })
}

/// Get status of a running task
#[tauri::command]
pub async fn get_task_status(_task_id: String) -> Result<TaskState, String> {
    // In a real implementation, look up task from storage
    Err("Task not found".to_string())
}

/// Cancel a running task
#[tauri::command]
pub async fn cancel_task(_task_id: String) -> Result<(), String> {
    // In a real implementation, signal cancellation
    Ok(())
}

/// File entry for directory listing
#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<String>,
}

/// List files in a directory
#[tauri::command]
pub async fn list_files(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileEntry>, String> {
    let full_path = state.workspace_path.join(&path);

    let mut entries = Vec::new();
    let mut dir = tokio::fs::read_dir(&full_path)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
        let metadata = entry.metadata().await.ok();
        let file_type = entry.file_type().await.ok();

        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            is_dir: file_type.map(|t| t.is_dir()).unwrap_or(false),
            size: metadata.as_ref().map(|m| m.len()),
            modified: metadata.and_then(|m| {
                m.modified().ok().map(|t| {
                    chrono::DateTime::<chrono::Utc>::from(t)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
            }),
        });
    }

    Ok(entries)
}

/// Read a file's contents
#[tauri::command]
pub async fn read_file(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let full_path = state.workspace_path.join(&path);
    tokio::fs::read_to_string(&full_path)
        .await
        .map_err(|e| e.to_string())
}

/// Write content to a file
#[tauri::command]
pub async fn write_file(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let full_path = state.workspace_path.join(&path);
    tokio::fs::write(&full_path, content)
        .await
        .map_err(|e| e.to_string())
}

/// Command execution request
#[derive(Debug, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    pub working_dir: Option<String>,
    pub timeout_secs: Option<u64>,
}

/// Command execution result
#[derive(Debug, Serialize)]
pub struct CommandResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// Execute a shell command
#[tauri::command]
pub async fn execute_command(
    request: CommandRequest,
    state: State<'_, AppState>,
) -> Result<CommandResult, String> {
    let working_dir = request
        .working_dir
        .map(|d| state.workspace_path.join(d))
        .unwrap_or_else(|| state.workspace_path.clone());

    let timeout = std::time::Duration::from_secs(request.timeout_secs.unwrap_or(30));

    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&request.command)
            .current_dir(&working_dir)
            .output(),
    )
    .await
    .map_err(|_| "Command timed out".to_string())?
    .map_err(|e| e.to_string())?;

    Ok(CommandResult {
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        success: output.status.success(),
    })
}

/// Get application settings
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let cm = state.config_manager.read().await;
    Ok(Settings::from(cm.config()))
}

/// Update application settings
#[tauri::command]
pub async fn update_settings(
    settings: Settings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cm = state.config_manager.write().await;

    // Update default provider if provider_type changed
    let provider_type = &settings.provider.provider_type;
    cm.set_default_provider(provider_type);

    // Update provider-specific settings
    if let Some(provider) = cm.config_mut().get_provider_mut(provider_type) {
        provider.model = settings.provider.model;
        provider.base_url = settings.provider.base_url;

        if let Some(key) = settings.provider.api_key {
            if !key.is_empty() {
                provider.api_key = Some(key);
            }
        }
    }

    // Update approval settings
    let config = cm.config_mut();
    config.approval.auto_approve_level = settings.approval.auto_approve_level;
    config.approval.show_dialogs = settings.approval.show_confirmation_dialogs;

    Ok(())
}

/// Save settings to disk
#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>) -> Result<(), String> {
    let cm = state.config_manager.read().await;
    cm.save().map_err(|e| e.to_string())
}

/// Check if API key is configured
#[tauri::command]
pub async fn check_api_key(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.has_api_key().await)
}

// ============================================================================
// Chat Commands
// ============================================================================

/// Session info for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub message_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Create a new chat session
#[tauri::command]
pub async fn create_session(state: State<'_, AppState>) -> Result<SessionInfo, String> {
    let cm = state.config_manager.read().await;
    let provider_config = cm
        .config()
        .get_default_provider()
        .ok_or_else(|| "No default provider configured".to_string())?;
    let provider = create_provider_from_config(provider_config)?;

    let session = ChatSession::new(provider);
    let info = SessionInfo {
        id: session.id.clone(),
        message_count: 0,
        created_at: chrono::Utc::now(),
    };

    drop(cm); // Release read lock before acquiring write lock

    let mut sessions = state.sessions.write().await;
    sessions.insert(session.id.clone(), session);

    Ok(info)
}

/// Get all active sessions
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionInfo>, String> {
    let sessions = state.sessions.read().await;
    let infos: Vec<SessionInfo> = sessions
        .values()
        .map(|s| SessionInfo {
            id: s.id.clone(),
            message_count: s.messages.len(),
            created_at: s
                .messages
                .first()
                .map(|m| m.timestamp)
                .unwrap_or_else(chrono::Utc::now),
        })
        .collect();
    Ok(infos)
}

/// Get messages from a session
#[tauri::command]
pub async fn get_session_messages(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChatMessage>, String> {
    let sessions = state.sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    Ok(session.messages.clone())
}

/// Send a message to a chat session
#[tauri::command]
pub async fn send_message(
    session_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<ChatMessage, String> {
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    session.send_message(content).await
}

/// Execute a pending tool call
#[tauri::command]
pub async fn execute_tool(
    session_id: String,
    tool_call_id: String,
    state: State<'_, AppState>,
) -> Result<Option<ChatMessage>, String> {
    // First, get the tool call details
    let tool_call = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        session
            .messages
            .iter()
            .flat_map(|m| m.tool_calls.iter())
            .find(|tc| tc.id == tool_call_id)
            .cloned()
            .ok_or_else(|| format!("Tool call {} not found", tool_call_id))?
    };

    // Execute the tool
    let result = execute_tool_impl(&tool_call, &state).await?;

    // Update session with result
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    session.execute_tool_call(&tool_call_id, result).await
}

/// Execute tool implementation
async fn execute_tool_impl(
    tool_call: &ToolCallInfo,
    state: &State<'_, AppState>,
) -> Result<String, String> {
    match tool_call.name.as_str() {
        "read_file" => {
            let path = tool_call.arguments["path"]
                .as_str()
                .ok_or("Missing path parameter")?;
            let full_path = state.workspace_path.join(path);
            tokio::fs::read_to_string(&full_path)
                .await
                .map_err(|e| e.to_string())
        }
        "write_file" => {
            let path = tool_call.arguments["path"]
                .as_str()
                .ok_or("Missing path parameter")?;
            let content = tool_call.arguments["content"]
                .as_str()
                .ok_or("Missing content parameter")?;
            let full_path = state.workspace_path.join(path);
            tokio::fs::write(&full_path, content)
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!("Successfully wrote to {}", path))
        }
        "list_directory" => {
            let path = tool_call.arguments["path"]
                .as_str()
                .unwrap_or(".");
            let full_path = state.workspace_path.join(path);
            let mut entries = Vec::new();
            let mut dir = tokio::fs::read_dir(&full_path)
                .await
                .map_err(|e| e.to_string())?;
            while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
                let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                let name = entry.file_name().to_string_lossy().to_string();
                entries.push(if is_dir {
                    format!("{}/", name)
                } else {
                    name
                });
            }
            Ok(entries.join("\n"))
        }
        "execute_command" => {
            let command = tool_call.arguments["command"]
                .as_str()
                .ok_or("Missing command parameter")?;
            let working_dir = tool_call.arguments["working_dir"]
                .as_str()
                .map(|d| state.workspace_path.join(d))
                .unwrap_or_else(|| state.workspace_path.clone());

            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&working_dir)
                .output()
                .await
                .map_err(|e| e.to_string())?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            Ok(format!(
                "Exit code: {}\nStdout:\n{}\nStderr:\n{}",
                exit_code, stdout, stderr
            ))
        }
        "search_files" => {
            let pattern = tool_call.arguments["pattern"]
                .as_str()
                .ok_or("Missing pattern parameter")?;
            let path = tool_call.arguments["path"]
                .as_str()
                .unwrap_or(".");
            let full_path = state.workspace_path.join(path);

            // Use glob to search
            let glob_pattern = full_path.join(pattern).to_string_lossy().to_string();
            let matches: Vec<String> = glob::glob(&glob_pattern)
                .map_err(|e| e.to_string())?
                .filter_map(|entry| entry.ok())
                .map(|p| p.to_string_lossy().to_string())
                .collect();

            if matches.is_empty() {
                Ok("No files found matching the pattern".to_string())
            } else {
                Ok(matches.join("\n"))
            }
        }
        _ => Err(format!("Unknown tool: {}", tool_call.name)),
    }
}

/// Approve a pending tool call
#[tauri::command]
pub async fn approve_tool_call(
    session_id: String,
    tool_call_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    for msg in &mut session.messages {
        for tc in &mut msg.tool_calls {
            if tc.id == tool_call_id {
                tc.status = ToolCallStatus::Approved;
                return Ok(());
            }
        }
    }
    Err(format!("Tool call {} not found", tool_call_id))
}

/// Reject a pending tool call
#[tauri::command]
pub async fn reject_tool_call(
    session_id: String,
    tool_call_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    for msg in &mut session.messages {
        for tc in &mut msg.tool_calls {
            if tc.id == tool_call_id {
                tc.status = ToolCallStatus::Rejected;
                return Ok(());
            }
        }
    }
    Err(format!("Tool call {} not found", tool_call_id))
}

/// Delete a chat session
#[tauri::command]
pub async fn delete_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut sessions = state.sessions.write().await;
    sessions.remove(&session_id);
    Ok(())
}

// ============================================================================
// Agentic Loop Commands
// ============================================================================

/// Start an agentic loop for a session
#[tauri::command]
pub async fn start_loop(
    session_id: String,
    prompt: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Check if loop already running for this session
    {
        let handles = state.loop_handles.read().await;
        if handles.contains_key(&session_id) {
            return Err("Loop already running for this session".to_string());
        }
    }

    // Get approval config from settings
    let approval_config = {
        let cm = state.config_manager.read().await;
        let config = cm.config();
        let level = match config.approval.auto_approve_level.as_str() {
            "none" => ApprovalLevel::None,
            "low" => ApprovalLevel::Low,
            "medium" => ApprovalLevel::Medium,
            "high" => ApprovalLevel::High,
            _ => ApprovalLevel::Low,
        };
        let mut ac = ApprovalConfig::default();
        ac.level = level;
        ac
    };

    // Create the loop
    let mut agentic_loop = AgenticLoop::new(session_id.clone(), app.clone(), approval_config);
    let handle = LoopHandle::new(session_id.clone(), agentic_loop.command_sender());

    // Store the handle
    {
        let mut handles = state.loop_handles.write().await;
        handles.insert(session_id.clone(), handle);
    }

    // Get session and run loop in background
    let sessions = state.sessions.clone();
    let workspace_path = state.workspace_path.clone();
    let loop_handles = state.loop_handles.clone();
    let session_id_clone = session_id.clone();

    tokio::spawn(async move {
        let result = {
            let mut sessions_guard = sessions.write().await;
            if let Some(session) = sessions_guard.get_mut(&session_id_clone) {
                let executor = DefaultToolExecutor::new(workspace_path);
                agentic_loop.run(session, prompt, executor).await
            } else {
                Err("Session not found".to_string())
            }
        };

        // Remove the handle when done
        {
            let mut handles = loop_handles.write().await;
            handles.remove(&session_id_clone);
        }

        if let Err(e) = result {
            tracing::error!("Agentic loop error: {}", e);
        }
    });

    Ok(())
}

/// Stop an agentic loop
#[tauri::command]
pub async fn stop_loop(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let handles = state.loop_handles.read().await;
    if let Some(handle) = handles.get(&session_id) {
        handle.cancel().await
    } else {
        Err("No active loop for this session".to_string())
    }
}

/// Approve pending tools in an agentic loop
#[tauri::command]
pub async fn approve_loop_tools(
    session_id: String,
    tool_ids: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let handles = state.loop_handles.read().await;
    if let Some(handle) = handles.get(&session_id) {
        match tool_ids {
            Some(ids) if !ids.is_empty() => handle.approve_selected(ids).await,
            _ => handle.approve_all().await,
        }
    } else {
        Err("No active loop for this session".to_string())
    }
}

/// Reject pending tools in an agentic loop
#[tauri::command]
pub async fn reject_loop_tools(
    session_id: String,
    tool_ids: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let handles = state.loop_handles.read().await;
    if let Some(handle) = handles.get(&session_id) {
        match tool_ids {
            Some(ids) if !ids.is_empty() => handle.reject_selected(ids).await,
            _ => handle.reject_all().await,
        }
    } else {
        Err("No active loop for this session".to_string())
    }
}

/// Get the current loop state for a session
#[tauri::command]
pub async fn get_loop_state(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<Option<LoopState>, String> {
    let handles = state.loop_handles.read().await;
    Ok(if handles.contains_key(&session_id) {
        Some(LoopState::WaitingForLlm) // Simplified - would need to track actual state
    } else {
        None
    })
}

/// Check if a loop is active for a session
#[tauri::command]
pub async fn is_loop_active(session_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let handles = state.loop_handles.read().await;
    Ok(handles.contains_key(&session_id))
}

// ============================================================================
// Streaming Commands
// ============================================================================

// ============================================================================
// Skills/Commands
// ============================================================================

/// Execute a skill/command
#[tauri::command]
pub async fn execute_skill(
    skill_name: String,
    args: String,
    state: State<'_, AppState>,
) -> Result<cowork_core::skills::SkillResult, String> {
    let registry = cowork_core::skills::SkillRegistry::with_builtins(state.workspace_path.clone());

    let ctx = cowork_core::skills::SkillContext {
        workspace: state.workspace_path.clone(),
        args,
        data: std::collections::HashMap::new(),
    };

    Ok(registry.execute(&skill_name, ctx).await)
}

/// List available skills
#[tauri::command]
pub async fn list_skills(
    state: State<'_, AppState>,
) -> Result<Vec<cowork_core::skills::SkillInfo>, String> {
    let registry = cowork_core::skills::SkillRegistry::with_builtins(state.workspace_path.clone());
    Ok(registry.list_user_invocable())
}

/// Execute a slash command (parses command string)
#[tauri::command]
pub async fn execute_command_string(
    command: String,
    state: State<'_, AppState>,
) -> Result<cowork_core::skills::SkillResult, String> {
    let registry = cowork_core::skills::SkillRegistry::with_builtins(state.workspace_path.clone());
    Ok(registry.execute_command(&command, state.workspace_path.clone()).await)
}

/// Send a message with streaming response
#[tauri::command]
pub async fn send_message_stream(
    session_id: String,
    content: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use crate::streaming::{StreamEvent, StreamingMessage};
    use tauri::Emitter;

    let message_id = uuid::Uuid::new_v4().to_string();

    // Add user message to session
    {
        let mut sessions = state.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        let user_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: content.clone(),
            tool_calls: Vec::new(),
            timestamp: chrono::Utc::now(),
        };
        session.messages.push(user_msg);
    }

    // Create streaming message handler
    let streaming_msg = StreamingMessage::new(session_id.clone(), message_id.clone(), app.clone());
    streaming_msg.start();

    // Get provider and send streaming request
    let sessions = state.sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Build LLM request
    let llm_messages: Vec<cowork_core::provider::LlmMessage> = session
        .messages
        .iter()
        .map(|m| cowork_core::provider::LlmMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();

    let request = cowork_core::provider::LlmRequest::new(llm_messages)
        .with_system(&session.system_prompt)
        .with_tools(session.available_tools.clone())
        .with_max_tokens(4096);

    drop(sessions);

    // Get response (non-streaming for now, emit final result)
    let sessions = state.sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let response = session
        .provider
        .complete(request)
        .await
        .map_err(|e| e.to_string())?;

    drop(sessions);

    // Convert tool calls
    let tool_calls: Vec<ToolCallInfo> = response
        .tool_calls
        .iter()
        .map(|tc| ToolCallInfo {
            id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            status: ToolCallStatus::Pending,
            result: None,
        })
        .collect();

    // Emit text content
    if let Some(ref text) = response.content {
        let event_name = format!("stream:{}", session_id);
        let _ = app.emit(
            &event_name,
            StreamEvent::TextDelta {
                session_id: session_id.clone(),
                message_id: message_id.clone(),
                delta: text.clone(),
                accumulated: text.clone(),
            },
        );
    }

    // Emit tool calls
    for tc in &tool_calls {
        streaming_msg.complete_tool_call(tc.clone());
    }

    // End stream
    streaming_msg.end(&response.finish_reason);

    // Add assistant message to session
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let assistant_msg = ChatMessage {
        id: message_id.clone(),
        role: "assistant".to_string(),
        content: response.content.unwrap_or_default(),
        tool_calls,
        timestamp: chrono::Utc::now(),
    };
    session.messages.push(assistant_msg);

    Ok(message_id)
}

// ============================================================================
// Context Management Commands
// ============================================================================

/// Context usage information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct ContextUsageInfo {
    pub used_tokens: usize,
    pub limit_tokens: usize,
    pub used_percentage: f64,
    pub remaining_tokens: usize,
    pub should_compact: bool,
    pub system_tokens: usize,
    pub conversation_tokens: usize,
    pub tool_tokens: usize,
    pub memory_tokens: usize,
}

/// Get context usage for a session
#[tauri::command]
pub async fn get_context_usage(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<ContextUsageInfo, String> {
    use cowork_core::context::{ContextMonitor, Message, MessageRole};
    use cowork_core::provider::ProviderType;

    let sessions = state.sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Create a monitor (default to Anthropic for now)
    let monitor = ContextMonitor::new(ProviderType::Anthropic);

    // Convert ChatMessages to context Messages
    let context_messages: Vec<Message> = session
        .messages
        .iter()
        .map(|m| Message {
            role: match m.role.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "system" => MessageRole::System,
                _ => MessageRole::Tool,
            },
            content: m.content.clone(),
            timestamp: m.timestamp,
        })
        .collect();

    let usage = monitor.calculate_usage(&context_messages, &session.system_prompt, None);

    Ok(ContextUsageInfo {
        used_tokens: usage.used_tokens,
        limit_tokens: usage.limit_tokens,
        used_percentage: usage.used_percentage,
        remaining_tokens: usage.remaining_tokens,
        should_compact: usage.should_compact,
        system_tokens: usage.breakdown.system_tokens,
        conversation_tokens: usage.breakdown.conversation_tokens,
        tool_tokens: usage.breakdown.tool_tokens,
        memory_tokens: usage.breakdown.memory_tokens,
    })
}

/// Compact result for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct CompactResultInfo {
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub messages_summarized: usize,
    pub messages_kept: usize,
}

/// Compact a session's conversation
#[tauri::command]
pub async fn compact_session(
    session_id: String,
    preserve_instructions: Option<String>,
    state: State<'_, AppState>,
) -> Result<CompactResultInfo, String> {
    use cowork_core::context::{
        CompactConfig, ContextMonitor, ConversationSummarizer, Message, MessageRole, SummarizerConfig,
    };
    use cowork_core::provider::ProviderType;

    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Create monitor and summarizer
    let monitor = ContextMonitor::new(ProviderType::Anthropic);
    let summarizer = ConversationSummarizer::new(SummarizerConfig::default());

    // Convert ChatMessages to context Messages
    let context_messages: Vec<Message> = session
        .messages
        .iter()
        .map(|m| Message {
            role: match m.role.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "system" => MessageRole::System,
                _ => MessageRole::Tool,
            },
            content: m.content.clone(),
            timestamp: m.timestamp,
        })
        .collect();

    // Create compact config
    let config = CompactConfig::from_command(preserve_instructions).without_llm();

    // Perform compaction
    let result = summarizer
        .compact(&context_messages, monitor.counter(), config, None)
        .await
        .map_err(|e| e.to_string())?;

    // Apply to session
    let summary_msg = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "system".to_string(),
        content: result.summary.content.clone(),
        tool_calls: Vec::new(),
        timestamp: result.summary.timestamp,
    };

    let kept_chat_messages: Vec<ChatMessage> = result
        .kept_messages
        .iter()
        .map(|m| ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool => "tool",
            }
            .to_string(),
            content: m.content.clone(),
            tool_calls: Vec::new(),
            timestamp: m.timestamp,
        })
        .collect();

    session.messages.clear();
    session.messages.push(summary_msg);
    session.messages.extend(kept_chat_messages);

    Ok(CompactResultInfo {
        tokens_before: result.tokens_before,
        tokens_after: result.tokens_after,
        messages_summarized: result.messages_summarized,
        messages_kept: result.messages_kept,
    })
}

/// Clear a session's conversation
#[tauri::command]
pub async fn clear_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Clear all messages
    session.messages.clear();

    Ok(())
}

/// Memory file information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct MemoryFileInfo {
    pub path: String,
    pub tier: String,
    pub size: usize,
}

/// Memory hierarchy information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct MemoryHierarchyInfo {
    pub files: Vec<MemoryFileInfo>,
    pub total_size: usize,
    pub combined_content: String,
}

/// Get memory hierarchy for the workspace
#[tauri::command]
pub async fn get_memory_hierarchy(
    state: State<'_, AppState>,
) -> Result<MemoryHierarchyInfo, String> {
    use cowork_core::context::ContextGatherer;

    let gatherer = ContextGatherer::new(&state.workspace_path);
    let hierarchy = gatherer.gather_memory_hierarchy().await;

    let files: Vec<MemoryFileInfo> = hierarchy
        .files
        .iter()
        .map(|f| MemoryFileInfo {
            path: f.path.to_string_lossy().to_string(),
            tier: f.tier.to_string(),
            size: f.size,
        })
        .collect();

    Ok(MemoryHierarchyInfo {
        files,
        total_size: hierarchy.total_size,
        combined_content: hierarchy.combined_content,
    })
}
