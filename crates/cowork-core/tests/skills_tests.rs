//! Skills system tests
//!
//! Tests for the skills/commands system that mirrors Claude Code's plugin architecture.

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

        // Should have all built-in skills
        assert!(registry.get("commit").is_some());
        assert!(registry.get("commit-push-pr").is_some());
        assert!(registry.get("push").is_some());
        assert!(registry.get("pr").is_some());
        assert!(registry.get("review").is_some());
        assert!(registry.get("clean-gone").is_some());
        assert!(registry.get("help").is_some());
    }

    #[test]
    fn test_list_skills() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let skills = registry.list();
        assert!(skills.len() >= 7, "Should have at least 7 skills");

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

mod skill_execution_tests {
    use super::*;

    #[tokio::test]
    async fn test_help_skill() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("help", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("/commit"));
        assert!(result.response.contains("/pr"));
        assert!(result.response.contains("/review"));
        assert!(result.response.contains("/clean-gone"));
    }

    #[tokio::test]
    async fn test_commit_skill_clean_repo() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("commit", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("clean")); // "Working tree is clean"
    }

    #[tokio::test]
    async fn test_commit_skill_with_changes() {
        let dir = setup_git_repo();

        // Create a new file
        std::fs::write(dir.path().join("new_file.txt"), "Hello World\n").unwrap();

        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());
        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: "Add new file".to_string(),
            data: HashMap::new(),
        };

        let result = registry.execute("commit", ctx).await;
        assert!(result.success);
        // Should generate a prompt with context
        assert!(result.response.contains("git add"));
        assert!(result.response.contains("new_file.txt"));
        assert!(result.response.contains("User hint")); // Our argument should appear
    }

    #[tokio::test]
    async fn test_review_skill_no_changes() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("review", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("No changes to review"));
    }

    #[tokio::test]
    async fn test_review_skill_with_changes() {
        let dir = setup_git_repo();

        // Modify the tracked README
        std::fs::write(dir.path().join("README.md"), "# Updated Test Project\n\nNew content.\n").unwrap();

        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());
        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("review", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Review"));
        assert!(result.response.contains("Correctness"));
        assert!(result.response.contains("Security"));
    }

    #[tokio::test]
    async fn test_push_skill() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("push", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("git push"));
    }

    #[tokio::test]
    async fn test_pr_skill() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: "My PR title".to_string(),
            data: HashMap::new(),
        };

        let result = registry.execute("pr", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("gh pr create"));
        assert!(result.response.contains("My PR title"));
    }

    #[tokio::test]
    async fn test_clean_gone_skill() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("clean-gone", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("gone"));
        assert!(result.response.contains("Safety rules"));
    }

    #[tokio::test]
    async fn test_commit_push_pr_skill() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = registry.execute("commit-push-pr", ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Branch Management"));
        assert!(result.response.contains("Commit"));
        assert!(result.response.contains("Push"));
        assert!(result.response.contains("Pull Request"));
    }
}

mod slash_command_tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_command_parsing() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        // Test basic command
        let result = registry.execute_command("/help", dir.path().to_path_buf()).await;
        assert!(result.success);

        // Test command with args
        let result = registry.execute_command("/pr My PR Title", dir.path().to_path_buf()).await;
        assert!(result.success);
        assert!(result.response.contains("My PR Title"));
    }

    #[tokio::test]
    async fn test_execute_command_without_slash() {
        let dir = setup_git_repo();
        let registry = SkillRegistry::with_builtins(dir.path().to_path_buf());

        let result = registry.execute_command("help", dir.path().to_path_buf()).await;
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

        // Create a custom help skill that overrides the builtin
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_dir(
            &skills_dir,
            "help",
            r#"---
name: help
description: Custom help skill
user-invocable: true
---

# Custom Help

This is a custom help message for this project.
"#,
        );

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        let registry = SkillRegistry::with_builtins(workspace.path().to_path_buf());
        let skill = registry.get("help").unwrap();

        // The custom skill should override the builtin
        assert_eq!(skill.info().description, "Custom help skill");
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
}

mod skill_info_tests {
    use super::*;
    use cowork_core::skills::git::*;

    #[test]
    fn test_commit_skill_info() {
        let dir = TempDir::new().unwrap();
        let skill = CommitSkill::new(dir.path().to_path_buf());
        let info = skill.info();

        assert_eq!(info.name, "commit");
        assert!(info.user_invocable);
        assert!(!info.description.is_empty());
        assert!(info.usage.contains("/commit"));
    }

    #[test]
    fn test_commit_push_pr_skill_info() {
        let dir = TempDir::new().unwrap();
        let skill = CommitPushPrSkill::new(dir.path().to_path_buf());
        let info = skill.info();

        assert_eq!(info.name, "commit-push-pr");
        assert!(info.user_invocable);
    }

    #[test]
    fn test_clean_gone_skill_info() {
        let dir = TempDir::new().unwrap();
        let skill = CleanGoneSkill::new(dir.path().to_path_buf());
        let info = skill.info();

        assert_eq!(info.name, "clean-gone");
        assert!(info.user_invocable);
    }

    #[test]
    fn test_allowed_tools() {
        let dir = TempDir::new().unwrap();

        // Skills that need to execute commands
        let commit = CommitSkill::new(dir.path().to_path_buf());
        assert!(commit.allowed_tools().is_some());

        let push = PushSkill::new(dir.path().to_path_buf());
        assert!(push.allowed_tools().is_some());

        // Review doesn't need to execute commands
        let review = ReviewSkill::new(dir.path().to_path_buf());
        assert!(review.allowed_tools().is_none());
    }
}
