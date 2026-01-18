//! Skill management command
//!
//! Provides commands for managing skills:
//! - /skill list - List installed skills
//! - /skill add <url> [--global|--local] - Install a skill from a URL
//! - /skill remove <name> - Remove an installed skill
//! - /skill info <name> - Show details about a skill

use super::installer::{InstallLocation, SkillInstaller};
use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};

/// Skill management command
pub struct SkillCmdSkill {
    workspace: std::path::PathBuf,
}

impl SkillCmdSkill {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self { workspace }
    }

    /// Parse subcommand and flags
    fn parse_args(args: &str) -> (String, Vec<String>, bool, bool) {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.is_empty() {
            return ("list".to_string(), vec![], false, false);
        }

        let subcommand = parts[0].to_string();
        let mut positional = Vec::new();
        let mut global = false;
        let mut force = false;

        for part in &parts[1..] {
            match *part {
                "--global" | "-g" => global = true,
                "--local" | "-l" | "--project" | "-p" => global = false,
                "--force" | "-f" => force = true,
                _ => positional.push(part.to_string()),
            }
        }

        (subcommand, positional, global, force)
    }

    /// List installed skills
    fn cmd_list(&self) -> SkillResult {
        let installer = SkillInstaller::new(self.workspace.clone());
        let skills = installer.list_installed();

        if skills.is_empty() {
            return SkillResult::success(
                "No custom skills installed.\n\n\
                 Use `/skill add <url>` to install a skill from a zip file.\n\n\
                 Skills are loaded from:\n  \
                 - Global: ~/.claude/skills/\n  \
                 - Project: .cowork/skills/"
            );
        }

        let mut output = String::from("Installed Skills:\n\n");

        // Group by location
        let global_skills: Vec<_> = skills
            .iter()
            .filter(|s| s.location == InstallLocation::Global)
            .collect();
        let project_skills: Vec<_> = skills
            .iter()
            .filter(|s| s.location == InstallLocation::Project)
            .collect();

        if !project_skills.is_empty() {
            output.push_str("Project Skills (.cowork/skills/):\n");
            for skill in project_skills {
                output.push_str(&format!(
                    "  /{} - {}\n",
                    skill.name, skill.description
                ));
            }
            output.push('\n');
        }

        if !global_skills.is_empty() {
            output.push_str("Global Skills (~/.claude/skills/):\n");
            for skill in global_skills {
                output.push_str(&format!(
                    "  /{} - {}\n",
                    skill.name, skill.description
                ));
            }
        }

        SkillResult::success(output.trim())
    }

    /// Install a skill from URL
    fn cmd_add(&self, args: Vec<String>, global: bool, force: bool) -> SkillResult {
        if args.is_empty() {
            return SkillResult::error(
                "Usage: /skill add <url> [--global|--local] [--force]\n\n\
                 Examples:\n  \
                 /skill add https://example.com/my-skill.zip\n  \
                 /skill add https://example.com/my-skill.zip --global\n  \
                 /skill add https://example.com/my-skill.zip --local --force"
            );
        }

        let url = &args[0];
        let location = if global {
            InstallLocation::Global
        } else {
            InstallLocation::Project
        };

        let installer = SkillInstaller::new(self.workspace.clone());

        match installer.install_from_url(url, location, force) {
            Ok(result) => {
                SkillResult::success(format!(
                    "Installed skill '{}' to {}\n\nDescription: {}\nPath: {}\n\nUse /{} to run it.",
                    result.name,
                    result.location,
                    result.description,
                    result.path.display(),
                    result.name
                ))
            }
            Err(e) => SkillResult::error(format!("Failed to install skill: {}", e)),
        }
    }

    /// Remove an installed skill
    fn cmd_remove(&self, args: Vec<String>, global: bool) -> SkillResult {
        if args.is_empty() {
            return SkillResult::error("Usage: /skill remove <name> [--global|--local]");
        }

        let name = &args[0];
        let location = if global {
            Some(InstallLocation::Global)
        } else {
            None // Will try project first, then global
        };

        let installer = SkillInstaller::new(self.workspace.clone());

        match installer.uninstall(name, location) {
            Ok(path) => {
                SkillResult::success(format!(
                    "Removed skill '{}' from {}",
                    name,
                    path.display()
                ))
            }
            Err(e) => SkillResult::error(format!("Failed to remove skill: {}", e)),
        }
    }

    /// Show info about a skill
    fn cmd_info(&self, args: Vec<String>) -> SkillResult {
        if args.is_empty() {
            return SkillResult::error("Usage: /skill info <name>");
        }

        let name = &args[0];
        let installer = SkillInstaller::new(self.workspace.clone());
        let skills = installer.list_installed();

        match skills.iter().find(|s| s.name == *name) {
            Some(skill) => {
                let skill_file = skill.path.join("SKILL.md");
                let content = std::fs::read_to_string(&skill_file)
                    .unwrap_or_else(|_| "(could not read SKILL.md)".to_string());

                SkillResult::success(format!(
                    "Skill: {}\nDescription: {}\nLocation: {}\nPath: {}\n\n---\n\n{}",
                    skill.name,
                    skill.description,
                    skill.location,
                    skill.path.display(),
                    content
                ))
            }
            None => SkillResult::error(format!("Skill '{}' not found", name)),
        }
    }
}

impl Skill for SkillCmdSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "skill".to_string(),
            display_name: "Skill Management".to_string(),
            description: "Install, remove, and manage custom skills".to_string(),
            usage: "/skill <list|add|remove|info> [args...]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let (subcommand, args, global, force) = Self::parse_args(&ctx.args);

            match subcommand.as_str() {
                "list" | "ls" | "" => self.cmd_list(),
                "add" | "install" => self.cmd_add(args, global, force),
                "remove" | "rm" | "uninstall" | "delete" => self.cmd_remove(args, global),
                "info" | "show" => self.cmd_info(args),
                "help" | "?" => SkillResult::success(HELP_TEXT),
                _ => SkillResult::error(format!(
                    "Unknown subcommand: '{}'\n\n{}",
                    subcommand, HELP_TEXT
                )),
            }
        })
    }

    fn prompt_template(&self) -> &str {
        "" // Skill command doesn't need AI processing
    }
}

const HELP_TEXT: &str = r#"Skill Management Commands:

  /skill list              - List installed skills
  /skill add <url>         - Install a skill from a zip URL
  /skill remove <name>     - Remove an installed skill
  /skill info <name>       - Show details about a skill

Options:
  --global, -g     Install/remove from global (~/.claude/skills/)
  --local, -l      Install/remove from project (.cowork/skills/) [default]
  --force, -f      Overwrite existing skill

Examples:
  /skill add https://example.com/my-skill.zip
  /skill add https://github.com/user/skill/archive/main.zip --global
  /skill remove my-skill
  /skill info my-skill

Skill Package Format:
  A skill package is a zip file containing:
  - SKILL.md (required) - Skill definition with YAML frontmatter
  - Additional files as needed (reference.md, templates, etc.)

SKILL.md Example:
  ---
  name: my-skill
  description: What my skill does
  allowed-tools: Bash, Read, Write
  ---

  # My Skill

  Instructions for Claude..."#;
