//! Vim mode skill
//!
//! Provides /vim command for toggling vim keybindings mode:
//! - /vim - Toggle vim mode
//! - /vim on - Enable vim mode
//! - /vim off - Disable vim mode
//! - /vim status - Show current mode

use std::path::PathBuf;

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};

/// Vim skill - toggle vim keybindings mode
pub struct VimSkill {
    _workspace: PathBuf,
}

impl VimSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { _workspace: workspace }
    }

    /// Parse subcommand from args
    fn parse_subcommand(args: &str) -> &str {
        let trimmed = args.trim().to_lowercase();
        if trimmed.is_empty() {
            "toggle"
        } else {
            match trimmed.as_str() {
                "on" | "enable" | "1" | "true" => "on",
                "off" | "disable" | "0" | "false" => "off",
                "status" | "?" => "status",
                _ => "toggle",
            }
        }
    }

    /// Handle vim mode toggle (returns instruction for UI layer)
    fn cmd_toggle(&self) -> SkillResult {
        // This would need to communicate with the UI layer
        // For now, return a response that can be interpreted
        SkillResult::success(
            "Vim mode toggled.\n\n\
             Note: Vim mode affects input handling in the CLI.\n\n\
             In vim mode:\n\
             - Press 'i' to enter insert mode\n\
             - Press Esc to enter normal mode\n\
             - Use hjkl for navigation\n\
             - Use :wq to submit, :q to cancel"
        ).with_data(serde_json::json!({
            "action": "vim_toggle"
        }))
    }

    /// Enable vim mode
    fn cmd_on(&self) -> SkillResult {
        SkillResult::success(
            "Vim mode enabled.\n\n\
             You're now in normal mode. Press 'i' to insert text.\n\n\
             Quick reference:\n\
             - i        Enter insert mode\n\
             - Esc      Return to normal mode\n\
             - :wq      Submit message\n\
             - :q       Cancel/clear\n\
             - hjkl     Navigate in normal mode"
        ).with_data(serde_json::json!({
            "action": "vim_enable",
            "vim_mode": true
        }))
    }

    /// Disable vim mode
    fn cmd_off(&self) -> SkillResult {
        SkillResult::success(
            "Vim mode disabled.\n\n\
             Standard editing mode is now active.\n\
             Use Ctrl+Enter to submit messages."
        ).with_data(serde_json::json!({
            "action": "vim_disable",
            "vim_mode": false
        }))
    }

    /// Show current vim mode status
    fn cmd_status(&self) -> SkillResult {
        // In a full implementation, this would query the UI state
        SkillResult::success(
            "Vim mode status: Check the status bar for current mode.\n\n\
             To toggle: /vim\n\
             To enable: /vim on\n\
             To disable: /vim off"
        ).with_data(serde_json::json!({
            "action": "vim_status"
        }))
    }
}

impl Skill for VimSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "vim".to_string(),
            display_name: "Vim Mode".to_string(),
            description: "Toggle vim-style keybindings for input".to_string(),
            usage: "/vim [on|off|status]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let subcommand = Self::parse_subcommand(&ctx.args);

            match subcommand {
                "toggle" => self.cmd_toggle(),
                "on" => self.cmd_on(),
                "off" => self.cmd_off(),
                "status" => self.cmd_status(),
                _ => self.cmd_toggle(),
            }
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
    async fn test_vim_toggle() {
        let skill = VimSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_vim_on() {
        let skill = VimSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: "on".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("enabled"));
    }

    #[tokio::test]
    async fn test_vim_off() {
        let skill = VimSkill::new(PathBuf::from("."));
        let ctx = SkillContext {
            workspace: PathBuf::from("."),
            args: "off".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("disabled"));
    }
}
