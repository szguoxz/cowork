//! Task/Agent tool - Launch subagents for complex tasks
//!
//! This tool allows spawning specialized subagents that can work autonomously
//! on complex, multi-step tasks.


use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// Agent types available for task execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum AgentType {
    /// Command execution specialist for bash commands
    Bash,
    /// General-purpose agent for research and multi-step tasks
    GeneralPurpose,
    /// Fast agent for exploring codebases
    Explore,
    /// Software architect for designing implementation plans
    Plan,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Bash => write!(f, "Bash"),
            AgentType::GeneralPurpose => write!(f, "general-purpose"),
            AgentType::Explore => write!(f, "Explore"),
            AgentType::Plan => write!(f, "Plan"),
        }
    }
}

impl std::str::FromStr for AgentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bash" => Ok(AgentType::Bash),
            "general-purpose" | "generalpurpose" | "general" => Ok(AgentType::GeneralPurpose),
            "explore" | "explorer" => Ok(AgentType::Explore),
            "plan" | "planner" => Ok(AgentType::Plan),
            _ => Err(format!("Unknown agent type: {}", s)),
        }
    }
}

/// Model selection for subagents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentModel {
    Sonnet,
    Opus,
    Haiku,
}

impl Default for AgentModel {
    fn default() -> Self {
        AgentModel::Sonnet
    }
}

/// Running agent instance
#[derive(Debug, Clone)]
pub struct AgentInstance {
    pub id: String,
    pub agent_type: AgentType,
    pub description: String,
    pub prompt: String,
    pub model: AgentModel,
    pub status: AgentStatus,
    pub output: Option<String>,
    pub output_file: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Registry for managing running agents
pub struct AgentInstanceRegistry {
    agents: Arc<RwLock<HashMap<String, AgentInstance>>>,
}

impl Default for AgentInstanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentInstanceRegistry {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, agent: AgentInstance) {
        let mut agents = self.agents.write().await;
        agents.insert(agent.id.clone(), agent);
    }

    pub async fn get(&self, id: &str) -> Option<AgentInstance> {
        let agents = self.agents.read().await;
        agents.get(id).cloned()
    }

    pub async fn update_status(&self, id: &str, status: AgentStatus, output: Option<String>) {
        let mut agents = self.agents.write().await;
        if let Some(agent) = agents.get_mut(id) {
            agent.status = status;
            if let Some(out) = output {
                agent.output = Some(out);
            }
        }
    }

    pub async fn list_running(&self) -> Vec<AgentInstance> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| a.status == AgentStatus::Running)
            .cloned()
            .collect()
    }
}

/// Tool for launching subagents
pub struct TaskTool {
    registry: Arc<AgentInstanceRegistry>,
}

impl TaskTool {
    pub fn new(registry: Arc<AgentInstanceRegistry>) -> Self {
        Self { registry }
    }
}


impl Tool for TaskTool {
    fn name(&self) -> &str {
        "task"
    }

    fn description(&self) -> &str {
        "Launch a new agent to handle complex, multi-step tasks autonomously. \
         Available agent types: Bash (command execution), general-purpose (research and multi-step tasks), \
         Explore (fast codebase exploration), Plan (software architecture and planning). \
         Use this tool when you need specialized help with complex tasks."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The detailed task for the agent to perform"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent: Bash, general-purpose, Explore, or Plan",
                    "enum": ["Bash", "general-purpose", "Explore", "Plan"]
                },
                "model": {
                    "type": "string",
                    "description": "Model to use: sonnet (default), opus (most capable), haiku (fast/cheap)",
                    "enum": ["sonnet", "opus", "haiku"]
                },
                "resume": {
                    "type": "string",
                    "description": "Agent ID to resume from a previous execution"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Run agent in background. Returns output_file path to check progress.",
                    "default": false
                },
                "max_turns": {
                    "type": "integer",
                    "description": "Maximum number of agentic turns before stopping",
                    "default": 50
                }
            },
            "required": ["description", "prompt", "subagent_type"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let description = params["description"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("description is required".into()))?;

        let prompt = params["prompt"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("prompt is required".into()))?;

        let agent_type_str = params["subagent_type"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("subagent_type is required".into()))?;

        let agent_type: AgentType = agent_type_str
            .parse()
            .map_err(|e: String| ToolError::InvalidParams(e))?;

        let model = match params["model"].as_str() {
            Some("opus") => AgentModel::Opus,
            Some("haiku") => AgentModel::Haiku,
            _ => AgentModel::Sonnet,
        };

        let run_in_background = params["run_in_background"].as_bool().unwrap_or(false);
        let _max_turns = params["max_turns"].as_u64().unwrap_or(50);

        // Check for resume
        if let Some(resume_id) = params["resume"].as_str() {
            if let Some(agent) = self.registry.get(resume_id).await {
                return Ok(ToolOutput::success(json!({
                    "agent_id": agent.id,
                    "status": agent.status,
                    "output": agent.output,
                    "resumed": true
                })));
            } else {
                return Err(ToolError::InvalidParams(format!(
                    "Agent {} not found for resume",
                    resume_id
                )));
            }
        }

        // Create new agent instance
        let agent_id = uuid::Uuid::new_v4().to_string();
        let output_file = if run_in_background {
            Some(format!("/tmp/cowork-agent-{}.log", agent_id))
        } else {
            None
        };

        let agent = AgentInstance {
            id: agent_id.clone(),
            agent_type: agent_type.clone(),
            description: description.to_string(),
            prompt: prompt.to_string(),
            model,
            status: AgentStatus::Running,
            output: None,
            output_file: output_file.clone(),
            created_at: chrono::Utc::now(),
        };

        self.registry.register(agent).await;

        // For now, return immediately with agent info
        // In a full implementation, this would spawn the agent task
        if run_in_background {
            Ok(ToolOutput::success(json!({
                "agent_id": agent_id,
                "status": "running",
                "output_file": output_file,
                "message": format!("Agent '{}' started in background. Use TaskOutput to check progress.", description)
            })))
        } else {
            // Simulate agent execution (in real implementation, this would run the agent)
            let result = execute_agent_task(&agent_type, prompt).await;

            self.registry
                .update_status(&agent_id, AgentStatus::Completed, Some(result.clone()))
                .await;

            Ok(ToolOutput::success(json!({
                "agent_id": agent_id,
                "status": "completed",
                "result": result
            })))
        }
            })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}

/// Execute an agent task (simplified implementation)
async fn execute_agent_task(agent_type: &AgentType, prompt: &str) -> String {
    match agent_type {
        AgentType::Bash => {
            format!(
                "Bash agent would execute commands for: {}",
                prompt
            )
        }
        AgentType::GeneralPurpose => {
            format!(
                "General-purpose agent researched and found: {}",
                prompt
            )
        }
        AgentType::Explore => {
            format!(
                "Explore agent analyzed codebase for: {}",
                prompt
            )
        }
        AgentType::Plan => {
            format!(
                "Plan agent created implementation plan for: {}",
                prompt
            )
        }
    }
}

/// Tool for getting output from background tasks
pub struct TaskOutputTool {
    registry: Arc<AgentInstanceRegistry>,
}

impl TaskOutputTool {
    pub fn new(registry: Arc<AgentInstanceRegistry>) -> Self {
        Self { registry }
    }
}


impl Tool for TaskOutputTool {
    fn name(&self) -> &str {
        "task_output"
    }

    fn description(&self) -> &str {
        "Retrieves output from a running or completed task (background agent or shell). \
         Use block=true to wait for completion, block=false for non-blocking status check."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task/agent ID to get output from"
                },
                "block": {
                    "type": "boolean",
                    "description": "Whether to wait for completion",
                    "default": true
                },
                "timeout": {
                    "type": "integer",
                    "description": "Max wait time in milliseconds",
                    "default": 30000
                }
            },
            "required": ["task_id"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("task_id is required".into()))?;

        let block = params["block"].as_bool().unwrap_or(true);
        let timeout_ms = params["timeout"].as_u64().unwrap_or(30000);

        if let Some(agent) = self.registry.get(task_id).await {
            if block && agent.status == AgentStatus::Running {
                // Wait for completion with timeout
                let start = std::time::Instant::now();
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                    if let Some(updated) = self.registry.get(task_id).await {
                        if updated.status != AgentStatus::Running {
                            return Ok(ToolOutput::success(json!({
                                "task_id": task_id,
                                "status": updated.status,
                                "output": updated.output
                            })));
                        }
                    }

                    if start.elapsed().as_millis() as u64 > timeout_ms {
                        return Ok(ToolOutput::success(json!({
                            "task_id": task_id,
                            "status": "running",
                            "message": "Timeout waiting for task completion"
                        })));
                    }
                }
            } else {
                Ok(ToolOutput::success(json!({
                    "task_id": task_id,
                    "status": agent.status,
                    "output": agent.output
                })))
            }
        } else {
            // Check if it's a file-based output
            if let Some(output_file) = params["output_file"].as_str() {
                match tokio::fs::read_to_string(output_file).await {
                    Ok(content) => Ok(ToolOutput::success(json!({
                        "task_id": task_id,
                        "output": content
                    }))),
                    Err(e) => Err(ToolError::ExecutionFailed(format!(
                        "Failed to read output file: {}",
                        e
                    ))),
                }
            } else {
                Err(ToolError::ResourceNotFound(format!(
                    "Task {} not found",
                    task_id
                )))
            }
        }
            })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
