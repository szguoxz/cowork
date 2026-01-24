//! KillShell tool - Terminate background shell processes
//!
//! Allows killing running background shell commands by their ID.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::RwLock;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// Registry for tracking background shell processes
pub struct ShellProcessRegistry {
    processes: Arc<RwLock<HashMap<String, BackgroundShell>>>,
}

/// A background shell process
pub struct BackgroundShell {
    pub id: String,
    pub command: String,
    pub child: Option<Child>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub status: ShellStatus,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShellStatus {
    Running,
    Completed,
    Failed,
    Killed,
}

impl Default for ShellProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellProcessRegistry {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, shell: BackgroundShell) {
        let mut processes = self.processes.write().await;
        processes.insert(shell.id.clone(), shell);
    }

    pub async fn get(&self, id: &str) -> Option<ShellStatus> {
        let processes = self.processes.read().await;
        processes.get(id).map(|s| s.status.clone())
    }

    pub async fn kill(&self, id: &str) -> Result<(), String> {
        let mut processes = self.processes.write().await;
        if let Some(shell) = processes.get_mut(id) {
            if shell.status == ShellStatus::Running {
                if let Some(ref mut child) = shell.child {
                    child
                        .kill()
                        .await
                        .map_err(|e| format!("Failed to kill process: {}", e))?;
                }
                shell.status = ShellStatus::Killed;
                Ok(())
            } else {
                Err(format!("Shell {} is not running (status: {:?})", id, shell.status))
            }
        } else {
            Err(format!("Shell {} not found", id))
        }
    }

    pub async fn list_running(&self) -> Vec<(String, String)> {
        let processes = self.processes.read().await;
        processes
            .iter()
            .filter(|(_, s)| s.status == ShellStatus::Running)
            .map(|(id, s)| (id.clone(), s.command.clone()))
            .collect()
    }
}

/// Tool for killing background shell processes
pub struct KillShell {
    registry: Arc<ShellProcessRegistry>,
}

impl KillShell {
    pub fn new(registry: Arc<ShellProcessRegistry>) -> Self {
        Self { registry }
    }
}

impl Tool for KillShell {
    fn name(&self) -> &str {
        "KillShell"
    }

    fn description(&self) -> &str {
        "Kills a running background bash shell by its ID.\n\n\
         - Takes a shell_id parameter identifying the shell to kill\n\
         - Returns a success or failure status\n\
         - Use this tool when you need to terminate a long-running shell\n\
         - Shell IDs can be found using the /tasks command"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "shell_id": {
                    "type": "string",
                    "description": "The ID of the background shell to kill"
                }
            },
            "required": ["shell_id"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let shell_id = params["shell_id"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("shell_id is required".into()))?;

            match self.registry.kill(shell_id).await {
                Ok(()) => Ok(ToolOutput::success(json!({
                    "success": true,
                    "shell_id": shell_id,
                    "message": format!("Shell {} has been terminated", shell_id)
                }))),
                Err(e) => Err(ToolError::ExecutionFailed(e)),
            }
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
