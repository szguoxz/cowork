//! EnterPlanMode tool - Transition to planning mode
//!
//! Used when starting a complex task to enter planning mode for user approval.

use rand::Rng;
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
        crate::prompt::builtin::claude_code::tools::ENTER_PLAN_MODE
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn execute(&self, _params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let mut state = self.state.write().await;

            // Check if already in plan mode
            if state.active {
                return Ok(ToolOutput::success(json!({
                    "status": "already_in_plan_mode",
                    "message": "Already in plan mode. Use ExitPlanMode when ready for approval.",
                    "plan_file": state.plan_file
                })));
            }

            // Generate plan file path: ~/.claude/plans/<random-name>.md
            let plan_file = generate_plan_file_path();

            // Ensure the directory exists
            if let Some(parent) = std::path::Path::new(&plan_file).parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            // Enter plan mode
            state.active = true;
            state.plan_file = Some(plan_file.clone());
            state.allowed_prompts.clear();

            Ok(ToolOutput::success(json!({
                "status": "entered_plan_mode",
                "message": format!(
                    "You are now in plan mode. Write your plan to {} and use ExitPlanMode when ready for approval.",
                    plan_file
                ),
                "plan_file": plan_file
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        // Entering plan mode requires user consent
        ApprovalLevel::Low
    }
}

/// Generate a plan file path in ~/.claude/plans/ with a random adjective-noun name
fn generate_plan_file_path() -> String {
    const ADJECTIVES: &[&str] = &[
        "keen", "bold", "calm", "dark", "fair", "glad", "warm", "wise",
        "bright", "clean", "crisp", "eager", "fresh", "quick", "sharp",
        "swift", "vivid", "gentle", "steady", "silent",
    ];
    const NOUNS: &[&str] = &[
        "beam", "dawn", "dusk", "fern", "glow", "lake", "leaf", "moon",
        "pine", "rain", "star", "tide", "wind", "cloud", "flame", "frost",
        "grove", "ridge", "stone", "brook",
    ];
    const MODIFIERS: &[&str] = &[
        "amber", "azure", "coral", "ivory", "jade", "pearl", "ruby",
        "silver", "golden", "copper", "crystal", "cobalt", "scarlet",
        "violet", "indigo", "crimson", "emerald", "obsidian", "sapphire",
        "stirring",
    ];

    let mut rng = rand::rng();
    let adj = ADJECTIVES[rng.random_range(0..ADJECTIVES.len())];
    let modifier = MODIFIERS[rng.random_range(0..MODIFIERS.len())];
    let noun = NOUNS[rng.random_range(0..NOUNS.len())];

    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let plans_dir = home.join(".claude").join("plans");
    plans_dir
        .join(format!("{}-{}-{}.md", adj, modifier, noun))
        .to_string_lossy()
        .to_string()
}
