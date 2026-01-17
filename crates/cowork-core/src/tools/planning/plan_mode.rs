//! ExitPlanMode tool - Exit plan mode and request user approval
//!
//! Used when in plan mode to signal completion of planning and request approval.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

/// Allowed prompt for bash commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedPrompt {
    pub tool: String,
    pub prompt: String,
}

/// Plan mode state
#[derive(Debug, Clone, Default)]
pub struct PlanModeState {
    pub active: bool,
    pub plan_file: Option<String>,
    pub allowed_prompts: Vec<AllowedPrompt>,
}

/// Tool for exiting plan mode
pub struct ExitPlanMode {
    state: Arc<RwLock<PlanModeState>>,
}

impl ExitPlanMode {
    pub fn new(state: Arc<RwLock<PlanModeState>>) -> Self {
        Self { state }
    }

    pub fn new_standalone() -> Self {
        Self {
            state: Arc::new(RwLock::new(PlanModeState::default())),
        }
    }
}

#[async_trait]
impl Tool for ExitPlanMode {
    fn name(&self) -> &str {
        "exit_plan_mode"
    }

    fn description(&self) -> &str {
        "Use this tool when in plan mode and have finished writing your plan. \
         This signals readiness for user approval. You can request prompt-based permissions \
         for bash commands your plan will need."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "allowedPrompts": {
                    "type": "array",
                    "description": "Prompt-based permissions needed to implement the plan",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": {
                                "type": "string",
                                "description": "The tool this prompt applies to",
                                "enum": ["Bash"]
                            },
                            "prompt": {
                                "type": "string",
                                "description": "Semantic description of the action, e.g. 'run tests', 'install dependencies'"
                            }
                        },
                        "required": ["tool", "prompt"]
                    }
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let mut state = self.state.write().await;

        // Parse allowed prompts
        let allowed_prompts: Vec<AllowedPrompt> = params
            .get("allowedPrompts")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Validate prompts
        for prompt in &allowed_prompts {
            if prompt.tool != "Bash" {
                return Err(ToolError::InvalidParams(format!(
                    "Only 'Bash' tool is supported for allowed prompts, got '{}'",
                    prompt.tool
                )));
            }
            if prompt.prompt.is_empty() {
                return Err(ToolError::InvalidParams(
                    "Prompt description cannot be empty".into(),
                ));
            }
        }

        // Store allowed prompts
        state.allowed_prompts = allowed_prompts.clone();
        state.active = false;

        Ok(ToolOutput::success(json!({
            "status": "plan_complete",
            "message": "Plan mode exited. Waiting for user approval.",
            "requested_permissions": allowed_prompts.iter().map(|p| {
                json!({
                    "tool": p.tool,
                    "prompt": p.prompt
                })
            }).collect::<Vec<_>>()
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
