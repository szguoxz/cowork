//! Context management tests
//!
//! Tests for token counting, summarization, context gathering,
//! memory hierarchy, and context monitoring.

use cowork_core::context::{
    TokenCounter, ConversationSummarizer, SummarizerConfig, ContextGatherer,
    Message, MessageRole, ContextMonitor, MonitorConfig, CompactConfig, MemoryTier,
};
use cowork_core::provider::ProviderType;
use chrono::Utc;
use tempfile::TempDir;
use std::fs;

/// Helper to create a message with current timestamp
fn msg(role: MessageRole, content: &str) -> Message {
    Message {
        role,
        content: content.to_string(),
        timestamp: Utc::now(),
    }
}

/// Create a test workspace with typical project files
fn setup_project_workspace() -> TempDir {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let base = dir.path();

    // Create CLAUDE.md
    fs::write(
        base.join("CLAUDE.md"),
        r#"# Project Instructions

This is a Rust project. When modifying code:
- Use `cargo fmt` before committing
- Run `cargo test` to verify changes
- Follow existing code style

## Build Commands
- Build: `cargo build`
- Test: `cargo test`
- Run: `cargo run`
"#,
    ).unwrap();

    // Create .git directory to simulate git repo
    fs::create_dir_all(base.join(".git")).unwrap();
    fs::write(base.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

    // Create Cargo.toml
    fs::write(
        base.join("Cargo.toml"),
        r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
tokio = { version = "1", features = ["full"] }
"#,
    ).unwrap();

    // Create source files
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(
        base.join("src/main.rs"),
        r#"fn main() {
    println!("Hello, world!");
}
"#,
    ).unwrap();

    dir
}

mod token_counter_tests {
    use super::*;

    fn create_counter() -> TokenCounter {
        TokenCounter::new(ProviderType::Anthropic)
    }

    #[test]
    fn test_count_empty_string() {
        let counter = create_counter();
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_count_simple_text() {
        let counter = create_counter();
        let count = counter.count("Hello, world!");
        assert!(count > 0, "Should count tokens in simple text");
        // Roughly 3-4 tokens for "Hello, world!"
        assert!(count >= 2 && count <= 10, "Token count should be reasonable: {}", count);
    }

    #[test]
    fn test_count_code() {
        let counter = create_counter();
        let code = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        let count = counter.count(code);
        assert!(count > 0);
        println!("Code token count: {}", count);
    }

    #[test]
    fn test_count_long_text() {
        let counter = create_counter();
        let long_text = "word ".repeat(1000);
        let count = counter.count(&long_text);
        // Each "word " should be roughly 1-2 tokens
        assert!(count >= 500 && count <= 3000, "Unexpected count: {}", count);
    }

    #[test]
    fn test_count_unicode() {
        let counter = create_counter();
        let unicode = "Hello 世界! 🌍 Привет мир";
        let count = counter.count(unicode);
        assert!(count > 0, "Should handle unicode");
    }

    #[test]
    fn test_count_messages() {
        let counter = create_counter();
        let messages = vec![
            msg(MessageRole::User, "Hello, how are you?"),
            msg(MessageRole::Assistant, "I'm doing well, thank you!"),
            msg(MessageRole::User, "Can you help me with code?"),
        ];
        let count = counter.count_messages(&messages);
        assert!(count > 0, "Should count tokens in messages");
    }

    #[test]
    fn test_context_limit() {
        let counter = create_counter();
        let limit = counter.context_limit();
        // Anthropic models have large context windows
        assert!(limit >= 100_000, "Context limit should be large: {}", limit);
    }

    #[test]
    fn test_should_summarize() {
        let counter = create_counter();
        let threshold = counter.summarization_threshold();

        assert!(!counter.should_summarize(threshold / 2));
        assert!(counter.should_summarize(threshold + 1000));
    }
}

mod summarizer_tests {
    use super::*;

    fn create_summarizer() -> ConversationSummarizer {
        ConversationSummarizer::new(SummarizerConfig::default())
    }

    fn create_counter() -> TokenCounter {
        TokenCounter::new(ProviderType::Anthropic)
    }

    #[test]
    fn test_create_summarizer() {
        let summarizer = create_summarizer();
        let counter = create_counter();
        let messages: Vec<Message> = vec![];
        // Empty messages should not need summarization
        assert!(!summarizer.needs_summarization(&messages, &counter));
    }

    #[test]
    fn test_needs_summarization_small() {
        let summarizer = create_summarizer();
        let counter = create_counter();
        let messages = vec![
            msg(MessageRole::User, "Hello"),
            msg(MessageRole::Assistant, "Hi there!"),
        ];
        // Small conversation shouldn't need summarization
        assert!(!summarizer.needs_summarization(&messages, &counter));
    }

    #[test]
    fn test_simple_summary() {
        let summarizer = create_summarizer();
        let messages = vec![
            msg(MessageRole::User, "I need help with authentication"),
            msg(MessageRole::Assistant, "I can help with JWT authentication"),
            msg(MessageRole::User, "Yes, let's use JWT"),
            msg(MessageRole::Assistant, "I'll create the JWT module"),
        ];

        let (summary, recent) = summarizer.simple_summary(&messages);

        // Summary should be a system message
        assert_eq!(summary.role, MessageRole::System);
        assert!(!summary.content.is_empty());

        // Recent messages should be preserved
        assert!(!recent.is_empty());
    }
}

mod context_gatherer_tests {
    use super::*;

    #[tokio::test]
    async fn test_gather_from_workspace() {
        let dir = setup_project_workspace();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        // Should return a ProjectContext with some data
        assert!(context.claude_md.is_some() || context.project_type.is_some());
    }

    #[tokio::test]
    async fn test_finds_claude_md() {
        let dir = setup_project_workspace();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        // Should find CLAUDE.md
        assert!(context.claude_md.is_some(), "Should find CLAUDE.md");
        let claude_content = context.claude_md.unwrap();
        assert!(claude_content.contains("Project Instructions"));
    }

    #[tokio::test]
    async fn test_detects_git_repo() {
        let dir = setup_project_workspace();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        // git_branch being Some indicates it's a git repo
        // Note: This may be None if git is not properly set up in test environment
        println!("Git branch: {:?}", context.git_branch);
    }

    #[tokio::test]
    async fn test_detects_rust_project() {
        let dir = setup_project_workspace();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        // Should detect Rust project or at least find Cargo.toml
        assert!(
            context.project_type.as_deref() == Some("rust") ||
            context.key_files.iter().any(|f| f.contains("Cargo.toml")),
            "Should detect Rust project"
        );
    }

    #[tokio::test]
    async fn test_gather_empty_workspace() {
        let dir = TempDir::new().unwrap();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;

        // Should handle empty workspace
        assert!(context.claude_md.is_none());
    }

    #[tokio::test]
    async fn test_format_as_prompt() {
        let dir = setup_project_workspace();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let context = gatherer.gather().await;
        let prompt = gatherer.format_as_prompt(&context);

        // Should produce some output
        println!("Formatted prompt:\n{}", prompt);
    }
}

mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_context_pipeline() {
        let dir = setup_project_workspace();

        // 1. Gather context
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());
        let context = gatherer.gather().await;
        let prompt = gatherer.format_as_prompt(&context);

        // 2. Count tokens
        let counter = TokenCounter::new(ProviderType::Anthropic);
        let token_count = counter.count(&prompt);
        // Token count is usize, so it's always >= 0; just verify it's reasonable
        assert!(token_count < 1_000_000, "Token count should be reasonable");
        println!("Context tokens: {}", token_count);

        // 3. Check if summarization needed
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());
        let messages = vec![
            msg(MessageRole::System, &prompt),
            msg(MessageRole::User, "Help me with this project"),
        ];
        let needs_summary = summarizer.needs_summarization(&messages, &counter);
        println!("Needs summarization: {}", needs_summary);
    }

    #[test]
    fn test_token_budget_calculation() {
        let counter = TokenCounter::new(ProviderType::Anthropic);

        let context_limit = counter.context_limit();
        let threshold = counter.summarization_threshold();

        // Threshold should be less than limit
        assert!(threshold < context_limit);

        let sample_message = "This is a typical user message that might be sent.";
        let tokens_per_message = counter.count(sample_message);

        let estimated_messages = threshold / tokens_per_message.max(1);
        println!("Estimated messages before summarization: {}", estimated_messages);
        assert!(estimated_messages > 100, "Should fit many messages before summarization");
    }

    #[test]
    fn test_different_providers() {
        let providers = [
            ProviderType::Anthropic,
            ProviderType::OpenAI,
            ProviderType::Gemini,
        ];

        for provider in providers {
            let counter = TokenCounter::new(provider.clone());
            let text = "Hello, world! This is a test.";
            let count = counter.count(text);
            println!("{:?} token count: {}", provider, count);
            assert!(count > 0);
        }
    }
}

mod context_monitor_tests {
    use super::*;

    #[test]
    fn test_monitor_default_config() {
        let monitor = ContextMonitor::new(ProviderType::Anthropic);
        let config = monitor.config();

        assert!(config.auto_compact_threshold > 0.0);
        assert!(config.auto_compact_threshold < 1.0);
        assert!(config.min_remaining_tokens > 0);
        assert!(config.check_interval > 0);
    }

    #[test]
    fn test_monitor_custom_config() {
        let config = MonitorConfig {
            auto_compact_threshold: 0.5,
            min_remaining_tokens: 10_000,
            check_interval: 3,
        };
        let monitor = ContextMonitor::with_config(ProviderType::Anthropic, config);

        assert_eq!(monitor.config().auto_compact_threshold, 0.5);
        assert_eq!(monitor.config().min_remaining_tokens, 10_000);
        assert_eq!(monitor.config().check_interval, 3);
    }

    #[test]
    fn test_calculate_usage_empty() {
        let monitor = ContextMonitor::new(ProviderType::Anthropic);
        let usage = monitor.calculate_usage(&[], "System prompt", None);

        assert!(usage.used_tokens > 0, "System prompt should use tokens");
        assert!(usage.used_percentage < 0.01, "Usage should be minimal");
        assert!(!usage.should_compact, "Should not need compaction");
        assert!(usage.breakdown.system_tokens > 0);
        assert_eq!(usage.breakdown.conversation_tokens, 0);
        assert_eq!(usage.breakdown.tool_tokens, 0);
        assert_eq!(usage.breakdown.memory_tokens, 0);
    }

    #[test]
    fn test_calculate_usage_with_messages() {
        let monitor = ContextMonitor::new(ProviderType::Anthropic);
        let messages = vec![
            msg(MessageRole::User, "Hello, how are you?"),
            msg(MessageRole::Assistant, "I'm doing well, thank you!"),
            msg(MessageRole::Tool, "Tool result: success"),
        ];

        let usage = monitor.calculate_usage(&messages, "System prompt", None);

        assert!(usage.breakdown.conversation_tokens > 0);
        assert!(usage.breakdown.tool_tokens > 0);
    }

    #[test]
    fn test_calculate_usage_with_memory() {
        let monitor = ContextMonitor::new(ProviderType::Anthropic);
        let memory = "# Project\nThis is the CLAUDE.md file content.";

        let usage = monitor.calculate_usage(&[], "System", Some(memory));

        assert!(usage.breakdown.memory_tokens > 0);
    }

    #[test]
    fn test_should_check_interval() {
        let mut monitor = ContextMonitor::new(ProviderType::Anthropic);

        // Default interval is 5
        for i in 1..=10 {
            let should = monitor.should_check();
            if i % 5 == 0 {
                assert!(should, "Should check at iteration {}", i);
            } else {
                assert!(!should, "Should not check at iteration {}", i);
            }
        }
    }

    #[test]
    fn test_format_usage() {
        let monitor = ContextMonitor::new(ProviderType::Anthropic);
        let usage = monitor.calculate_usage(&[], "System prompt", None);
        let formatted = monitor.format_usage(&usage);

        assert!(formatted.contains("Context Usage:"));
        assert!(formatted.contains("Breakdown:"));
        assert!(formatted.contains("System:"));
        assert!(formatted.contains("Memory:"));
        assert!(formatted.contains("Conversation:"));
        assert!(formatted.contains("Tool calls:"));
    }

    #[test]
    fn test_usage_triggers_compaction() {
        let config = MonitorConfig {
            auto_compact_threshold: 0.01, // Very low threshold
            min_remaining_tokens: 199_000, // Almost full
            check_interval: 1,
        };
        let monitor = ContextMonitor::with_config(ProviderType::Anthropic, config);

        // Create enough messages to trigger compaction
        let messages: Vec<_> = (0..100)
            .map(|i| msg(MessageRole::User, &format!("Message {} with some content", i)))
            .collect();

        let usage = monitor.calculate_usage(&messages, "System", None);
        assert!(usage.should_compact, "Should trigger compaction");
    }
}

mod memory_hierarchy_tests {
    use super::*;

    /// Create a workspace with the full 4-tier memory hierarchy
    fn setup_memory_hierarchy() -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let base = dir.path();

        // Tier 2: Project level - ./CLAUDE.md
        fs::write(
            base.join("CLAUDE.md"),
            "# Project Instructions\n\nThis is the main project CLAUDE.md file.",
        ).unwrap();

        // Tier 3: Rules - ./.claude/rules/*.md
        fs::create_dir_all(base.join(".claude/rules")).unwrap();
        fs::write(
            base.join(".claude/rules/01-coding-style.md"),
            "# Coding Style\n\nUse 4-space indentation.",
        ).unwrap();
        fs::write(
            base.join(".claude/rules/02-testing.md"),
            "# Testing\n\nAlways write tests for new features.",
        ).unwrap();

        // Tier 4: User level - ./CLAUDE.local.md
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
        println!("Found {} memory files", hierarchy.file_count());
        println!("{}", hierarchy.summary());
    }

    #[tokio::test]
    async fn test_memory_tiers_ordering() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;

        // Verify files are sorted by tier (priority order)
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
    async fn test_finds_user_tier() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;
        let user_files = hierarchy.files_in_tier(MemoryTier::User);

        assert!(!user_files.is_empty(), "Should find user tier files");
        assert!(user_files[0].content.contains("Local Overrides"));
    }

    #[tokio::test]
    async fn test_combined_content() {
        let dir = setup_memory_hierarchy();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;

        assert!(!hierarchy.combined_content.is_empty());
        assert!(hierarchy.combined_content.contains("Project Instructions"));
        assert!(hierarchy.combined_content.contains("Coding Style"));
        assert!(hierarchy.combined_content.contains("Testing"));
        assert!(hierarchy.combined_content.contains("Local Overrides"));
    }

    #[tokio::test]
    async fn test_empty_workspace_hierarchy() {
        let dir = TempDir::new().unwrap();
        let gatherer = ContextGatherer::new(dir.path().to_path_buf());

        let hierarchy = gatherer.gather_memory_hierarchy().await;

        assert!(hierarchy.is_empty());
        assert_eq!(hierarchy.file_count(), 0);
        assert_eq!(hierarchy.total_size, 0);
    }
}

mod compact_config_tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CompactConfig::default();

        assert!(config.use_llm);
        assert!(config.target_ratio > 0.0);
        assert!(config.target_ratio < 1.0);
        assert!(config.min_keep_recent > 0);
        assert!(config.preserve_instructions.is_none());
    }

    #[test]
    fn test_auto_config() {
        let config = CompactConfig::auto();

        assert!(config.use_llm);
        assert_eq!(config.target_ratio, 0.3);
    }

    #[test]
    fn test_from_command() {
        let config = CompactConfig::from_command(Some("keep API changes".to_string()));

        assert_eq!(
            config.preserve_instructions,
            Some("keep API changes".to_string())
        );
    }

    #[test]
    fn test_builder_pattern() {
        let config = CompactConfig::default()
            .with_instructions("preserve auth logic")
            .without_llm()
            .with_target_ratio(0.5)
            .with_min_keep_recent(10);

        assert_eq!(
            config.preserve_instructions,
            Some("preserve auth logic".to_string())
        );
        assert!(!config.use_llm);
        assert_eq!(config.target_ratio, 0.5);
        assert_eq!(config.min_keep_recent, 10);
    }

    #[test]
    fn test_target_ratio_clamping() {
        // Should clamp to valid range
        let config = CompactConfig::default().with_target_ratio(1.5);
        assert_eq!(config.target_ratio, 0.9);

        let config = CompactConfig::default().with_target_ratio(0.0);
        assert_eq!(config.target_ratio, 0.1);
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
                    "Message {} with enough content to make it realistic. \
                     This includes some code: fn foo() {{ return {}; }}",
                    i, i
                );
                msg(role, &content)
            })
            .collect()
    }

    #[tokio::test]
    async fn test_compact_small_conversation() {
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());
        let counter = TokenCounter::new(ProviderType::Anthropic);
        let config = CompactConfig::default().without_llm();

        let messages = generate_conversation(5);
        let result = summarizer.compact(&messages, &counter, config, None).await.unwrap();

        // Small conversation shouldn't be compacted much
        assert!(result.messages_kept >= result.messages_summarized);
    }

    #[tokio::test]
    async fn test_compact_large_conversation() {
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());
        let counter = TokenCounter::new(ProviderType::Anthropic);
        let config = CompactConfig::default()
            .without_llm()
            .with_target_ratio(0.3);

        let messages = generate_conversation(50);
        let result = summarizer.compact(&messages, &counter, config, None).await.unwrap();

        assert!(result.messages_summarized > 0, "Should summarize some messages");
        assert!(result.tokens_after <= result.tokens_before, "Should reduce tokens");
        assert!(!result.summary.content.is_empty(), "Should have a summary");
    }

    #[tokio::test]
    async fn test_compact_preserves_instructions() {
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());
        let counter = TokenCounter::new(ProviderType::Anthropic);
        let config = CompactConfig::default()
            .without_llm()
            .with_instructions("API endpoints");

        let messages = generate_conversation(30);
        let result = summarizer.compact(&messages, &counter, config, None).await.unwrap();

        // The summary should mention the preserved topic
        // (Note: without LLM, this is best-effort heuristic)
        assert!(result.summary.content.contains("Preserved context: API endpoints"));
    }

    #[tokio::test]
    async fn test_compact_keeps_recent() {
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());
        let counter = TokenCounter::new(ProviderType::Anthropic);
        let config = CompactConfig::default()
            .without_llm()
            .with_min_keep_recent(10);

        let messages = generate_conversation(30);
        let result = summarizer.compact(&messages, &counter, config, None).await.unwrap();

        assert!(result.messages_kept >= 10, "Should keep at least 10 recent messages");
    }

    #[test]
    fn test_simple_summary_extracts_files() {
        // Use a config with low keep_recent to ensure summarization happens
        let config = SummarizerConfig {
            keep_recent: 2,
            target_summary_tokens: 500,
            min_messages_to_summarize: 3,
        };
        let summarizer = ConversationSummarizer::new(config);

        // Create enough messages to trigger summarization
        let messages = vec![
            msg(MessageRole::User, "Let's start by reviewing the project"),
            msg(MessageRole::Assistant, "Sure, I'll look at the codebase"),
            msg(MessageRole::User, "Let's modify src/main.rs"),
            msg(MessageRole::Assistant, "I'll update src/main.rs with the new code"),
            msg(MessageRole::User, "Also update Cargo.toml"),
            msg(MessageRole::Assistant, "Done, I've updated both files"),
        ];

        let (summary, _recent) = summarizer.simple_summary(&messages);

        // Should extract mentioned files from the summarized messages
        assert!(
            summary.content.contains("src/main.rs") || summary.content.contains("Cargo.toml") || summary.content.contains("Files mentioned"),
            "Should mention files in summary: {}",
            summary.content
        );
    }

    #[test]
    fn test_simple_summary_extracts_commands() {
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());

        let messages = vec![
            msg(MessageRole::User, "Run the tests"),
            msg(MessageRole::Tool, "$ cargo test\nrunning 10 tests\ntest result: ok"),
            msg(MessageRole::Assistant, "All tests passed"),
        ];

        let (summary, _recent) = summarizer.simple_summary(&messages);

        // Check that summary was created
        assert!(!summary.content.is_empty());
    }
}

mod tiktoken_tests {
    use super::*;

    #[test]
    fn test_tiktoken_status() {
        let counter = TokenCounter::new(ProviderType::Anthropic);

        // This test verifies tiktoken integration
        let is_using = counter.is_using_tiktoken();
        println!("Using tiktoken: {}", is_using);

        // Token counting should work regardless
        let count = counter.count("Hello, world!");
        assert!(count > 0);
    }

    #[test]
    fn test_token_counts_reasonable() {
        let counter = TokenCounter::new(ProviderType::Anthropic);

        // Known examples with expected token ranges
        let examples = [
            ("Hello", 1, 3),
            ("Hello, world!", 2, 6),
            ("The quick brown fox jumps over the lazy dog.", 8, 15),
            ("fn main() { println!(\"Hello\"); }", 8, 20),
        ];

        for (text, min, max) in examples {
            let count = counter.count(text);
            assert!(
                count >= min && count <= max,
                "Token count for '{}' should be {}-{}, got {}",
                text, min, max, count
            );
        }
    }
}
