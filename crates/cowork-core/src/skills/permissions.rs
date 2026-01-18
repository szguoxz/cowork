//! Permissions management skill
//!
//! Provides /permissions command for viewing and understanding tool permissions:
//! - /permissions - Show current approval settings
//! - /permissions tools - List tools by approval level
//! - /permissions levels - Explain approval levels

use std::path::PathBuf;

use crate::config::ConfigManager;

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};

/// Permissions skill - view tool permissions and approval settings
pub struct PermissionsSkill {
    _workspace: PathBuf,
}

impl PermissionsSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { _workspace: workspace }
    }

    /// Parse subcommand from args
    fn parse_subcommand(args: &str) -> (&str, Vec<&str>) {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.is_empty() {
            ("show", vec![])
        } else {
            (parts[0], parts[1..].to_vec())
        }
    }

    /// Show current approval settings
    fn cmd_show(&self) -> SkillResult {
        let config = match ConfigManager::new() {
            Ok(cm) => cm,
            Err(e) => return SkillResult::error(format!("Failed to load config: {}", e)),
        };

        let approval_level = &config.config().approval.auto_approve_level;

        let mut output = String::from("=== Permission Settings ===\n\n");
        output.push_str(&format!("Auto-approve level: {}\n\n", approval_level));

        // Explain what this means
        let explanation = match approval_level.as_str() {
            "none" => "All operations require explicit approval",
            "low" => "Read operations are auto-approved; writes need approval",
            "medium" => "Read and simple write operations are auto-approved",
            "high" => "Most operations auto-approved; only critical ones need approval",
            "critical" | "all" => "All operations are auto-approved (be careful!)",
            _ => "Custom approval level",
        };
        output.push_str(&format!("Meaning: {}\n\n", explanation));

        output.push_str("To change, edit your config file:\n");
        output.push_str("  approval:\n");
        output.push_str("    auto_approve_level: \"medium\"  # none, low, medium, high, all\n\n");

        output.push_str("Tip: Use `/permissions tools` to see tool approval levels.\n");
        output.push_str("     Use `/permissions levels` to understand each level.\n");

        SkillResult::success(output)
    }

    /// List tools by approval level
    fn cmd_tools(&self) -> SkillResult {
        // Static list of tools and their approval levels
        // This is informational - actual levels are defined in each tool
        let output = r#"=== Tools by Approval Level ===

None (Read-only, always allowed):
  - Read          Read file contents
  - Glob          Find files by pattern
  - Grep          Search file contents
  - ListDirectory List directory contents
  - LSP           Code intelligence queries

Low (Create/modify with low risk):
  - Write         Create/overwrite files
  - Edit          Modify existing files
  - NotebookEdit  Edit Jupyter notebooks
  - TodoWrite     Update task list

Medium (External actions):
  - Bash          Execute shell commands
  - WebFetch      Fetch web content
  - WebSearch     Search the web
  - BrowserNav    Navigate in browser
  - TaskOutput    Get background task output

High (Destructive or sensitive):
  - Delete        Remove files
  - MoveFile      Move/rename files
  - KillShell     Terminate processes
  - BrowserInteract Click/type in browser

Critical (Always requires approval):
  - (None currently - reserved for future use)

Note: When approval level is set to "medium", operations at Medium
and above will prompt for approval. Operations below Medium are
auto-approved.

Session Approvals:
  - Press 'Y' to approve once
  - Press 'A' to approve for session (same operation)
  - Press 'N' to deny
"#;

        SkillResult::success(output)
    }

    /// Explain approval levels
    fn cmd_levels(&self) -> SkillResult {
        let output = r#"=== Approval Levels ===

Approval levels control which operations need explicit user confirmation.

Levels (from safest to most permissive):

NONE
  - Every operation requires approval
  - Maximum safety, but interrupts workflow
  - Use when: Learning the system or untrusted code

LOW
  - Read operations are auto-approved
  - Writes and modifications need approval
  - Use when: You want to review all changes

MEDIUM (Recommended)
  - Read and simple write operations are auto-approved
  - Shell commands and external requests need approval
  - Use when: Regular development work

HIGH
  - Most operations are auto-approved
  - Only destructive operations need approval
  - Use when: You trust the AI's decisions

ALL/CRITICAL
  - Everything is auto-approved
  - Use with caution - no safety net
  - Use when: Automated pipelines or trusted scripts

Configuration:
  In your config file (~/.config/cowork/config.toml):

  [approval]
  auto_approve_level = "medium"

Command line override:
  cowork --approval-level high

Keyboard shortcuts during prompts:
  Y - Approve this operation
  N - Deny this operation
  A - Approve all similar operations for this session
"#;

        SkillResult::success(output)
    }
}

impl Skill for PermissionsSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "permissions".to_string(),
            display_name: "Permissions".to_string(),
            description: "View and understand tool permission settings".to_string(),
            usage: "/permissions [tools|levels]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let (subcommand, _args) = Self::parse_subcommand(&ctx.args);

            match subcommand {
                "show" | "" => self.cmd_show(),
                "tools" | "list" => self.cmd_tools(),
                "levels" | "explain" => self.cmd_levels(),
                "help" | "?" => SkillResult::success(HELP_TEXT),
                _ => self.cmd_show(),
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

const HELP_TEXT: &str = r#"Permissions Management Commands:

  /permissions           - Show current approval settings
  /permissions tools     - List tools by approval level
  /permissions levels    - Explain approval levels in detail

The approval system controls which AI operations need explicit
confirmation before executing. Higher levels = more permissive.

Examples:
  /permissions
  /permissions tools
  /permissions levels"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_permissions_show() {
        let skill = PermissionsSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Permission Settings"));
    }

    #[tokio::test]
    async fn test_permissions_tools() {
        let skill = PermissionsSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: "tools".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Tools by Approval Level"));
    }

    #[tokio::test]
    async fn test_permissions_levels() {
        let skill = PermissionsSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: "levels".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Approval Levels"));
    }
}
