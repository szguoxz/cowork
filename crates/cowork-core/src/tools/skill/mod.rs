//! Skill tool - allows the LLM to invoke registered skills/slash commands

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::error::ToolError;
use crate::prompt::builtin::claude_code::tools::SKILL as SKILL_DESCRIPTION;
use crate::skills::{SkillContext, SkillRegistry};
use crate::tools::{BoxFuture, Tool, ToolOutput};

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
                .unwrap_or("")
                .to_string();

            let ctx = SkillContext {
                workspace: self.workspace.clone(),
                args,
                data: HashMap::new(),
            };

            let result = self.skill_registry.execute(skill_name, ctx).await;

            if result.success {
                Ok(ToolOutput::success(Value::String(result.response)))
            } else {
                let error_msg = result.error.unwrap_or_else(|| "Skill execution failed".into());
                Ok(ToolOutput::error(error_msg))
            }
        })
    }
}
