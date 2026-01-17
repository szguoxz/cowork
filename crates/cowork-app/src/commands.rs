//! Tauri commands exposed to the frontend

use serde::{Deserialize, Serialize};
use tauri::State;

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
    state: State<'_, AppState>,
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
pub async fn get_task_status(task_id: String) -> Result<TaskState, String> {
    // In a real implementation, look up task from storage
    Err(format!("Task {} not found", task_id))
}

/// Cancel a running task
#[tauri::command]
pub async fn cancel_task(task_id: String) -> Result<(), String> {
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
pub async fn get_settings() -> Result<Settings, String> {
    // In a real implementation, load from config file
    Ok(Settings::default())
}

/// Update application settings
#[tauri::command]
pub async fn update_settings(settings: Settings) -> Result<(), String> {
    // In a real implementation, save to config file
    Ok(())
}
