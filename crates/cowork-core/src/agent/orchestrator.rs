//! Orchestrator - coordinates multiple agents to complete tasks

use async_trait::async_trait;
use std::sync::Arc;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::task::{StepResult, Task, TaskStatus, TaskStep, TaskType};
use crate::tools::Tool;

use super::{Agent, AgentRegistry};

/// The Orchestrator coordinates multiple agents to complete complex tasks
pub struct Orchestrator {
    registry: AgentRegistry,
}

impl Orchestrator {
    pub fn new(registry: AgentRegistry) -> Self {
        Self { registry }
    }

    /// Plan how to execute a task
    pub fn plan(&self, task: &Task) -> Result<Vec<TaskStep>> {
        // For now, create a single step per task
        // In a full implementation, this would use LLM to break down the task
        let step = TaskStep {
            id: format!("{}-step-1", task.id),
            description: task.description.clone(),
            tool_name: self.select_tool_for_task(&task.task_type)?,
            parameters: serde_json::json!({}),
            dependencies: Vec::new(),
        };

        Ok(vec![step])
    }

    /// Select the appropriate tool for a task type
    fn select_tool_for_task(&self, task_type: &TaskType) -> Result<String> {
        match task_type {
            TaskType::FileOperation => Ok("Read".to_string()),
            TaskType::ShellCommand => Ok("Bash".to_string()),
            TaskType::WebAutomation => Ok("browser_navigate".to_string()),
            TaskType::DocumentProcessing => Ok("read_pdf".to_string()),
            TaskType::Search => Ok("search_files".to_string()),
            TaskType::Build => Ok("Bash".to_string()),
            TaskType::Test => Ok("Bash".to_string()),
            TaskType::Screenshot => Ok("browser_screenshot".to_string()),
            TaskType::Custom(name) => Ok(name.clone()),
        }
    }

    /// Execute a complete task
    pub async fn execute_task(&self, task: &mut Task, ctx: &mut Context) -> Result<()> {
        task.status = TaskStatus::InProgress;

        // Plan the task
        let steps = self.plan(task)?;

        // Execute each step
        for step in &steps {
            let result = self.execute_step(step, ctx).await?;

            // Check if step succeeded
            if !result.output.success {
                task.status = TaskStatus::Failed;
                return Err(Error::Task(format!(
                    "Step {} failed: {:?}",
                    step.id, result.output.error
                )));
            }
        }

        task.status = TaskStatus::Completed;
        Ok(())
    }

    /// Execute a single task step
    async fn execute_step(&self, step: &TaskStep, ctx: &mut Context) -> Result<StepResult> {
        // Find an agent that can handle this step
        let agents = self.registry.find_capable(&self.infer_task_type(step));

        let agent = agents
            .first()
            .ok_or_else(|| Error::Agent("No capable agent found".to_string()))?;

        agent.execute(step, ctx).await
    }

    /// Infer task type from step
    fn infer_task_type(&self, step: &TaskStep) -> TaskType {
        match step.tool_name.as_str() {
            "Read" | "Write" | "list_directory" | "delete_file" | "move_file" => {
                TaskType::FileOperation
            }
            "search_files" => TaskType::Search,
            "Bash" => TaskType::ShellCommand,
            "browser_navigate" | "browser_click" => TaskType::WebAutomation,
            "browser_screenshot" => TaskType::Screenshot,
            "read_pdf" | "read_office_doc" => TaskType::DocumentProcessing,
            _ => TaskType::Custom(step.tool_name.clone()),
        }
    }
}

#[async_trait]
impl Agent for Orchestrator {
    fn id(&self) -> &str {
        "orchestrator"
    }

    fn name(&self) -> &str {
        "Orchestrator"
    }

    fn description(&self) -> &str {
        "Coordinates multiple specialized agents to complete complex, multi-step tasks."
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        // Orchestrator has access to all tools from all agents
        self.registry
            .list()
            .iter()
            .flat_map(|info| {
                self.registry
                    .get(&info.id)
                    .map(|a| a.tools())
                    .unwrap_or_default()
            })
            .collect()
    }

    async fn execute(&self, step: &TaskStep, ctx: &mut Context) -> Result<StepResult> {
        self.execute_step(step, ctx).await
    }

    fn can_handle(&self, _task_type: &TaskType) -> bool {
        true // Orchestrator can handle any task type
    }

    fn system_prompt(&self) -> &str {
        r#"You are the Orchestrator, responsible for coordinating multiple specialized agents.

Your role is to:
1. Analyze complex tasks and break them into steps
2. Delegate steps to the most appropriate agent
3. Coordinate execution and handle dependencies
4. Aggregate results and report back

Available agents:
- File Agent: filesystem operations
- Shell Agent: command execution
- Browser Agent: web automation
- Document Agent: document processing

Plan tasks carefully, considering dependencies between steps.
Request user approval for high-risk operations.
Provide clear progress updates throughout execution."#
    }
}
