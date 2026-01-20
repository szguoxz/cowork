//! EnterPlanMode tool - Transition to planning mode
//!
//! Used when starting a complex task to enter planning mode for user approval.

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::plan_mode::PlanModeState;

/// Tool for entering plan mode
pub struct EnterPlanMode {
    state: Arc<RwLock<PlanModeState>>,
}

impl EnterPlanMode {
    pub fn new(state: Arc<RwLock<PlanModeState>>) -> Self {
        Self { state }
    }

    pub fn new_standalone() -> Self {
        Self {
            state: Arc::new(RwLock::new(PlanModeState::default())),
        }
    }
}

impl Tool for EnterPlanMode {
    fn name(&self) -> &str {
        "enter_plan_mode"
    }

    fn description(&self) -> &str {
        "Use this tool proactively when starting a non-trivial implementation task. \
         Getting user sign-off on your approach before writing code prevents wasted effort. \
         In plan mode, you can explore the codebase and design an implementation approach \
         for user approval. Use this when: adding new features, code modifications affect \
         existing behavior, multiple valid approaches exist, or task requires architectural decisions."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_description": {
                    "type": "string",
                    "description": "Brief description of the task being planned"
                },
                "plan_file": {
                    "type": "string",
                    "description": "Path to write the plan file (optional, defaults to PLAN.md)"
                }
            }
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let mut state = self.state.write().await;

            // Check if already in plan mode
            if state.active {
                return Ok(ToolOutput::success(json!({
                    "status": "already_in_plan_mode",
                    "message": "Already in plan mode. Use exit_plan_mode when ready for approval.",
                    "plan_file": state.plan_file
                })));
            }

            let task_description = params
                .get("task_description")
                .and_then(|v| v.as_str())
                .unwrap_or("Implementation task");

            let plan_file = params
                .get("plan_file")
                .and_then(|v| v.as_str())
                .unwrap_or("PLAN.md")
                .to_string();

            // Enter plan mode
            state.active = true;
            state.plan_file = Some(plan_file.clone());
            state.allowed_prompts.clear();

            Ok(ToolOutput::success(json!({
                "status": "entered_plan_mode",
                "message": format!(
                    "Entered plan mode for: {}. \
                     You can now explore the codebase and design an implementation. \
                     Write your plan to {} and use exit_plan_mode when ready for approval.",
                    task_description, plan_file
                ),
                "plan_file": plan_file,
                "guidelines": [
                    "Explore the codebase using read-only tools (read_file, glob, grep)",
                    "Understand existing patterns and architecture",
                    "Design an implementation approach",
                    "Write your plan to the plan file",
                    "Use exit_plan_mode with any needed permissions when ready"
                ]
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        // Entering plan mode requires user consent
        ApprovalLevel::Low
    }
}
