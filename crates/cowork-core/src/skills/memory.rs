//! Memory file management skill
//!
//! Provides /memory command for managing CLAUDE.md memory files:
//! - /memory list - List all memory files in the hierarchy
//! - /memory show [file] - Show content of a memory file
//! - /memory edit - Instructions on editing memory files
//! - /memory add <text> - Add content to project CLAUDE.md

use std::path::PathBuf;

use crate::context::{ContextGatherer, MemoryTier};

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};

/// Memory skill - manage CLAUDE.md memory files
pub struct MemorySkill {
    workspace: PathBuf,
}

impl MemorySkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    /// Parse subcommand from args
    fn parse_subcommand(args: &str) -> (&str, Vec<&str>) {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.is_empty() {
            ("list", vec![])
        } else {
            (parts[0], parts[1..].to_vec())
        }
    }

    /// List all memory files
    async fn cmd_list(&self) -> SkillResult {
        let gatherer = ContextGatherer::new(&self.workspace);
        let hierarchy = gatherer.gather_memory_hierarchy().await;

        if hierarchy.is_empty() {
            let mut output = String::from("No memory files found.\n\n");
            output.push_str("Memory files provide persistent instructions for the AI.\n\n");
            output.push_str("To create one, use: /memory add <instructions>\n");
            output.push_str("Or create a CLAUDE.md file in your project root.\n\n");
            output.push_str("Memory hierarchy (priority order):\n");
            output.push_str("  1. Enterprise: /etc/claude-code/CLAUDE.md\n");
            output.push_str("  2. Project:    ./CLAUDE.md, ./.claude/CLAUDE.md\n");
            output.push_str("  3. Rules:      ./.claude/rules/*.md\n");
            output.push_str("  4. User:       ~/.claude/CLAUDE.md, ./CLAUDE.local.md\n");
            return SkillResult::success(output);
        }

        let mut output = String::from("Memory Files:\n\n");

        for tier in [MemoryTier::Enterprise, MemoryTier::Project, MemoryTier::Rules, MemoryTier::User] {
            let tier_files = hierarchy.files_in_tier(tier);
            if !tier_files.is_empty() {
                let tier_name = match tier {
                    MemoryTier::Enterprise => "Enterprise (highest priority)",
                    MemoryTier::Project => "Project",
                    MemoryTier::Rules => "Rules",
                    MemoryTier::User => "User (lowest priority)",
                };
                output.push_str(&format!("{}:\n", tier_name));

                for file in tier_files {
                    let size_kb = file.size as f64 / 1024.0;
                    if size_kb >= 1.0 {
                        output.push_str(&format!("  - {} ({:.1} KB)\n", file.path.display(), size_kb));
                    } else {
                        output.push_str(&format!("  - {} ({} bytes)\n", file.path.display(), file.size));
                    }
                }
                output.push('\n');
            }
        }

        output.push_str(&format!("Total: {} files, {} bytes\n", hierarchy.file_count(), hierarchy.total_size));

        SkillResult::success(output.trim())
    }

    /// Show content of a specific memory file
    async fn cmd_show(&self, args: Vec<&str>) -> SkillResult {
        let gatherer = ContextGatherer::new(&self.workspace);
        let hierarchy = gatherer.gather_memory_hierarchy().await;

        if hierarchy.is_empty() {
            return SkillResult::error("No memory files found. Use `/memory add` to create one.");
        }

        if args.is_empty() {
            // Show all content combined
            let mut output = String::from("=== Combined Memory Content ===\n\n");
            output.push_str(&hierarchy.combined_content);
            return SkillResult::success(output);
        }

        // Find specific file
        let filename = args.join(" ");
        let file = hierarchy.files.iter().find(|f| {
            let path_str = f.path.to_string_lossy();
            path_str.contains(&filename) ||
            f.path.file_name().map(|n| n.to_string_lossy().contains(&filename)).unwrap_or(false)
        });

        match file {
            Some(f) => {
                let mut output = format!("=== {} ===\n", f.path.display());
                output.push_str(&format!("Tier: {} | Size: {} bytes\n\n", f.tier, f.size));
                output.push_str(&f.content);
                SkillResult::success(output)
            }
            None => {
                let available: Vec<String> = hierarchy.files.iter()
                    .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().to_string()))
                    .collect();
                SkillResult::error(format!(
                    "File '{}' not found.\n\nAvailable files: {}",
                    filename,
                    available.join(", ")
                ))
            }
        }
    }

    /// Instructions for editing memory files
    fn cmd_edit(&self) -> SkillResult {
        let project_path = self.workspace.join("CLAUDE.md");
        let local_path = self.workspace.join("CLAUDE.local.md");

        let output = format!(r#"Editing Memory Files

Memory files are Markdown files that provide persistent instructions.

Recommended approach:
1. Project instructions: Edit {}
   - Shared with team via version control
   - Project-specific coding standards and context

2. Personal preferences: Edit {}
   - Gitignored, personal settings only
   - Your individual preferences and shortcuts

3. Rules directory: .claude/rules/*.md
   - Multiple rule files for organization
   - Good for modular project guidelines

To create a file:
  /memory add <your instructions here>

Or use your editor:
  $EDITOR {}

Example CLAUDE.md content:
```
# Project Instructions

## Tech Stack
- Rust with Tokio for async
- SQLite for persistence

## Conventions
- Use snake_case for functions
- Add doc comments to public items
- Run `cargo clippy` before commits
```
"#,
            project_path.display(),
            local_path.display(),
            project_path.display()
        );

        SkillResult::success(output)
    }

    /// Add content to project CLAUDE.md
    async fn cmd_add(&self, args: Vec<&str>) -> SkillResult {
        if args.is_empty() {
            return SkillResult::error("Usage: /memory add <content>\n\nExample: /memory add Use snake_case for function names");
        }

        let content = args.join(" ");
        let project_md = self.workspace.join("CLAUDE.md");

        // Read existing content if file exists
        let existing = tokio::fs::read_to_string(&project_md).await.unwrap_or_default();

        let new_content = if existing.is_empty() {
            format!("# Project Instructions\n\n{}\n", content)
        } else {
            format!("{}\n\n{}\n", existing.trim(), content)
        };

        // Write the file
        match tokio::fs::write(&project_md, &new_content).await {
            Ok(_) => SkillResult::success(format!(
                "Added to {}:\n\n{}\n\nTotal size: {} bytes",
                project_md.display(),
                content,
                new_content.len()
            )),
            Err(e) => SkillResult::error(format!("Failed to write {}: {}", project_md.display(), e)),
        }
    }
}

impl Skill for MemorySkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "memory".to_string(),
            display_name: "Memory Files".to_string(),
            description: "Manage CLAUDE.md memory files for persistent instructions".to_string(),
            usage: "/memory <list|show|edit|add> [args...]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let (subcommand, args) = Self::parse_subcommand(&ctx.args);

            match subcommand {
                "list" | "ls" => self.cmd_list().await,
                "show" | "cat" | "view" => self.cmd_show(args).await,
                "edit" | "open" => self.cmd_edit(),
                "add" | "append" => self.cmd_add(args).await,
                "help" | "?" => SkillResult::success(HELP_TEXT),
                _ => {
                    // If not a subcommand, treat as show with filename
                    let all_args: Vec<&str> = ctx.args.split_whitespace().collect();
                    self.cmd_show(all_args).await
                }
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

const HELP_TEXT: &str = r#"Memory File Management Commands:

  /memory               - List all memory files (same as /memory list)
  /memory list          - List all memory files in the hierarchy
  /memory show [file]   - Show content (all files or specific file)
  /memory edit          - Instructions for editing memory files
  /memory add <content> - Add content to project CLAUDE.md

Memory Hierarchy (priority order):
  1. Enterprise: /etc/claude-code/CLAUDE.md
  2. Project:    ./CLAUDE.md, ./.claude/CLAUDE.md
  3. Rules:      ./.claude/rules/*.md
  4. User:       ~/.claude/CLAUDE.md, ./CLAUDE.local.md

Examples:
  /memory add Use async/await consistently
  /memory show CLAUDE.md
  /memory list"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_memory_list_empty() {
        let temp = TempDir::new().unwrap();
        let skill = MemorySkill::new(temp.path().to_path_buf());
        let ctx = SkillContext {
            workspace: temp.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("No memory files found"));
    }

    #[tokio::test]
    async fn test_memory_add() {
        let temp = TempDir::new().unwrap();
        let skill = MemorySkill::new(temp.path().to_path_buf());
        let ctx = SkillContext {
            workspace: temp.path().to_path_buf(),
            args: "add Use snake_case".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Added to"));

        // Verify file was created
        let content = tokio::fs::read_to_string(temp.path().join("CLAUDE.md")).await.unwrap();
        assert!(content.contains("Use snake_case"));
    }

    #[tokio::test]
    async fn test_memory_show_with_file() {
        let temp = TempDir::new().unwrap();

        // Create a CLAUDE.md file
        tokio::fs::write(temp.path().join("CLAUDE.md"), "# Test\nHello world").await.unwrap();

        let skill = MemorySkill::new(temp.path().to_path_buf());
        let ctx = SkillContext {
            workspace: temp.path().to_path_buf(),
            args: "show".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Hello world"));
    }
}
