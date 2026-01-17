//! Agent system for Cowork
//!
//! Agents are specialized AI assistants that can use tools to accomplish tasks.
//! Each agent has a specific focus (files, shell, browser, etc.) and a set of
//! tools it can use.

pub mod browser_agent;
pub mod document_agent;
pub mod file_agent;
pub mod orchestrator;
pub mod shell_agent;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::context::Context;
use crate::error::Result;
use crate::task::{StepResult, TaskStep, TaskType};
use crate::tools::Tool;

/// Core trait for all agents
#[async_trait]
pub trait Agent: Send + Sync {
    /// Unique identifier for this agent
    fn id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Description of agent's capabilities
    fn description(&self) -> &str;

    /// Tools this agent can use
    fn tools(&self) -> Vec<Arc<dyn Tool>>;

    /// Execute a task step
    async fn execute(&self, step: &TaskStep, ctx: &mut Context) -> Result<StepResult>;

    /// Check if this agent can handle a given task type
    fn can_handle(&self, task_type: &TaskType) -> bool;

    /// Get the system prompt for this agent
    fn system_prompt(&self) -> &str;
}

/// Registry of available agents
pub struct AgentRegistry {
    agents: HashMap<String, Arc<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Register an agent
    pub fn register(&mut self, agent: Arc<dyn Agent>) {
        self.agents.insert(agent.id().to_string(), agent);
    }

    /// Get an agent by ID
    pub fn get(&self, id: &str) -> Option<Arc<dyn Agent>> {
        self.agents.get(id).cloned()
    }

    /// Find agents that can handle a task type
    pub fn find_capable(&self, task_type: &TaskType) -> Vec<Arc<dyn Agent>> {
        self.agents
            .values()
            .filter(|a| a.can_handle(task_type))
            .cloned()
            .collect()
    }

    /// List all registered agents
    pub fn list(&self) -> Vec<AgentInfo> {
        self.agents
            .values()
            .map(|a| AgentInfo {
                id: a.id().to_string(),
                name: a.name().to_string(),
                description: a.description().to_string(),
                tool_count: a.tools().len(),
            })
            .collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent information for display
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tool_count: usize,
}
