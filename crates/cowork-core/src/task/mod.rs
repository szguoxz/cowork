//! Task management system
//!
//! Tasks represent work to be done by agents. The task system handles:
//! - Task creation and lifecycle
//! - Planning and step decomposition
//! - Execution coordination
//! - Result aggregation

mod executor;
mod planner;

pub use executor::TaskExecutor;
pub use planner::TaskPlanner;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::ToolOutput;

/// Unique task identifier
pub type TaskId = String;

/// Types of tasks the system can handle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskType {
    FileOperation,
    ShellCommand,
    WebAutomation,
    DocumentProcessing,
    Search,
    Build,
    Test,
    Screenshot,
    Custom(String),
}

/// Current status of a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Planning,
    WaitingApproval,
    InProgress,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// A task to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub description: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub steps: Vec<TaskStep>,
    pub context: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Task {
    pub fn new(description: impl Into<String>, task_type: TaskType) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            task_type,
            status: TaskStatus::Pending,
            steps: Vec::new(),
            context: Value::Null,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_context(mut self, context: Value) -> Self {
        self.context = context;
        self
    }
}

/// A single step within a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub id: String,
    pub description: String,
    pub tool_name: String,
    pub parameters: Value,
    pub dependencies: Vec<String>,
}

impl TaskStep {
    pub fn new(
        description: impl Into<String>,
        tool_name: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            tool_name: tool_name.into(),
            parameters,
            dependencies: Vec::new(),
        }
    }

    pub fn with_dependency(mut self, step_id: impl Into<String>) -> Self {
        self.dependencies.push(step_id.into());
        self
    }
}

/// Result of executing a task step
#[derive(Debug, Clone)]
pub struct StepResult {
    pub step_id: String,
    pub output: ToolOutput,
    pub next_steps: Vec<TaskStep>,
}

/// Summary of task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub steps_completed: usize,
    pub steps_total: usize,
    pub duration_ms: u64,
    pub errors: Vec<String>,
}
