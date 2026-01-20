//! Tauri commands exposed to the frontend

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::agentic_loop::{
    AgenticLoop, ApprovalConfig, ApprovalLevel, DefaultToolExecutor,
    LoopHandle, LoopState,
};
use crate::chat::{create_provider_from_config, ChatMessage, ChatSession, ToolCallInfo, ToolCallStatus};
use crate::session_storage::{generate_title, SessionData, SessionStorage};
use crate::state::{AppState, Settings, TaskState, TaskStatus};

/// Helper function to auto-save a session
async fn auto_save_session(
    session_id: &str,
    state: &AppState,
) -> Result<(), String> {
    let sessions = state.sessions.read().await;
    let session = match sessions.get(session_id) {
        Some(s) => s,
        None => return Ok(()), // Session not found, skip save
    };

    // Skip saving empty sessions
    if session.messages.is_empty() {
        return Ok(());
    }

    // Get provider info from config
    let cm = state.config_manager.read().await;
    let (provider_type, model) = if let Some(provider) = cm.config().get_default_provider() {
        (provider.provider_type.clone(), provider.model.clone())
    } else {
        ("unknown".to_string(), "unknown".to_string())
    };

    let now = chrono::Utc::now();
    let created_at = session
        .messages
        .first()
        .map(|m| m.timestamp)
        .unwrap_or(now);

    let session_data = SessionData {
        id: session.id.clone(),
        title: generate_title(&session.messages),
        messages: session.messages.clone(),
        system_prompt: session.system_prompt.clone(),
        provider_type,
        model,
        created_at,
        updated_at: now,
    };

    drop(cm);
    drop(sessions);

    let storage = SessionStorage::new();
    storage.save(&session_data).map_err(|e| e.to_string())?;

    Ok(())
}

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

    #[cfg(windows)]
    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("cmd")
            .args(["/C", &request.command])
            .current_dir(&working_dir)
            .output(),
    )
    .await
    .map_err(|_| "Command timed out".to_string())?
    .map_err(|e| e.to_string())?;

    #[cfg(not(windows))]
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
    use cowork_core::ProviderConfig;

    let mut cm = state.config_manager.write().await;

    // Update default provider if provider_type changed
    let provider_type = &settings.provider.provider_type;
    cm.set_default_provider(provider_type);

    // Create provider config if it doesn't exist
    if cm.config().get_provider(provider_type).is_none() {
        let new_provider = match provider_type.as_str() {
            "anthropic" => ProviderConfig::anthropic(),
            "openai" => ProviderConfig::openai(),
            "gemini" => ProviderConfig::gemini(),
            "groq" => ProviderConfig::groq(),
            "deepseek" => ProviderConfig::deepseek(),
            "cohere" => ProviderConfig::cohere(),
            "together" => ProviderConfig::together(),
            "fireworks" => ProviderConfig::fireworks(),
            "zai" => ProviderConfig::zai(),
            "nebius" => ProviderConfig::nebius(),
            "mimo" => ProviderConfig::mimo(),
            "bigmodel" => ProviderConfig::bigmodel(),
            "xai" => {
                // xAI doesn't have a factory, create manually
                let mut p = ProviderConfig::anthropic();
                p.provider_type = "xai".to_string();
                p.api_key_env = Some("XAI_API_KEY".to_string());
                p.model = "grok-2".to_string();
                p
            }
            "ollama" => {
                let mut p = ProviderConfig::anthropic();
                p.provider_type = "ollama".to_string();
                p.api_key = None;
                p.api_key_env = None;
                p.model = "llama3.2".to_string();
                p.base_url = Some("http://localhost:11434".to_string());
                p
            }
            _ => ProviderConfig::anthropic(),
        };
        cm.set_provider(provider_type, new_provider);
    }

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

/// Test API connection result
#[derive(Debug, Clone, Serialize)]
pub struct ApiTestResult {
    pub success: bool,
    pub message: String,
}

/// Test API connection with given provider and key
#[tauri::command]
pub async fn test_api_connection(
    provider_type: String,
    api_key: String,
    model: Option<String>,
) -> Result<ApiTestResult, String> {
    use cowork_core::provider::{GenAIProvider, LlmMessage, ProviderType};

    // Parse provider type
    let pt = match provider_type.to_lowercase().as_str() {
        "anthropic" => ProviderType::Anthropic,
        "openai" => ProviderType::OpenAI,
        "gemini" => ProviderType::Gemini,
        "groq" => ProviderType::Groq,
        "deepseek" => ProviderType::DeepSeek,
        "xai" => ProviderType::XAI,
        "together" => ProviderType::Together,
        "fireworks" => ProviderType::Fireworks,
        "zai" => ProviderType::Zai,
        "nebius" => ProviderType::Nebius,
        "mimo" => ProviderType::MIMO,
        "bigmodel" => ProviderType::BigModel,
        "ollama" => ProviderType::Ollama,
        _ => return Err(format!("Unknown provider type: {}", provider_type)),
    };

    // Get default model if not provided
    let model_str = model.unwrap_or_else(|| match pt {
        ProviderType::Anthropic => "claude-sonnet-4-20250514".to_string(),
        ProviderType::OpenAI => "gpt-4o".to_string(),
        ProviderType::Gemini => "gemini-2.0-flash".to_string(),
        ProviderType::Groq => "llama-3.3-70b-versatile".to_string(),
        ProviderType::DeepSeek => "deepseek-chat".to_string(),
        ProviderType::XAI => "grok-2".to_string(),
        ProviderType::Together => "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo".to_string(),
        ProviderType::Fireworks => "accounts/fireworks/models/llama-v3p1-70b-instruct".to_string(),
        ProviderType::Zai => "glm-4-plus".to_string(),
        ProviderType::Nebius => "meta-llama/Meta-Llama-3.1-70B-Instruct".to_string(),
        ProviderType::MIMO => "mimo-v2-flash".to_string(),
        ProviderType::BigModel => "glm-4-plus".to_string(),
        ProviderType::Ollama => "llama3.2".to_string(),
        _ => "".to_string(),
    });

    // Create provider and test
    let provider = GenAIProvider::with_api_key(pt, &api_key, Some(&model_str));

    let test_messages = vec![LlmMessage {
        role: "user".to_string(),
        content: "Say 'hello' in one word.".to_string(),
    }];

    match provider.chat(test_messages, None).await {
        Ok(_) => Ok(ApiTestResult {
            success: true,
            message: "Connection successful!".to_string(),
        }),
        Err(e) => Ok(ApiTestResult {
            success: false,
            message: format!("Connection failed: {}", e),
        }),
    }
}

/// Check if onboarding is complete (config exists and has API key)
#[tauri::command]
pub async fn is_setup_complete(state: State<'_, AppState>) -> Result<bool, String> {
    let cm = state.config_manager.read().await;
    Ok(cm.is_setup_complete())
}

/// Model info for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfoResponse {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub recommended: bool,
}

/// Fetch available models for a provider
#[tauri::command]
pub async fn fetch_provider_models(
    provider_type: String,
    api_key: String,
) -> Result<Vec<ModelInfoResponse>, String> {
    use cowork_core::provider::{fetch_models, ProviderType};

    // Parse provider type
    let pt: ProviderType = provider_type
        .parse()
        .map_err(|e: String| e)?;

    // Fetch models from provider API
    let models = fetch_models(pt, &api_key)
        .await
        .map_err(|e| e.to_string())?;

    // Convert to response format
    let response: Vec<ModelInfoResponse> = models
        .into_iter()
        .map(|m| ModelInfoResponse {
            id: m.id,
            name: m.name,
            description: m.description,
            recommended: m.recommended,
        })
        .collect();

    Ok(response)
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
    let result = {
        let mut sessions = state.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        session.send_message(content).await
    };

    // Auto-save after message exchange
    if result.is_ok() {
        let _ = auto_save_session(&session_id, &state).await;
    }

    result
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

            #[cfg(windows)]
            let output = tokio::process::Command::new("cmd")
                .args(["/C", command])
                .current_dir(&working_dir)
                .output()
                .await
                .map_err(|e| e.to_string())?;

            #[cfg(not(windows))]
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

/// Answer a question from the AI during an agentic loop
#[tauri::command]
pub async fn answer_loop_question(
    session_id: String,
    request_id: String,
    answers: std::collections::HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let handles = state.loop_handles.read().await;
    if let Some(handle) = handles.get(&session_id) {
        let answer = crate::agentic_loop::QuestionAnswer {
            request_id,
            answers,
        };
        handle.answer_question(answer).await
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
    use crate::streaming::StreamingMessage;
    use cowork_core::provider::StreamChunk;
    use tokio::sync::mpsc;

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
    let mut streaming_msg = StreamingMessage::new(session_id.clone(), message_id.clone(), app.clone());
    streaming_msg.start();

    // Build LLM messages from session
    let (llm_messages, system_prompt, tools, provider_type, api_key) = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        let llm_messages: Vec<cowork_core::provider::LlmMessage> = session
            .messages
            .iter()
            .map(|m| cowork_core::provider::LlmMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let system_prompt = session.system_prompt.clone();
        let tools = if session.available_tools.is_empty() {
            None
        } else {
            Some(session.available_tools.clone())
        };

        // Get provider info from config
        let cm = state.config_manager.read().await;
        let provider_config = cm.config().get_default_provider()
            .ok_or_else(|| "No provider configured".to_string())?;
        let provider_type = provider_config.provider_type.parse::<cowork_core::provider::ProviderType>()
            .map_err(|e| format!("Invalid provider type: {}", e))?;
        let api_key = provider_config.get_api_key();

        (llm_messages, system_prompt, tools, provider_type, api_key)
    };

    // Create provider for streaming
    let provider = if let Some(key) = api_key {
        cowork_core::provider::GenAIProvider::with_api_key(provider_type, &key, None)
            .with_system_prompt(&system_prompt)
    } else {
        cowork_core::provider::GenAIProvider::new(provider_type, None)
            .with_system_prompt(&system_prompt)
    };

    // Create channel for streaming chunks
    let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);

    // Spawn streaming task
    let stream_handle = tokio::spawn(async move {
        provider.chat_stream(llm_messages, tools, tx).await
    });

    // Track accumulated content and tool calls
    let mut accumulated_text = String::new();
    let mut accumulated_thinking = String::new();
    let mut tool_calls: Vec<ToolCallInfo> = Vec::new();

    // Process streaming chunks
    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Start => {
                // Already sent start event
            }
            StreamChunk::Thinking(text) => {
                accumulated_thinking.push_str(&text);
                streaming_msg.add_thinking(&text);
            }
            StreamChunk::TextDelta(text) => {
                accumulated_text.push_str(&text);
                streaming_msg.add_text(&text);
            }
            StreamChunk::ToolCallStart { id, name } => {
                streaming_msg.start_tool_call(id.clone(), name.clone());
                tool_calls.push(ToolCallInfo {
                    id,
                    name,
                    arguments: serde_json::Value::Null,
                    status: ToolCallStatus::Pending,
                    result: None,
                });
            }
            StreamChunk::ToolCallDelta { id, delta } => {
                streaming_msg.add_tool_arg(&id, &delta);
                // Update arguments in tool_calls
                if let Some(tc) = tool_calls.iter_mut().find(|t| t.id == id) {
                    if let Ok(args) = serde_json::from_str::<serde_json::Value>(&delta) {
                        tc.arguments = args;
                    }
                }
            }
            StreamChunk::ToolCallComplete(id) => {
                if let Some(tc) = tool_calls.iter().find(|t| t.id == id) {
                    streaming_msg.complete_tool_call(tc.clone());
                }
            }
            StreamChunk::End(reason) => {
                streaming_msg.end(&reason);
            }
            StreamChunk::Error(err) => {
                streaming_msg.error(&err);
                return Err(err);
            }
        }
    }

    // Wait for streaming to complete
    let result = stream_handle.await
        .map_err(|e| format!("Stream task failed: {}", e))?
        .map_err(|e| format!("Streaming error: {}", e))?;

    // Get finish reason and any remaining tool calls from result
    let finish_reason = match &result {
        cowork_core::provider::CompletionResult::Message(_) => "stop",
        cowork_core::provider::CompletionResult::ToolCalls(_) => "tool_calls",
    };

    // If we got tool calls from the final result but not during streaming, add them
    if let cowork_core::provider::CompletionResult::ToolCalls(pending_tools) = result {
        if tool_calls.is_empty() {
            for tc in pending_tools {
                tool_calls.push(ToolCallInfo {
                    id: tc.call_id,
                    name: tc.name,
                    arguments: tc.arguments,
                    status: ToolCallStatus::Pending,
                    result: None,
                });
            }
        }
    }

    // Add assistant message to session (include thinking content as metadata)
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Store thinking in content if present, marked appropriately
    let final_content = if !accumulated_thinking.is_empty() {
        format!("<thinking>\n{}\n</thinking>\n\n{}", accumulated_thinking, accumulated_text)
    } else {
        accumulated_text
    };

    let assistant_msg = ChatMessage {
        id: message_id.clone(),
        role: "assistant".to_string(),
        content: final_content,
        tool_calls,
        timestamp: chrono::Utc::now(),
    };
    session.messages.push(assistant_msg);

    drop(sessions);

    // Ensure stream is properly ended
    streaming_msg.end(finish_reason);

    // Auto-save after message exchange
    let _ = auto_save_session(&session_id, &state).await;

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

/// Quick start / help information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct QuickStartInfo {
    pub version: String,
    pub sections: Vec<HelpSection>,
}

/// A section of help content
#[derive(Debug, Clone, Serialize)]
pub struct HelpSection {
    pub title: String,
    pub content: String,
}

/// Get quick start / help information
#[tauri::command]
pub async fn get_quick_start() -> Result<QuickStartInfo, String> {
    Ok(QuickStartInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        sections: vec![
            HelpSection {
                title: "Getting Started".to_string(),
                content: r#"Welcome to Cowork! Here's how to get started:

1. **Set up your API key**
   - Go to Settings (gear icon) and enter your API key
   - Supported providers: Anthropic (Claude), OpenAI (GPT-4)

2. **Start chatting**
   - Type your message and press Enter or click Send
   - The AI can read files, write code, and execute commands

3. **Approve tool actions**
   - When the AI wants to make changes, you'll see a prompt
   - Click Approve (Y) or Reject (N) for each action"#.to_string(),
            },
            HelpSection {
                title: "Slash Commands".to_string(),
                content: r#"Use slash commands for common workflows:

**Git Commands**
  /commit         - Create a commit with auto-generated message
  /push           - Push commits to remote
  /pr             - Create a pull request
  /status         - Show git status
  /diff           - Show current changes

**Development**
  /test           - Run project tests
  /build          - Build the project
  /lint           - Run linter
  /format         - Format code

**Session**
  /clear          - Clear conversation
  /compact        - Summarize to reduce context
  /memory         - Manage CLAUDE.md files
  /help           - Show all commands"#.to_string(),
            },
            HelpSection {
                title: "Keyboard Shortcuts".to_string(),
                content: r#"**During Approval Prompts**
  Y           - Approve action
  N           - Reject action
  A           - Approve all pending
  Escape      - Cancel current operation

**In Chat**
  Enter       - Send message
  Ctrl+Enter  - New line
  Up/Down     - Navigate history"#.to_string(),
            },
            HelpSection {
                title: "Configuration".to_string(),
                content: r#"**Config File Location**
  ~/.config/cowork/config.toml

**Example Configuration**
```toml
default_provider = "anthropic"

[providers.anthropic]
provider_type = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"

[approval]
auto_approve_level = "medium"
```

**Approval Levels**
  none    - Approve everything manually
  low     - Auto-approve reads only
  medium  - Auto-approve reads and simple writes
  high    - Auto-approve most actions
  all     - Auto-approve everything"#.to_string(),
            },
            HelpSection {
                title: "Memory Files".to_string(),
                content: r#"Create CLAUDE.md files to provide persistent instructions:

**Project Instructions** (shared with team)
  ./CLAUDE.md
  ./.claude/CLAUDE.md

**Personal Settings** (gitignored)
  ./CLAUDE.local.md

**Global User Settings**
  ~/.claude/CLAUDE.md

**Example CLAUDE.md**
```markdown
# Project Instructions

## Tech Stack
- Rust with async/await
- SQLite for data storage

## Conventions
- Use snake_case for functions
- Add doc comments to public items
```"#.to_string(),
            },
            HelpSection {
                title: "Tips".to_string(),
                content: r#"- **Be specific**: "Add a logout button to the navbar" works better than "add logout"
- **Iterate**: Ask follow-up questions to refine the solution
- **Review changes**: Always review AI-generated code before committing
- **Use /compact**: If the conversation gets long, use /compact to summarize
- **Memory files**: Put project conventions in CLAUDE.md so the AI remembers them"#.to_string(),
            },
        ],
    })
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

// ============================================================================
// MCP Server Management Commands
// ============================================================================

/// MCP server information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct McpServerInfo {
    pub name: String,
    pub command: String,
    pub enabled: bool,
    pub status: String,
    pub tool_count: usize,
    pub error: Option<String>,
}

/// MCP tool information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub server: String,
}

/// List all MCP servers
#[tauri::command]
pub async fn list_mcp_servers(
    state: State<'_, AppState>,
) -> Result<Vec<McpServerInfo>, String> {
    use cowork_core::mcp_manager::McpServerManager;

    let config_manager = state.config_manager.read().await;
    let manager = McpServerManager::with_configs(config_manager.config().mcp_servers.clone());

    let servers = manager.list_servers();
    Ok(servers
        .into_iter()
        .map(|s| McpServerInfo {
            name: s.name,
            command: s.command,
            enabled: s.enabled,
            status: format!("{:?}", s.status).to_lowercase(),
            tool_count: s.tool_count,
            error: match s.status {
                cowork_core::mcp_manager::McpServerStatus::Failed(msg) => Some(msg),
                _ => None,
            },
        })
        .collect())
}

/// List all tools from MCP servers
#[tauri::command]
pub async fn list_mcp_tools(
    state: State<'_, AppState>,
) -> Result<Vec<McpToolInfo>, String> {
    use cowork_core::mcp_manager::McpServerManager;

    let config_manager = state.config_manager.read().await;
    let manager = McpServerManager::with_configs(config_manager.config().mcp_servers.clone());

    let tools = manager.get_all_tools();
    Ok(tools
        .into_iter()
        .map(|t| McpToolInfo {
            name: t.name,
            description: t.description,
            server: t.server,
        })
        .collect())
}

/// Add a new MCP server
#[tauri::command]
pub async fn add_mcp_server(
    name: String,
    command: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use cowork_core::config::McpServerConfig;

    let mut config_manager = state.config_manager.write().await;

    // Check if URL or command
    let config = if command.starts_with("http://") || command.starts_with("https://") {
        McpServerConfig::new_http(command)
    } else {
        McpServerConfig::new(command)
    };

    config_manager
        .config_mut()
        .mcp_servers
        .insert(name, config);

    config_manager
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(())
}

/// Start an MCP server
#[tauri::command]
pub async fn start_mcp_server(
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use cowork_core::mcp_manager::McpServerManager;

    let config_manager = state.config_manager.read().await;
    let manager = McpServerManager::with_configs(config_manager.config().mcp_servers.clone());

    manager
        .start_server(&name)
        .map_err(|e| format!("Failed to start server: {}", e))?;

    Ok(())
}

/// Stop an MCP server
#[tauri::command]
pub async fn stop_mcp_server(
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use cowork_core::mcp_manager::McpServerManager;

    let config_manager = state.config_manager.read().await;
    let manager = McpServerManager::with_configs(config_manager.config().mcp_servers.clone());

    manager
        .stop_server(&name)
        .map_err(|e| format!("Failed to stop server: {}", e))?;

    Ok(())
}

/// Remove an MCP server
#[tauri::command]
pub async fn remove_mcp_server(
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut config_manager = state.config_manager.write().await;

    config_manager.config_mut().mcp_servers.remove(&name);

    config_manager
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(())
}

// ============================================================================
// Skill Installation Commands
// ============================================================================

/// Installed skill information for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct InstalledSkillInfo {
    pub name: String,
    pub description: String,
    pub location: String,
    pub path: String,
}

/// List installed skills
#[tauri::command]
pub async fn list_installed_skills(
    state: State<'_, AppState>,
) -> Result<Vec<InstalledSkillInfo>, String> {
    use cowork_core::skills::installer::SkillInstaller;

    let installer = SkillInstaller::new(state.workspace_path.clone());
    let skills = installer.list_installed();

    Ok(skills
        .into_iter()
        .map(|s| InstalledSkillInfo {
            name: s.name,
            description: s.description,
            location: format!("{:?}", s.location).to_lowercase(),
            path: s.path.to_string_lossy().to_string(),
        })
        .collect())
}

/// Install a skill from URL
#[tauri::command]
pub async fn install_skill(
    url: String,
    location: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<InstalledSkillInfo, String> {
    use cowork_core::skills::installer::{InstallLocation, SkillInstaller};

    let installer = SkillInstaller::new(state.workspace_path.clone());

    let loc = match location.as_str() {
        "global" => InstallLocation::Global,
        _ => InstallLocation::Project,
    };

    let result = installer
        .install_from_url(&url, loc, force)
        .map_err(|e| format!("Failed to install skill: {}", e))?;

    Ok(InstalledSkillInfo {
        name: result.name,
        description: result.description,
        location: format!("{:?}", result.location).to_lowercase(),
        path: result.path.to_string_lossy().to_string(),
    })
}

/// Remove an installed skill
#[tauri::command]
pub async fn remove_skill(
    name: String,
    location: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use cowork_core::skills::installer::{InstallLocation, SkillInstaller};

    let installer = SkillInstaller::new(state.workspace_path.clone());

    let loc = match location.as_str() {
        "global" => Some(InstallLocation::Global),
        "project" => Some(InstallLocation::Project),
        _ => None,
    };

    installer
        .uninstall(&name, loc)
        .map_err(|e| format!("Failed to remove skill: {}", e))?;

    Ok(())
}

// ============================================================================
// Session Persistence Commands
// ============================================================================

/// Saved session info for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct SavedSessionInfo {
    pub id: String,
    pub title: Option<String>,
    pub message_count: usize,
    pub provider_type: String,
    pub created_at: String,
    pub updated_at: String,
    pub file_size: u64,
}

/// Sessions directory info
#[derive(Debug, Clone, Serialize)]
pub struct SessionsDirectoryInfo {
    pub path: String,
    pub session_count: usize,
    pub total_size: u64,
}

/// Save the current session to disk
#[tauri::command]
pub async fn save_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use crate::session_storage::{generate_title, SessionData, SessionStorage};

    let sessions = state.sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Get provider info from config
    let cm = state.config_manager.read().await;
    let (provider_type, model) = if let Some(provider) = cm.config().get_default_provider() {
        (provider.provider_type.clone(), provider.model.clone())
    } else {
        ("unknown".to_string(), "unknown".to_string())
    };

    let now = chrono::Utc::now();
    let created_at = session
        .messages
        .first()
        .map(|m| m.timestamp)
        .unwrap_or(now);

    let session_data = SessionData {
        id: session.id.clone(),
        title: generate_title(&session.messages),
        messages: session.messages.clone(),
        system_prompt: session.system_prompt.clone(),
        provider_type,
        model,
        created_at,
        updated_at: now,
    };

    let storage = SessionStorage::new();
    let path = storage.save(&session_data).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

/// List all saved sessions
#[tauri::command]
pub async fn list_saved_sessions() -> Result<Vec<SavedSessionInfo>, String> {
    use crate::session_storage::SessionStorage;

    let storage = SessionStorage::new();
    let sessions = storage.list().map_err(|e| e.to_string())?;

    Ok(sessions
        .into_iter()
        .map(|s| SavedSessionInfo {
            id: s.id,
            title: s.title,
            message_count: s.message_count,
            provider_type: s.provider_type,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
            file_size: s.file_size,
        })
        .collect())
}

/// Load a saved session and make it active
#[tauri::command]
pub async fn load_saved_session(
    saved_session_id: String,
    state: State<'_, AppState>,
) -> Result<SessionInfo, String> {
    use crate::session_storage::SessionStorage;

    let storage = SessionStorage::new();
    let session_data = storage.load(&saved_session_id).map_err(|e| e.to_string())?;

    // Create a new ChatSession from the saved data
    let cm = state.config_manager.read().await;
    let provider_config = cm
        .config()
        .get_default_provider()
        .ok_or_else(|| "No default provider configured".to_string())?;
    let provider = create_provider_from_config(provider_config)?;

    let mut session = ChatSession::new(provider);
    // Replace the generated ID with the saved one
    session.id = session_data.id.clone();
    session.messages = session_data.messages;
    session.system_prompt = session_data.system_prompt;

    let info = SessionInfo {
        id: session.id.clone(),
        message_count: session.messages.len(),
        created_at: session_data.created_at,
    };

    drop(cm);

    let mut sessions = state.sessions.write().await;
    sessions.insert(session.id.clone(), session);

    Ok(info)
}

/// Delete a saved session
#[tauri::command]
pub async fn delete_saved_session(saved_session_id: String) -> Result<(), String> {
    use crate::session_storage::SessionStorage;

    let storage = SessionStorage::new();
    storage.delete(&saved_session_id).map_err(|e| e.to_string())
}

/// Delete sessions older than specified days
#[tauri::command]
pub async fn delete_old_sessions(days: i64) -> Result<Vec<String>, String> {
    use crate::session_storage::SessionStorage;

    let storage = SessionStorage::new();
    storage.delete_older_than(days).map_err(|e| e.to_string())
}

/// Delete all saved sessions
#[tauri::command]
pub async fn delete_all_saved_sessions() -> Result<usize, String> {
    use crate::session_storage::SessionStorage;

    let storage = SessionStorage::new();
    storage.delete_all().map_err(|e| e.to_string())
}

/// Get info about the sessions directory
#[tauri::command]
pub async fn get_sessions_directory_info() -> Result<SessionsDirectoryInfo, String> {
    use crate::session_storage::SessionStorage;

    let storage = SessionStorage::new();
    let sessions = storage.list().map_err(|e| e.to_string())?;
    let total_size = storage.total_size().unwrap_or(0);

    Ok(SessionsDirectoryInfo {
        path: storage.sessions_dir().to_string_lossy().to_string(),
        session_count: sessions.len(),
        total_size,
    })
}

/// Open the sessions folder in the system file manager
#[tauri::command]
pub async fn open_sessions_folder() -> Result<(), String> {
    use crate::session_storage::SessionStorage;

    let storage = SessionStorage::new();
    storage.ensure_dir().map_err(|e| e.to_string())?;

    let path = storage.sessions_dir();

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
