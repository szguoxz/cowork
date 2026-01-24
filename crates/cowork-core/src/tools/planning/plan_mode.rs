//! ExitPlanMode tool - Exit plan mode and request user approval
//!
//! Used when in plan mode to signal completion of planning and request approval.


use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

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
    workspace: PathBuf,
}

impl ExitPlanMode {
    pub fn new(state: Arc<RwLock<PlanModeState>>, workspace: PathBuf) -> Self {
        Self { state, workspace }
    }

    pub fn new_standalone() -> Self {
        Self {
            state: Arc::new(RwLock::new(PlanModeState::default())),
            workspace: PathBuf::from("."),
        }
    }
}


impl Tool for ExitPlanMode {
    fn name(&self) -> &str {
        "ExitPlanMode"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::EXIT_PLAN_MODE
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

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
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

            // Read the plan file if one was set
            let plan_contents = if let Some(ref plan_file) = state.plan_file {
                let plan_path = self.workspace.join(plan_file);
                match std::fs::read_to_string(&plan_path) {
                    Ok(contents) => Some(contents),
                    Err(_) => None,
                }
            } else {
                None
            };

            // Store allowed prompts and deactivate
            state.allowed_prompts = allowed_prompts.clone();
            state.active = false;

            let mut result = json!({
                "status": "plan_complete",
                "message": "Plan mode exited. Waiting for user approval.",
                "requested_permissions": allowed_prompts.iter().map(|p| {
                    json!({
                        "tool": p.tool,
                        "prompt": p.prompt
                    })
                }).collect::<Vec<_>>()
            });

            // Include plan contents if available
            if let Some(contents) = plan_contents {
                result["plan_file"] = json!(state.plan_file);
                result["plan_contents"] = json!(contents);
            }

            Ok(ToolOutput::success(result))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
