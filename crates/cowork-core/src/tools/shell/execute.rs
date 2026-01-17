//! Execute command tool

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

use super::ShellConfig;

/// Tool for executing shell commands
pub struct ExecuteCommand {
    config: ShellConfig,
    workspace: PathBuf,
}

impl ExecuteCommand {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            config: ShellConfig::default(),
            workspace,
        }
    }

    pub fn with_config(mut self, config: ShellConfig) -> Self {
        self.config = config;
        self
    }

    fn is_command_blocked(&self, command: &str) -> bool {
        for blocked in &self.config.blocked_commands {
            if command.contains(blocked) {
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl Tool for ExecuteCommand {
    fn name(&self) -> &str {
        "execute_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Commands are run in a sandboxed environment."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the command (relative to workspace)"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)",
                    "default": 30
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("command is required".into()))?;

        let timeout = params["timeout"].as_u64().unwrap_or(self.config.timeout_seconds);

        // Security check
        if self.is_command_blocked(command) {
            return Err(ToolError::PermissionDenied(format!(
                "Command contains blocked pattern: {}",
                command
            )));
        }

        let working_dir = if let Some(dir) = params["working_dir"].as_str() {
            self.workspace.join(dir)
        } else {
            self.config
                .working_dir
                .clone()
                .unwrap_or_else(|| self.workspace.clone())
        };

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| ToolError::ExecutionFailed(format!("Command timed out after {}s", timeout)))?
        .map_err(ToolError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolOutput::success(json!({
            "exit_code": output.status.code(),
            "stdout": stdout,
            "stderr": stderr,
            "success": output.status.success()
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Medium
    }
}
