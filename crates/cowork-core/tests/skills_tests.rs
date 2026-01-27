//! Skills system tests
//!
//! Tests for the skills/commands system that mirrors Claude Code's plugin architecture.
//! Skills are prompt templates (SKILL.md format) that get expanded with substitutions
//! and injected into the conversation for the LLM to follow.

#[allow(unused_imports)]
use cowork_core::skills::{Skill, SkillRegistry, SkillContext};
use std::collections::HashMap;
use tempfile::TempDir;

/// Create a temp git repo for testing skills
fn setup_git_repo() -> TempDir {
    let dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    // Configure git user for commits
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to config email");

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to config name");

    // Create initial commit
    std::fs::write(dir.path().join("README.md"), "# Test Project\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("Failed to add");
    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to commit");

    dir
}

mod skill_registry_tests {
    use super::*;

    #[test]
    fn test_registry_with_builtins() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        // Should have all 6 official built-in skills
        assert!(registry.get("commit").is_some());
        assert!(registry.get("commit-push-pr").is_some());
        assert!(registry.get("clean_gone").is_some());
        assert!(registry.get("code-review").is_some());
        assert!(registry.get("feature-dev").is_some());
        assert!(registry.get("review-pr").is_some());
    }

    #[test]
    fn test_list_skills() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let skills = registry.list();
        assert_eq!(skills.len(), 6, "Should have exactly 6 built-in skills");

        // All skills should have names and descriptions
        for skill in &skills {
            assert!(!skill.name.is_empty());
            assert!(!skill.description.is_empty());
        }
    }

    #[test]
    fn test_list_user_invocable() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let skills = registry.list_user_invocable();

        // All built-in skills should be user-invocable
        assert!(!skills.is_empty());
        for skill in &skills {
            assert!(skill.user_invocable);
        }
    }

    #[test]
    fn test_get_nonexistent_skill() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        assert!(registry.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_execute_unknown_skill() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("nonexistent", ctx).await;
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Unknown skill"));
    }
}

mod skill_template_tests {
    use super::*;

    #[test]
    fn test_commit_skill_template() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let skill = registry.get("commit").unwrap();
        let template = skill.prompt_template();

        // Template should contain command substitution markers (resolved at invocation time)
        assert!(template.contains("!`git status`"));
        assert!(template.contains("!`git diff HEAD`"));
        assert!(template.contains("git commit"));
    }

    #[tokio::test]
    async fn test_commit_skill_execute_substitutes_args() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        // execute() returns the body with $ARGUMENTS substituted (but NOT command substitution)
        let result = registry.execute("commit", ctx).await;
        assert!(result.success);
        // The template body is returned as-is (with only $ARGUMENTS replaced)
        assert!(result.response.contains("git commit"));
    }

    #[tokio::test]
    async fn test_feature_dev_skill_substitutes_arguments() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: "Add dark mode toggle".to_string(),
            data: HashMap::new(),
        };

        let result = registry.execute("feature-dev", ctx).await;
        assert!(result.success);
        // $ARGUMENTS should be replaced
        assert!(result.response.contains("Add dark mode toggle"));
    }

    #[test]
    fn test_review_pr_skill_template() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let skill = registry.get("review-pr").unwrap();
        let template = skill.prompt_template();

        assert!(template.contains("Comprehensive PR Review"));
        assert!(template.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_clean_gone_skill_template() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let skill = registry.get("clean_gone").unwrap();
        let template = skill.prompt_template();

        assert!(template.contains("gone"));
        assert!(template.contains("git branch -v"));
    }

    #[test]
    fn test_commit_push_pr_skill_template() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let skill = registry.get("commit-push-pr").unwrap();
        let template = skill.prompt_template();

        assert!(template.contains("commit"));
        assert!(template.contains("Push") || template.contains("push"));
        assert!(template.contains("pull request"));
    }

    #[test]
    fn test_builtin_skills_have_allowed_tools() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        // commit skill should have specific allowed tools
        let commit = registry.get("commit").unwrap();
        let tools = commit.allowed_tools();
        assert!(tools.is_some(), "commit skill should have allowed tools");

        // commit-push-pr should also have allowed tools
        let cpr = registry.get("commit-push-pr").unwrap();
        let tools = cpr.allowed_tools();
        assert!(tools.is_some(), "commit-push-pr skill should have allowed tools");
    }
}

mod slash_command_tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_command_parsing() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        // Test basic command
        let result = registry.execute_command("/commit", dir.path().to_path_buf()).await;
        assert!(result.success);

        // Test command with args
        let result = registry.execute_command("/feature-dev My Feature", dir.path().to_path_buf()).await;
        assert!(result.success);
        assert!(result.response.contains("My Feature"));
    }

    #[tokio::test]
    async fn test_execute_command_without_slash() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let result = registry.execute_command("commit", dir.path().to_path_buf()).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_execute_unknown_command() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let result = registry.execute_command("/unknown", dir.path().to_path_buf()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unknown skill"));
    }
}

mod dynamic_skill_tests {
    use super::*;
    use std::path::Path;

    fn create_skill_dir(base: &Path, name: &str, content: &str) {
        let skill_dir = base.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn test_registry_loads_project_skills() {
        let workspace = TempDir::new().unwrap();

        // Create a project skill
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "my-custom-skill",
            r#"---
name: my-custom-skill
description: A custom project skill
user-invocable: true
---

# My Custom Skill

This is my custom skill with $ARGUMENTS.
"#,
        );

        // Initialize git repo (required for with_builtins)
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());

        // Should have the custom skill
        let custom_skill = registry.get("my-custom-skill");
        assert!(custom_skill.is_some(), "Custom skill should be loaded");

        let info = custom_skill.unwrap().info();
        assert_eq!(info.name, "my-custom-skill");
        assert_eq!(info.description, "A custom project skill");
        assert!(info.user_invocable);
    }

    #[tokio::test]
    async fn test_execute_dynamic_skill() {
        let workspace = TempDir::new().unwrap();

        // Create a project skill
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "greet",
            r#"---
name: greet
description: Greet someone
usage: /greet <name>
---

# Greeting Skill

Please greet $ARGUMENTS warmly and wish them a good day.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());

        let ctx = SkillContext {
            workspace: workspace.path().to_path_buf(),
            args: "Alice".to_string(),
            data: HashMap::new(),
        };

        let result = registry.execute("greet", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Alice"));
        assert!(result.response.contains("greet"));
    }

    #[test]
    fn test_dynamic_skill_allowed_tools() {
        let workspace = TempDir::new().unwrap();

        // Create a skill with specific allowed tools
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "build",
            r#"---
name: build
description: Build the project
allowed-tools: Bash, Read
user-invocable: true
---

# Build Skill

Run the build command.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());
        // The dynamic skill overrides the builtin "build" skill
        let skill = registry.get("build").unwrap();

        let tools = skill.allowed_tools();
        assert!(tools.is_some());
        let tools = tools.unwrap();
        assert!(tools.contains(&"Bash"));
        assert!(tools.contains(&"Read"));
    }

    #[test]
    fn test_dynamic_skill_override_builtin() {
        let workspace = TempDir::new().unwrap();

        // Create a custom commit skill that overrides the builtin
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "commit",
            r#"---
name: commit
description: Custom commit skill
user-invocable: true
---

# Custom Commit

This is a custom commit workflow for this project.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());
        let skill = registry.get("commit").unwrap();

        // The custom skill should override the builtin
        assert_eq!(skill.info().description, "Custom commit skill");
    }

    #[test]
    fn test_list_includes_dynamic_skills() {
        let workspace = TempDir::new().unwrap();

        // Create a project skill
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "deploy",
            r#"---
name: deploy
description: Deploy to production
user-invocable: true
---

Deploy instructions.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());
        let skills = registry.list();

        // Should include the dynamic skill
        assert!(skills.iter().any(|s| s.name == "deploy"));
    }

    /// Test skills with context: fork should indicate subagent execution
    #[test]
    fn test_skill_context_fork_runs_in_subagent() {
        let workspace = TempDir::new().unwrap();

        // Create a skill with context: fork
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "research",
            r#"---
name: research
description: Deep research task
context: fork
agent: Explore
---

Research $ARGUMENTS thoroughly.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());
        let skill = registry.get("research").unwrap();

        // Should indicate it runs in a subagent
        assert!(skill.runs_in_subagent(), "Skill with context: fork should run in subagent");
        assert_eq!(skill.subagent_type(), Some("Explore"));
    }

    /// Test skills without context: fork should run inline
    #[test]
    fn test_skill_default_runs_inline() {
        let workspace = TempDir::new().unwrap();

        // Create a skill without context: fork
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "lint",
            r#"---
name: lint
description: Run linter
---

Run the linter on the codebase.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());
        let skill = registry.get("lint").unwrap();

        // Should NOT run in subagent by default
        assert!(!skill.runs_in_subagent(), "Skill without context: fork should run inline");
        assert_eq!(skill.subagent_type(), None);
    }

    /// Test skill with model override
    #[test]
    fn test_skill_model_override() {
        let workspace = TempDir::new().unwrap();

        // Create a skill with model override
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "quick-task",
            r#"---
name: quick-task
description: A quick task using fast model
model: haiku
---

Do this quickly.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());
        let skill = registry.get("quick-task").unwrap();

        assert_eq!(skill.model_override(), Some("haiku"));
    }
}

/// Tests for positional argument substitution
mod argument_substitution_tests {
    use cowork_core::tools::skill::substitute_arguments;

    #[test]
    fn test_full_arguments_substitution() {
        let template = "Process: $ARGUMENTS";
        let result = substitute_arguments(template, "file1.txt file2.txt");
        assert_eq!(result, "Process: file1.txt file2.txt");
    }

    #[test]
    fn test_braced_arguments_substitution() {
        let template = "Process: ${ARGUMENTS}";
        let result = substitute_arguments(template, "file1.txt file2.txt");
        assert_eq!(result, "Process: file1.txt file2.txt");
    }

    #[test]
    fn test_positional_shorthand() {
        let template = "Move $0 to $1";
        let result = substitute_arguments(template, "source.txt dest.txt");
        assert_eq!(result, "Move source.txt to dest.txt");
    }

    #[test]
    fn test_positional_indexed() {
        let template = "First: $ARGUMENTS[0], Second: $ARGUMENTS[1]";
        let result = substitute_arguments(template, "alpha beta");
        assert_eq!(result, "First: alpha, Second: beta");
    }

    #[test]
    fn test_positional_indexed_braced() {
        let template = "First: ${ARGUMENTS[0]}, Second: ${ARGUMENTS[1]}";
        let result = substitute_arguments(template, "alpha beta");
        assert_eq!(result, "First: alpha, Second: beta");
    }

    #[test]
    fn test_mixed_substitution() {
        let template = "Command: $0, Full: $ARGUMENTS";
        let result = substitute_arguments(template, "git status --short");
        assert_eq!(result, "Command: git, Full: git status --short");
    }

    #[test]
    fn test_missing_positional_argument() {
        let template = "Third: $2";
        let result = substitute_arguments(template, "only two");
        assert_eq!(result, "Third: ");  // Empty string for missing args
    }

    #[test]
    fn test_empty_arguments() {
        let template = "Args: $ARGUMENTS, First: $0";
        let result = substitute_arguments(template, "");
        assert_eq!(result, "Args: , First: ");
    }
}
