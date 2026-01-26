//! ExitPlanMode tool - Exit plan mode and request user approval
//!
//! Used when in plan mode to signal completion of planning and request approval.

use rand::prelude::IndexedRandom;
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

/// Generate a random plan file name like "keen-stirring-sunbeam"
pub fn generate_plan_name() -> String {
    const ADJECTIVES: &[&str] = &[
        "keen", "swift", "bright", "calm", "bold", "clear", "warm", "cool",
        "fresh", "wild", "soft", "pure", "deep", "wise", "fair", "kind",
    ];
    const VERBS: &[&str] = &[
        "stirring", "flowing", "dancing", "glowing", "rising", "shining",
        "drifting", "soaring", "blazing", "sparking", "singing", "humming",
    ];
    const NOUNS: &[&str] = &[
        "sunbeam", "river", "breeze", "storm", "flame", "wave", "cloud",
        "mountain", "forest", "meadow", "ocean", "canyon", "valley", "dawn",
    ];

    let mut rng = rand::rng();
    let adj = ADJECTIVES.choose(&mut rng).unwrap_or(&"bright");
    let verb = VERBS.choose(&mut rng).unwrap_or(&"flowing");
    let noun = NOUNS.choose(&mut rng).unwrap_or(&"stream");

    format!("{}-{}-{}", adj, verb, noun)
}

/// Get the plans directory path
pub fn get_plans_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("plans")
}

/// Plan mode state
#[derive(Debug, Clone, Default)]
pub struct PlanModeState {
    pub active: bool,
    /// Full path to the plan file (e.g., ~/.claude/plans/keen-stirring-sunbeam.md)
    pub plan_file: Option<PathBuf>,
    pub allowed_prompts: Vec<AllowedPrompt>,
}

impl PlanModeState {
    /// Generate a new plan file path and set it
    pub fn generate_plan_file(&mut self) -> PathBuf {
        let plans_dir = get_plans_dir();
        let name = generate_plan_name();
        let plan_file = plans_dir.join(format!("{}.md", name));
        self.plan_file = Some(plan_file.clone());
        plan_file
    }
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

            // Read the plan file if one was set (plan_file is already a full path)
            let plan_contents = if let Some(ref plan_file) = state.plan_file {
                match std::fs::read_to_string(plan_file) {
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
                if let Some(ref plan_file) = state.plan_file {
                    result["plan_file"] = json!(plan_file.to_string_lossy());
                }
                result["plan_contents"] = json!(contents);
            }

            Ok(ToolOutput::success(result))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
