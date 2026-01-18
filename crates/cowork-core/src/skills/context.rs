//! Context management skills
//!
//! Provides /compact, /clear, and /context commands for managing conversation context.

use std::path::PathBuf;

use crate::context::{CompactConfig, ContextGatherer};

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};

/// /compact command - Summarize conversation with optional preservation instructions
pub struct CompactSkill {
    workspace: PathBuf,
}

impl CompactSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for CompactSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "compact".to_string(),
            display_name: "Compact Context".to_string(),
            description: "Summarize the conversation to reduce context usage. Optionally specify what to preserve.".to_string(),
            usage: "/compact [preserve instructions]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let preserve_instructions = if ctx.args.trim().is_empty() {
                None
            } else {
                Some(ctx.args.trim().to_string())
            };

            // Create compact config
            let config = CompactConfig::from_command(preserve_instructions.clone());

            let response = if let Some(ref instructions) = preserve_instructions {
                format!(
                    "Compacting conversation context.\n\
                     Preservation focus: {}\n\n\
                     The conversation will be summarized while prioritizing the specified content.\n\
                     Use /context to see the updated usage after compaction.",
                    instructions
                )
            } else {
                "Compacting conversation context.\n\n\
                 The conversation will be summarized to reduce token usage.\n\
                 Use /context to see the updated usage after compaction."
                    .to_string()
            };

            SkillResult::success(response).with_data(serde_json::json!({
                "action": "compact",
                "preserve_instructions": preserve_instructions,
                "config": {
                    "use_llm": config.use_llm,
                    "target_ratio": config.target_ratio,
                    "min_keep_recent": config.min_keep_recent,
                }
            }))
        })
    }

    fn prompt_template(&self) -> &str {
        r#"You are a conversation summarizer. Your task is to:
1. Analyze the conversation history
2. Create a concise summary that preserves:
   - Key decisions made
   - Files created or modified
   - Important context for continuing work
   - Any specific content the user asked to preserve
3. The summary should be clear and actionable"#
    }
}

/// /clear command - Reset conversation while keeping memory files
pub struct ClearSkill {
    workspace: PathBuf,
}

impl ClearSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for ClearSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "clear".to_string(),
            display_name: "Clear Conversation".to_string(),
            description: "Clear the conversation history. Memory files (CLAUDE.md) are preserved.".to_string(),
            usage: "/clear".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, _ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            SkillResult::success(
                "Conversation cleared.\n\n\
                 Project context from memory files (CLAUDE.md) has been preserved.\n\
                 You can start a fresh conversation while keeping project instructions.",
            )
            .with_data(serde_json::json!({
                "action": "clear",
            }))
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

/// /context command - Show context usage statistics
pub struct ContextSkill {
    workspace: PathBuf,
}

impl ContextSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for ContextSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "context".to_string(),
            display_name: "Context Usage".to_string(),
            description: "Display current context usage statistics and memory hierarchy.".to_string(),
            usage: "/context".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, _ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        let workspace = self.workspace.clone();
        Box::pin(async move {
            // Gather memory hierarchy
            let gatherer = ContextGatherer::new(&workspace);
            let hierarchy = gatherer.gather_memory_hierarchy().await;

            let mut response = String::new();

            // Memory hierarchy section
            response.push_str("=== Memory Hierarchy ===\n\n");
            if hierarchy.is_empty() {
                response.push_str("No memory files found.\n");
                response.push_str("\nTip: Create a CLAUDE.md file in your project root to provide project-specific instructions.\n");
            } else {
                response.push_str(&hierarchy.summary());
            }

            // Context usage section (placeholder - actual usage comes from the session)
            response.push_str("\n=== Context Usage ===\n\n");
            response.push_str("Context usage statistics are displayed in the session status bar.\n");
            response.push_str("Use /compact to reduce context usage when it gets high.\n");

            // Tips section
            response.push_str("\n=== Tips ===\n\n");
            response.push_str("- Use /compact [instructions] to summarize with preservation focus\n");
            response.push_str("- Use /clear to reset the conversation\n");
            response.push_str("- Memory files are loaded in priority order:\n");
            response.push_str("  1. Enterprise: /etc/claude-code/CLAUDE.md\n");
            response.push_str("  2. Project: ./CLAUDE.md, ./.claude/CLAUDE.md\n");
            response.push_str("  3. Rules: ./.claude/rules/*.md\n");
            response.push_str("  4. User: ~/.claude/CLAUDE.md, ./CLAUDE.local.md\n");

            SkillResult::success(response).with_data(serde_json::json!({
                "action": "context",
                "memory_hierarchy": {
                    "file_count": hierarchy.file_count(),
                    "total_size": hierarchy.total_size,
                }
            }))
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_compact_skill_no_args() {
        let skill = CompactSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Compacting"));
    }

    #[tokio::test]
    async fn test_compact_skill_with_args() {
        let skill = CompactSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: "keep API changes".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("API changes"));
    }

    #[tokio::test]
    async fn test_clear_skill() {
        let skill = ClearSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("cleared"));
    }

    #[tokio::test]
    async fn test_context_skill() {
        let skill = ContextSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Memory Hierarchy"));
    }
}
