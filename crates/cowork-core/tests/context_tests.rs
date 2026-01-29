//! Context management tests

use cowork_core::context::{
    compact, context_limit, should_compact, usage_stats,
    CompactConfig, ContextGatherer,
    Message, MessageRole, MemoryTier,
};
use tempfile::TempDir;
use std::fs;

/// Helper to create a message
fn msg(role: MessageRole, content: &str) -> Message {
    Message::new(role, content)
}

/// Create a test workspace with typical project files
fn setup_project_workspace() -> TempDir {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let base = dir.path();

    fs::write(
        base.join("CLAUDE.md"),
        r#"# Project Instructions

This is a Rust project. When modifying code:
- Use `cargo fmt` before committing
- Run `cargo test` to verify changes
"#,
    ).unwrap();

    fs::create_dir_all(base.join(".git")).unwrap();
    fs::write(base.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

    fs::write(
        base.join("Cargo.toml"),
        r#"[package]
name = "test-project"
version = "0.1.0"
"#,
    ).unwrap();

    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/main.rs"), "fn main() {}\n").unwrap();

    dir
}

mod monitor_tests {
    use super::*;

    #[test]
    fn test_context_limit_anthropic() {
        let limit = context_limit("anthropic", None);
        assert_eq!(limit, 200_000);
    }

    #[test]
    fn test_context_limit_unknown() {
        let limit = context_limit("unknown", None);
        assert_eq!(limit, 128_000); // fallback
    }

    #[test]
    fn test_should_compact_below_threshold() {
        // 50% usage - should not compact
        assert!(!should_compact(100_000, 0, 200_000));
    }

    #[test]
    fn test_should_compact_above_threshold() {
        // 80% usage - should compact
        assert!(should_compact(160_000, 0, 200_000));
    }

    #[test]
    fn test_should_compact_low_remaining() {
        // Less than 20k remaining - should compact
        assert!(should_compact(185_000, 0, 200_000));
    }

    #[test]
    fn test_usage_stats() {
        let usage = usage_stats(50_000, 1_000, 200_000);
        assert_eq!(usage.input_tokens, 50_000);
        assert_eq!(usage.output_tokens, 1_000);
        assert!(!usage.should_compact);
        assert!(usage.used_percentage > 0.0);
    }
}

mod context_gatherer_tests {
    use super::*;

    #[tokio::test]
    async fn test_gather_from_workspace() {
        let dir = setup_project_workspace();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        assert!(context.claude_md.is_some() || context.project_type.is_some());
    }

    #[tokio::test]
    async fn test_finds_claude_md() {
        let dir = setup_project_workspace();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        assert!(context.claude_md.is_some(), "Should find CLAUDE.md");
        let claude_content = context.claude_md.unwrap();
        assert!(claude_content.contains("Project Instructions"));
    }

    #[tokio::test]
    async fn test_gather_empty_workspace() {
        let dir = TempDir::new().unwrap();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        assert!(context.claude_md.is_none());
    }
}

mod memory_hierarchy_tests {
    use super::*;

    fn setup_memory_hierarchy() -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let base = dir.path();

        fs::write(
            base.join("CLAUDE.md"),
            "# Project Instructions\n\nThis is the main project CLAUDE.md file.",
        ).unwrap();

        fs::create_dir_all(base.join(".claude/rules")).unwrap();
        fs::write(
            base.join(".claude/rules/01-coding-style.md"),
            "# Coding Style\n\nUse 4-space indentation.",
        ).unwrap();
        fs::write(
            base.join(".claude/rules/02-testing.md"),
            "# Testing\n\nAlways write tests.",
        ).unwrap();

        fs::write(
            base.join("CLAUDE.local.md"),
            "# Local Overrides\n\nMy personal preferences.",
        ).unwrap();

        dir
    }

    #[tokio::test]
    async fn test_gather_memory_hierarchy() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;

        assert!(!hierarchy.is_empty(), "Should find memory files");
    }

    #[tokio::test]
    async fn test_memory_tiers_ordering() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;

        let mut prev_tier: Option<MemoryTier> = None;
        for file in &hierarchy.files {
            if let Some(pt) = prev_tier {
                assert!(file.tier >= pt, "Files should be sorted by tier");
            }
            prev_tier = Some(file.tier);
        }
    }

    #[tokio::test]
    async fn test_finds_project_tier() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;
        let project_files = hierarchy.files_in_tier(MemoryTier::Project);

        assert!(!project_files.is_empty(), "Should find project tier files");
        assert!(project_files[0].content.contains("Project Instructions"));
    }

    #[tokio::test]
    async fn test_finds_rules_tier() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;
        let rules_files = hierarchy.files_in_tier(MemoryTier::Rules);

        assert_eq!(rules_files.len(), 2, "Should find 2 rule files");
    }

    #[tokio::test]
    async fn test_combined_content() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;

        assert!(!hierarchy.combined_content.is_empty());
        assert!(hierarchy.combined_content.contains("Project Instructions"));
        assert!(hierarchy.combined_content.contains("Coding Style"));
    }
}

mod compact_config_tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CompactConfig::default();

        assert!(config.use_llm);
        assert!(config.preserve_instructions.is_none());
    }

    #[test]
    fn test_auto_config() {
        let config = CompactConfig::auto();

        assert!(config.use_llm);
    }

    #[test]
    fn test_from_command() {
        let config = CompactConfig::from_command(Some("keep API changes".to_string()));

        assert_eq!(
            config.preserve_instructions,
            Some("keep API changes".to_string())
        );
    }
}

mod compaction_tests {
    use super::*;

    fn generate_conversation(message_count: usize) -> Vec<Message> {
        (0..message_count)
            .map(|i| {
                let role = if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                };
                let content = format!(
                    "Message {} with some code: fn foo() {{ return {}; }}",
                    i, i
                );
                msg(role, &content)
            })
            .collect()
    }

    #[tokio::test]
    async fn test_compact_small_conversation() {
        let mut config = CompactConfig::default();
        config.use_llm = false;

        let messages = generate_conversation(5);
        let result = compact(&messages, config, None).await.unwrap();

        assert_eq!(result.messages_summarized, 5);
        assert!(result.summary.content.contains("<summary>"));
    }

    #[tokio::test]
    async fn test_compact_large_conversation() {
        let mut config = CompactConfig::default();
        config.use_llm = false;

        let messages = generate_conversation(50);
        let result = compact(&messages, config, None).await.unwrap();

        assert_eq!(result.messages_summarized, 50);
        assert!(result.chars_after <= result.chars_before);
        assert!(!result.summary.content.is_empty());
        assert!(result.summary.content.contains("<summary>"));
    }

    #[tokio::test]
    async fn test_compact_preserves_instructions() {
        let config = CompactConfig {
            preserve_instructions: Some("API endpoints".to_string()),
            use_llm: false,
        };

        let messages = generate_conversation(30);
        let result = compact(&messages, config, None).await.unwrap();

        assert!(result.summary.content.contains("Preserved Context"));
        assert!(result.summary.content.contains("API endpoints"));
    }

    #[tokio::test]
    async fn test_compact_returns_user_message() {
        let mut config = CompactConfig::default();
        config.use_llm = false;

        let messages = generate_conversation(30);
        let result = compact(&messages, config, None).await.unwrap();

        assert!(matches!(result.summary.role, MessageRole::User));
    }

    #[tokio::test]
    async fn test_compact_empty_messages() {
        let config = CompactConfig::default();
        let messages: Vec<Message> = vec![];

        let result = compact(&messages, config, None).await.unwrap();

        assert_eq!(result.messages_summarized, 0);
        assert!(result.summary.content.contains("No prior context"));
    }
}
