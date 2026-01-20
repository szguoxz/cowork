//! Shell Agent - specialized for command execution

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use crate::context::Context;
use crate::error::Result;
use crate::task::{StepResult, TaskStep, TaskType};
use crate::tools::shell::ExecuteCommand;
use crate::tools::Tool;

use super::Agent;

/// Agent specialized for shell command execution
pub struct ShellAgent {
    id: String,
    tools: Vec<Arc<dyn Tool>>,
}

impl ShellAgent {
    pub fn new(workspace: PathBuf) -> Self {
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(ExecuteCommand::new(workspace))];

        Self {
            id: "shell_agent".to_string(),
            tools,
        }
    }
}

#[async_trait]
impl Agent for ShellAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "Shell Agent"
    }

    fn description(&self) -> &str {
        "Specialized agent for executing shell commands in a sandboxed environment."
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.clone()
    }

    async fn execute(&self, step: &TaskStep, _ctx: &mut Context) -> Result<StepResult> {
        let tool = self
            .tools
            .first()
            .ok_or_else(|| crate::error::Error::Agent("No tools available".to_string()))?;

        let output = tool
            .execute(step.parameters.clone())
            .await
            .map_err(crate::error::Error::Tool)?;

        Ok(StepResult {
            step_id: step.id.clone(),
            output,
            next_steps: Vec::new(),
        })
    }

    fn can_handle(&self, task_type: &TaskType) -> bool {
        matches!(task_type, TaskType::ShellCommand | TaskType::Build | TaskType::Test)
    }

    fn system_prompt(&self) -> &str {
        r#"You are a Shell Agent specialized in command-line operations.

Your capabilities include:
- Executing shell commands
- Running build systems (make, cargo, npm, etc.)
- Running tests
- Git operations
- System utilities

Always consider security implications before running commands.
Avoid destructive operations without explicit user approval.
Break complex operations into smaller, verifiable steps.
Capture and report both stdout and stderr appropriately."#
    }
}
