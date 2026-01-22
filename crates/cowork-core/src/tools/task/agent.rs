//! Task/Agent tool - Launch subagents for complex tasks
//!
//! This tool allows spawning specialized subagents that can work autonomously
//! on complex, multi-step tasks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::provider::ProviderType;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::executor::{self, AgentExecutionConfig};

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

/// Model tier selection for subagents (provider-agnostic)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    /// Fast model for quick tasks (e.g., Haiku, gpt-4o-mini)
    Fast,
    /// Balanced model for general tasks (e.g., Sonnet, gpt-4o)
    #[default]
    Balanced,
    /// Powerful model for complex reasoning (e.g., Opus, o1)
    Powerful,
}

impl std::str::FromStr for ModelTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // Provider-agnostic names
            "fast" => Ok(ModelTier::Fast),
            "balanced" => Ok(ModelTier::Balanced),
            "powerful" => Ok(ModelTier::Powerful),
            // Legacy Anthropic-style aliases for backwards compatibility
            "haiku" => Ok(ModelTier::Fast),
            "sonnet" => Ok(ModelTier::Balanced),
            "opus" => Ok(ModelTier::Powerful),
            _ => Err(format!("Unknown model tier: {}", s)),
        }
    }
}

impl AgentType {
    /// Get the recommended default model tier for this agent type
    pub fn default_tier(&self) -> ModelTier {
        match self {
            // Explore is read-only, fast operations - use fast model
            AgentType::Explore => ModelTier::Fast,
            // Bash is simple command execution - use fast model
            AgentType::Bash => ModelTier::Fast,
            // Plan needs reasoning for architecture - use balanced
            AgentType::Plan => ModelTier::Balanced,
            // General purpose needs full capabilities - use balanced
            AgentType::GeneralPurpose => ModelTier::Balanced,
        }
    }
}

// Type alias for backwards compatibility
pub type AgentModel = ModelTier;

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
    workspace: PathBuf,
    provider_type: ProviderType,
    api_key: Option<String>,
    model_tiers: Option<crate::config::ModelTiers>,
}

impl TaskTool {
    /// Create a new TaskTool with the given registry and workspace
    pub fn new(registry: Arc<AgentInstanceRegistry>, workspace: PathBuf) -> Self {
        Self {
            registry,
            workspace,
            provider_type: ProviderType::Anthropic,
            api_key: None,
            model_tiers: None,
        }
    }

    /// Set the provider type for subagent execution
    pub fn with_provider(mut self, provider_type: ProviderType) -> Self {
        self.provider_type = provider_type;
        self
    }

    /// Set the API key for subagent execution
    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Set custom model tiers for subagent execution
    pub fn with_model_tiers(mut self, model_tiers: crate::config::ModelTiers) -> Self {
        self.model_tiers = Some(model_tiers);
        self
    }
}


impl Tool for TaskTool {
    fn name(&self) -> &str {
        "Task"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::TASK
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
                    "description": "Model tier: fast (quick tasks), balanced (default), powerful (complex reasoning). Also accepts: haiku, sonnet, opus as aliases.",
                    "enum": ["fast", "balanced", "powerful", "haiku", "sonnet", "opus"]
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

        // Parse model tier, falling back to agent type's recommended default
        let model = params["model"]
            .as_str()
            .and_then(|s| s.parse::<ModelTier>().ok())
            .unwrap_or_else(|| agent_type.default_tier());

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
            Some(
                std::env::temp_dir()
                    .join(format!("cowork-agent-{}.log", agent_id))
                    .to_string_lossy()
                    .to_string(),
            )
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

        // Create execution config
        let mut config = AgentExecutionConfig::new(self.workspace.clone())
            .with_provider(self.provider_type)
            .with_max_turns(_max_turns);

        if let Some(ref key) = self.api_key {
            config = config.with_api_key(key.clone());
        }

        // Use custom model tiers if provided, otherwise executor uses provider defaults
        if let Some(ref tiers) = self.model_tiers {
            config = config.with_model_tiers(tiers.clone());
        }

        if run_in_background {
            // Start agent in background
            executor::execute_agent_background(
                agent_type,
                model,
                prompt.to_string(),
                config,
                self.registry.clone(),
                agent_id.clone(),
                output_file.clone().unwrap_or_default(),
            );

            Ok(ToolOutput::success(json!({
                "agent_id": agent_id,
                "status": "running",
                "output_file": output_file,
                "message": format!("Agent '{}' started in background. Use TaskOutput to check progress.", description)
            })))
        } else {
            // Execute agent synchronously
            let result = executor::execute_agent_loop(
                &agent_type,
                &model,
                prompt,
                &config,
                self.registry.clone(),
                &agent_id,
            )
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Agent execution failed: {}", e)))?;

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
        "TaskOutput"
    }

    fn description(&self) -> &str {
        "Retrieves output from a running or completed task (background shell, agent, or remote session).\n\n\
         - Takes a task_id parameter identifying the task\n\
         - Returns the task output along with status information\n\
         - Use block=true (default) to wait for task completion\n\
         - Use block=false for non-blocking check of current status\n\
         - Task IDs can be found using the /tasks command\n\
         - Works with all task types: background shells, async agents, and remote sessions"
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

                    if let Some(updated) = self.registry.get(task_id).await
                        && updated.status != AgentStatus::Running {
                            return Ok(ToolOutput::success(json!({
                                "task_id": task_id,
                                "status": updated.status,
                                "output": updated.output
                            })));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_task_tool_metadata() {
        let registry = Arc::new(AgentInstanceRegistry::new());
        let workspace = PathBuf::from("/tmp/test-workspace");
        let tool = TaskTool::new(registry.clone(), workspace);

        // Verify tool metadata (PascalCase name)
        assert_eq!(tool.name(), "Task");
        assert!(tool.description().contains("agent"));
    }

    #[tokio::test]
    async fn test_task_tool_background() {
        let registry = Arc::new(AgentInstanceRegistry::new());
        let workspace = PathBuf::from("/tmp/test-workspace");
        let tool = TaskTool::new(registry.clone(), workspace);

        let params = json!({
            "description": "Background task",
            "prompt": "Run a long task",
            "subagent_type": "Bash",
            "run_in_background": true
        });

        let result = tool.execute(params).await.unwrap();
        assert_eq!(result.content["status"].as_str(), Some("running"));
        assert!(result.content["output_file"].as_str().is_some());
        assert!(result.content["agent_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_task_tool_resume() {
        let registry = Arc::new(AgentInstanceRegistry::new());
        let workspace = PathBuf::from("/tmp/test-workspace");
        let tool = TaskTool::new(registry.clone(), workspace);

        // Register a completed agent
        let agent = AgentInstance {
            id: "resume-test-123".to_string(),
            agent_type: AgentType::Explore,
            description: "Test agent".to_string(),
            prompt: "Do something".to_string(),
            model: ModelTier::Balanced,
            status: AgentStatus::Completed,
            output: Some("Completed successfully".to_string()),
            output_file: None,
            created_at: chrono::Utc::now(),
        };
        registry.register(agent).await;

        // Try to resume
        let params = json!({
            "description": "Resume task",
            "prompt": "Continue",
            "subagent_type": "Explore",
            "resume": "resume-test-123"
        });

        let result = tool.execute(params).await.unwrap();
        assert_eq!(result.content["resumed"].as_bool(), Some(true));
        assert_eq!(result.content["agent_id"].as_str(), Some("resume-test-123"));
    }

    #[tokio::test]
    async fn test_task_output_tool() {
        let registry = Arc::new(AgentInstanceRegistry::new());
        let output_tool = TaskOutputTool::new(registry.clone());

        // First register an agent directly
        let agent = AgentInstance {
            id: "output-test-123".to_string(),
            agent_type: AgentType::Explore,
            description: "Test agent".to_string(),
            prompt: "Do something".to_string(),
            model: ModelTier::Balanced,
            status: AgentStatus::Completed,
            output: Some("Test output result".to_string()),
            output_file: None,
            created_at: chrono::Utc::now(),
        };
        registry.register(agent).await;

        // Now get the output
        let output_params = json!({
            "task_id": "output-test-123",
            "block": false
        });

        let output_result = output_tool.execute(output_params).await.unwrap();
        assert_eq!(output_result.content["status"].as_str(), Some("completed"));
        assert_eq!(
            output_result.content["output"].as_str(),
            Some("Test output result")
        );
    }

    #[tokio::test]
    async fn test_agent_type_parsing() {
        assert_eq!("bash".parse::<AgentType>().unwrap(), AgentType::Bash);
        assert_eq!("Bash".parse::<AgentType>().unwrap(), AgentType::Bash);
        assert_eq!("explore".parse::<AgentType>().unwrap(), AgentType::Explore);
        assert_eq!(
            "general-purpose".parse::<AgentType>().unwrap(),
            AgentType::GeneralPurpose
        );
        assert_eq!("Plan".parse::<AgentType>().unwrap(), AgentType::Plan);
        assert!("unknown".parse::<AgentType>().is_err());
    }

    #[tokio::test]
    async fn test_agent_registry() {
        let registry = AgentInstanceRegistry::new();

        let agent = AgentInstance {
            id: "test-123".to_string(),
            agent_type: AgentType::Explore,
            description: "Test agent".to_string(),
            prompt: "Do something".to_string(),
            model: ModelTier::Balanced,
            status: AgentStatus::Running,
            output: None,
            output_file: None,
            created_at: chrono::Utc::now(),
        };

        registry.register(agent).await;

        // Check we can get it
        let retrieved = registry.get("test-123").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.as_ref().unwrap().description, "Test agent");

        // Update status
        registry
            .update_status("test-123", AgentStatus::Completed, Some("Done!".to_string()))
            .await;
        let updated = registry.get("test-123").await.unwrap();
        assert_eq!(updated.status, AgentStatus::Completed);
        assert_eq!(updated.output, Some("Done!".to_string()));

        // Check running list is empty now
        let running = registry.list_running().await;
        assert!(running.is_empty());
    }
}
