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
        "EnterPlanMode"
    }

    fn description(&self) -> &str {
        "Use this tool proactively when you're about to start a non-trivial implementation task.\n\n\
         Getting user sign-off on your approach before writing code prevents wasted effort and ensures alignment.\n\n\
         Prefer using EnterPlanMode for implementation tasks unless they're simple. Use when:\n\
         - New Feature Implementation: Adding meaningful new functionality\n\
         - Multiple Valid Approaches: The task can be solved several different ways\n\
         - Code Modifications: Changes that affect existing behavior or structure\n\
         - Architectural Decisions: Task requires choosing between patterns or technologies\n\
         - Multi-File Changes: Task will likely touch more than 2-3 files\n\
         - Unclear Requirements: Need to explore before understanding the full scope\n\n\
         Skip for: single-line fixes, typos, obvious bugs, small tweaks"
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
