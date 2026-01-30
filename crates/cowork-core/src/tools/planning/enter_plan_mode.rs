//! EnterPlanMode tool - Transition to planning mode
//!
//! Used proactively by the LLM to enter plan mode for non-trivial implementation tasks.

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

use super::plan_mode::{get_plans_dir, PlanModeState};

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
        include_str!("../../prompt/builtin/claude_code/tools/enterplanmode.md")
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": {}
        })
    }

    fn execute(&self, _params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let mut state = self.state.write().await;

            if state.active {
                let plan_file = state.plan_file.as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                return Ok(ToolOutput::success(json!({
                    "status": "already_active",
                    "message": "Already in plan mode",
                    "plan_file": plan_file
                })));
            }

            // Create the plans directory if it doesn't exist
            let plans_dir = get_plans_dir();
            if let Err(e) = std::fs::create_dir_all(&plans_dir) {
                return Err(ToolError::ExecutionFailed(format!(
                    "Failed to create plans directory: {}", e
                )));
            }

            // Generate a new plan file path
            let plan_file = state.generate_plan_file();
            state.active = true;

            Ok(ToolOutput::success(json!({
                "status": "plan_mode_activated",
                "message": "Plan mode activated. Write your plan to the plan file, then use ExitPlanMode when ready for user approval.",
                "plan_file": plan_file.to_string_lossy()
            })))
        })
    }
}
