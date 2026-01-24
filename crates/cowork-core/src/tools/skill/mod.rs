//! Skill tool - allows the LLM to invoke registered skills/slash commands
//!
//! When invoked, the skill's prompt template is resolved (command substitution
//! and argument substitution) and returned with metadata signaling the agent
//! loop to inject it as a user message (matching Claude Code's behavior).

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::prompt::builtin::claude_code::tools::SKILL as SKILL_DESCRIPTION;
use crate::prompt::substitution::substitute_commands;
use crate::skills::SkillRegistry;
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// Metadata key signaling the agent loop to inject content as a user message
pub const INJECT_AS_MESSAGE: &str = "inject_as_message";
/// Metadata key for the skill name
pub const SKILL_NAME_KEY: &str = "skill_name";

/// Tool that allows the LLM to execute skills from the skill registry
pub struct SkillTool {
    skill_registry: Arc<SkillRegistry>,
    workspace: PathBuf,
}

impl SkillTool {
    pub fn new(skill_registry: Arc<SkillRegistry>, workspace: PathBuf) -> Self {
        Self {
            skill_registry,
            workspace,
        }
    }
}

impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        SKILL_DESCRIPTION
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name to invoke (e.g., \"commit\", \"review\", \"test\")"
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "required": ["skill"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let skill_name = params
                .get("skill")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams("Missing required 'skill' parameter".into()))?;

            let args = params
                .get("args")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let skill = self.skill_registry.get(skill_name)
                .ok_or_else(|| ToolError::ExecutionFailed(
                    format!("Unknown skill: '{}'. Use /help to see available commands.", skill_name)
                ))?;

            // Get the prompt template and apply substitutions
            let template = skill.prompt_template();

            // Apply $ARGUMENTS substitution
            let prompt = template
                .replace("$ARGUMENTS", args)
                .replace("${ARGUMENTS}", args);

            // Apply command substitution (!`command`)
            let workspace_str = self.workspace.to_string_lossy().to_string();
            let resolved = substitute_commands(&prompt, None, Some(&workspace_str));

            // Return with metadata signaling message injection
            let mut output = ToolOutput::success(Value::String(resolved));
            output.metadata.insert(
                INJECT_AS_MESSAGE.to_string(),
                Value::Bool(true),
            );
            output.metadata.insert(
                SKILL_NAME_KEY.to_string(),
                Value::String(skill_name.to_string()),
            );

            Ok(output)
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
