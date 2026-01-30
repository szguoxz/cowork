//! TodoWrite tool - task tracking for AI workflows


use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

/// Status of a todo item
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// A todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: TodoStatus,
    #[serde(rename = "activeForm")]
    pub active_form: String,
}

/// Shared todo list state
pub type TodoList = Arc<RwLock<Vec<TodoItem>>>;

/// Tool for managing a todo list during task execution
pub struct TodoWrite {
    todos: TodoList,
}

impl TodoWrite {
    pub fn new() -> Self {
        Self {
            todos: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_shared_list(todos: TodoList) -> Self {
        Self { todos }
    }

    pub fn get_list(&self) -> TodoList {
        self.todos.clone()
    }
}

impl Default for TodoWrite {
    fn default() -> Self {
        Self::new()
    }
}


impl Tool for TodoWrite {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::TODOWRITE
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "minLength": 1,
                                "description": "The imperative form describing what needs to be done (e.g., 'Run tests', 'Build the project')"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status of the task"
                            },
                            "activeForm": {
                                "type": "string",
                                "minLength": 1,
                                "description": "Present continuous form shown during execution (e.g., 'Running tests', 'Building the project')"
                            }
                        },
                        "required": ["content", "status", "activeForm"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let todos_value = params["todos"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidParams("todos array is required".into()))?;

        let mut new_todos: Vec<TodoItem> = Vec::new();

        for item in todos_value {
            let content = item["content"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("todo.content is required".into()))?
                .to_string();

            let status_str = item["status"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("todo.status is required".into()))?;

            let status = match status_str {
                "pending" => TodoStatus::Pending,
                "in_progress" => TodoStatus::InProgress,
                "completed" => TodoStatus::Completed,
                _ => {
                    return Err(ToolError::InvalidParams(format!(
                        "Invalid status: {}. Must be pending, in_progress, or completed",
                        status_str
                    )))
                }
            };

            let active_form = item["activeForm"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("todo.activeForm is required".into()))?
                .to_string();

            new_todos.push(TodoItem {
                content,
                status,
                active_form,
            });
        }

        // Validate: only one task should be in_progress at a time
        let in_progress_count = new_todos
            .iter()
            .filter(|t| t.status == TodoStatus::InProgress)
            .count();

        if in_progress_count > 1 {
            return Err(ToolError::InvalidParams(
                "Only one task should be in_progress at a time".into(),
            ));
        }

        // Update the shared list
        {
            let mut list = self.todos.write().await;
            *list = new_todos.clone();
        }

        // Calculate summary
        let total = new_todos.len();
        let completed = new_todos
            .iter()
            .filter(|t| t.status == TodoStatus::Completed)
            .count();
        let in_progress = new_todos
            .iter()
            .filter(|t| t.status == TodoStatus::InProgress)
            .count();
        let pending = new_todos
            .iter()
            .filter(|t| t.status == TodoStatus::Pending)
            .count();

        Ok(ToolOutput::success(json!({
            "success": true,
            "todos": new_todos,
            "summary": {
                "total": total,
                "completed": completed,
                "in_progress": in_progress,
                "pending": pending
            }
        })))
            })
    }
}
