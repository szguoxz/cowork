//! Skill tool - allows the LLM to invoke registered skills/slash commands
//!
//! When invoked, the skill's prompt template is resolved (command substitution
//! and argument substitution) and returned with metadata signaling the agent
//! loop to inject it as a user message (matching Claude Code's behavior).
//!
//! Skills with `context: fork` run in a subagent instead of inline.

use std::path::PathBuf;
use std::sync::Arc;

use regex::Regex;
use serde_json::Value;

use crate::error::ToolError;
use crate::prompt::builtin::claude_code::tools::SKILL as SKILL_DESCRIPTION;
use crate::prompt::substitution::substitute_commands;
use crate::skills::SkillRegistry;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

/// Metadata key signaling the agent loop to inject content as a user message
pub const INJECT_AS_MESSAGE: &str = "inject_as_message";
/// Metadata key for the skill name
pub const SKILL_NAME_KEY: &str = "skill_name";
/// Metadata key signaling the agent loop to spawn a subagent
pub const SPAWN_SUBAGENT: &str = "spawn_subagent";
/// Metadata key for the subagent type
pub const SUBAGENT_TYPE: &str = "subagent_type";
/// Metadata key for model override
pub const MODEL_OVERRIDE: &str = "model_override";
/// Metadata key for allowed tools
pub const ALLOWED_TOOLS: &str = "allowed_tools";

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

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
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

            // Apply argument substitutions (positional and full)
            let prompt = substitute_arguments(template, args);

            // Apply command substitution (!`command`)
            let workspace_str = self.workspace.to_string_lossy().to_string();
            let resolved = substitute_commands(&prompt, None, Some(&workspace_str));

            // Build output with appropriate metadata
            let mut output = ToolOutput::success(Value::String(resolved.clone()));
            output.metadata.insert(
                SKILL_NAME_KEY.to_string(),
                Value::String(skill_name.to_string()),
            );

            // Check if skill should run in a subagent
            if skill.runs_in_subagent() {
                // Signal agent loop to spawn a subagent
                output.metadata.insert(
                    SPAWN_SUBAGENT.to_string(),
                    Value::Bool(true),
                );

                // Set subagent type (defaults to "general-purpose")
                let agent_type = skill.subagent_type().unwrap_or("general-purpose");
                output.metadata.insert(
                    SUBAGENT_TYPE.to_string(),
                    Value::String(agent_type.to_string()),
                );
            } else {
                // Run inline - inject as user message
                output.metadata.insert(
                    INJECT_AS_MESSAGE.to_string(),
                    Value::Bool(true),
                );
            }

            // Add model override if specified
            if let Some(model) = skill.model_override() {
                output.metadata.insert(
                    MODEL_OVERRIDE.to_string(),
                    Value::String(model.to_string()),
                );
            }

            // Add allowed tools if specified
            if let Some(tools) = skill.allowed_tools() {
                output.metadata.insert(
                    ALLOWED_TOOLS.to_string(),
                    Value::Array(tools.into_iter().map(|t| Value::String(t.to_string())).collect()),
                );
            }

            Ok(output)
        })
    }
}

/// Substitute argument placeholders in skill templates
///
/// Supports:
/// - `$ARGUMENTS` / `${ARGUMENTS}` - all arguments
/// - `$ARGUMENTS[N]` / `${ARGUMENTS[N]}` - positional argument by index
/// - `$N` - shorthand for positional argument (e.g., `$0`, `$1`)
pub fn substitute_arguments(template: &str, args: &str) -> String {
    // Split arguments by whitespace for positional access
    let arg_parts: Vec<&str> = args.split_whitespace().collect();

    let mut result = template.to_string();

    // Replace $ARGUMENTS[N] and ${ARGUMENTS[N]} with positional args
    let indexed_re = Regex::new(r"\$\{?ARGUMENTS\[(\d+)\]\}?").unwrap();
    result = indexed_re
        .replace_all(&result, |caps: &regex::Captures| {
            let index: usize = caps[1].parse().unwrap_or(0);
            arg_parts.get(index).unwrap_or(&"").to_string()
        })
        .to_string();

    // Replace $N shorthand (must do after $ARGUMENTS to avoid conflicts)
    // Match $0, $1, etc. but not $ARGUMENTS
    let shorthand_re = Regex::new(r"\$(\d+)").unwrap();
    result = shorthand_re
        .replace_all(&result, |caps: &regex::Captures| {
            let index: usize = caps[1].parse().unwrap_or(0);
            arg_parts.get(index).unwrap_or(&"").to_string()
        })
        .to_string();

    // Replace full $ARGUMENTS / ${ARGUMENTS}
    result = result.replace("$ARGUMENTS", args);
    result = result.replace("${ARGUMENTS}", args);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_arguments_full() {
        let template = "Hello $ARGUMENTS!";
        assert_eq!(substitute_arguments(template, "world"), "Hello world!");

        let template = "Hello ${ARGUMENTS}!";
        assert_eq!(substitute_arguments(template, "world"), "Hello world!");
    }

    #[test]
    fn test_substitute_arguments_positional() {
        let template = "Move $0 to $1";
        assert_eq!(substitute_arguments(template, "foo bar"), "Move foo to bar");

        let template = "Move $ARGUMENTS[0] to $ARGUMENTS[1]";
        assert_eq!(substitute_arguments(template, "foo bar"), "Move foo to bar");

        let template = "Move ${ARGUMENTS[0]} to ${ARGUMENTS[1]}";
        assert_eq!(substitute_arguments(template, "foo bar"), "Move foo to bar");
    }

    #[test]
    fn test_substitute_arguments_mixed() {
        let template = "Run $0 with args: $ARGUMENTS";
        assert_eq!(
            substitute_arguments(template, "build --release"),
            "Run build with args: build --release"
        );
    }

    #[test]
    fn test_substitute_arguments_missing() {
        let template = "Value: $2";
        assert_eq!(substitute_arguments(template, "only one"), "Value: ");
    }
}
