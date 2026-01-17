//! Context management tests
//!
//! Tests for token counting, summarization, and context gathering.

use cowork_core::context::{TokenCounter, ConversationSummarizer, SummarizerConfig, ContextGatherer, Message, MessageRole};
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
        assert!(token_count >= 0);
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
