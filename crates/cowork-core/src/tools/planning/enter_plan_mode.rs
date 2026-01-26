//! EnterPlanMode tool - Transition to planning mode
//!
//! Used proactively by the LLM to enter plan mode for non-trivial implementation tasks.

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
        include_str!("../../prompt/builtin/claude_code/tools/enterplanmode.md")
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": {}
        })
    }

    fn execute(&self, _params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let mut state = self.state.write().await;

            if state.active {
                return Ok(ToolOutput::success(json!({
                    "status": "already_active",
                    "message": "Already in plan mode"
                })));
            }

            state.active = true;

            Ok(ToolOutput::success(json!({
                "status": "plan_mode_activated",
                "message": "Plan mode activated. You can now explore the codebase and design your implementation approach. Use ExitPlanMode when ready to present your plan for user approval."
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        // Requires user approval to enter plan mode
        ApprovalLevel::Low
    }
}
