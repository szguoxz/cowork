//! Execute command tool

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::process_utils::{shell_command, shell_command_background};
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::{BackgroundShell, ShellConfig, ShellProcessRegistry, ShellStatus};

/// Tool for executing shell commands
pub struct ExecuteCommand {
    config: ShellConfig,
    workspace: PathBuf,
    process_registry: Option<Arc<ShellProcessRegistry>>,
}

impl ExecuteCommand {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            config: ShellConfig::default(),
            workspace,
            process_registry: None,
        }
    }

    pub fn with_config(mut self, config: ShellConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_registry(mut self, registry: Arc<ShellProcessRegistry>) -> Self {
        self.process_registry = Some(registry);
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

impl Tool for ExecuteCommand {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::BASH
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does in active voice. For simple commands keep it brief (5-10 words). For complex commands add enough context to clarify what it does."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Optional timeout in milliseconds (max 600000)",
                    "default": 120000
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this command in the background. Use TaskOutput to read the output later.",
                    "default": false
                },
                "dangerouslyDisableSandbox": {
                    "type": "boolean",
                    "description": "Set this to true to dangerously override sandbox mode and run commands without sandboxing.",
                    "default": false
                }
            },
            "required": ["command"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let command = params["command"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("command is required".into()))?;

            let timeout_ms = params["timeout"].as_u64().unwrap_or(120000);
            let timeout_secs = (timeout_ms / 1000).min(600);
            let run_in_background = params["run_in_background"].as_bool().unwrap_or(false);
            let _description = params["description"].as_str().unwrap_or("");

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

            // Handle background execution
            if run_in_background {
                if let Some(registry) = &self.process_registry {
                    let shell_id = uuid::Uuid::new_v4().to_string();
                    let output_file = std::env::temp_dir()
                        .join(format!("cowork-shell-{}.log", shell_id))
                        .to_string_lossy()
                        .to_string();

                    // Spawn the command in background with output redirection
                    // Uses process_utils which handles hiding console windows on Windows
                    let child = shell_command_background(command, &output_file)
                        .current_dir(&working_dir)
                        .spawn()
                        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to spawn: {}", e)))?;

                    let bg_shell = BackgroundShell {
                        id: shell_id.clone(),
                        command: command.to_string(),
                        child: Some(child),
                        started_at: chrono::Utc::now(),
                        status: ShellStatus::Running,
                        output: None,
                    };

                    registry.register(bg_shell).await;

                    return Ok(ToolOutput::success(json!({
                        "shell_id": shell_id,
                        "status": "running",
                        "output_file": output_file,
                        "message": "Command started in background. Use TaskOutput to check results."
                    })));
                } else {
                    return Err(ToolError::ExecutionFailed(
                        "Background execution not available: no process registry".into(),
                    ));
                }
            }

            // Foreground execution with timeout
            // Uses process_utils which handles hiding console windows on Windows
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                shell_command(command)
                    .current_dir(&working_dir)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output(),
            )
            .await
            .map_err(|_| {
                ToolError::ExecutionFailed(format!("Command timed out after {}s", timeout_secs))
            })?
            .map_err(ToolError::Io)?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            Ok(ToolOutput::success(json!({
                "exit_code": output.status.code(),
                "stdout": stdout,
                "stderr": stderr,
                "success": output.status.success()
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Medium
    }
}
