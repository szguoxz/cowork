//! Task planner - breaks down tasks into steps

use crate::error::Result;

use super::{Task, TaskStep, TaskType};

/// Plans how to execute tasks
pub struct TaskPlanner {
    /// Maximum steps per task
    max_steps: usize,
}

impl TaskPlanner {
    pub fn new() -> Self {
        Self { max_steps: 100 }
    }

    pub fn with_max_steps(mut self, max: usize) -> Self {
        self.max_steps = max;
        self
    }

    /// Create a plan for executing a task
    pub fn plan(&self, task: &Task) -> Result<Vec<TaskStep>> {
        // In a full implementation, this would use an LLM to analyze
        // the task and create appropriate steps

        let steps = self.create_default_steps(task);
        Ok(steps)
    }

    /// Create default steps based on task type
    fn create_default_steps(&self, task: &Task) -> Vec<TaskStep> {
        match &task.task_type {
            TaskType::FileOperation => vec![TaskStep::new(
                &task.description,
                "Read",
                task.context.clone(),
            )],

            TaskType::ShellCommand => vec![TaskStep::new(
                &task.description,
                "Bash",
                task.context.clone(),
            )],

            TaskType::WebAutomation => vec![
                TaskStep::new("Navigate to URL", "browser_navigate", task.context.clone()),
                TaskStep::new(
                    "Take screenshot",
                    "browser_screenshot",
                    serde_json::json!({}),
                ),
            ],

            TaskType::DocumentProcessing => vec![TaskStep::new(
                &task.description,
                "read_pdf",
                task.context.clone(),
            )],

            TaskType::Search => vec![TaskStep::new(
                &task.description,
                "search_files",
                task.context.clone(),
            )],

            TaskType::Build => vec![TaskStep::new(
                "Run build command",
                "Bash",
                serde_json::json!({ "command": "make" }),
            )],

            TaskType::Test => vec![TaskStep::new(
                "Run tests",
                "Bash",
                serde_json::json!({ "command": "make test" }),
            )],

            TaskType::Screenshot => vec![TaskStep::new(
                &task.description,
                "browser_screenshot",
                task.context.clone(),
            )],

            TaskType::Custom(_) => vec![TaskStep::new(
                &task.description,
                "Bash",
                task.context.clone(),
            )],
        }
    }

    /// Validate a plan
    pub fn validate(&self, steps: &[TaskStep]) -> Result<()> {
        if steps.len() > self.max_steps {
            return Err(crate::error::Error::Task(format!(
                "Plan exceeds maximum steps ({})",
                self.max_steps
            )));
        }

        // Check for dependency cycles
        // (simplified - in reality would need proper cycle detection)
        for step in steps {
            if step.dependencies.contains(&step.id) {
                return Err(crate::error::Error::Task(format!(
                    "Step {} has circular dependency",
                    step.id
                )));
            }
        }

        Ok(())
    }
}

impl Default for TaskPlanner {
    fn default() -> Self {
        Self::new()
    }
}
