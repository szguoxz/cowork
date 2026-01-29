//! Conversation summarization for context compaction

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::prompt::builtin::reminders::CONVERSATION_SUMMARIZATION;
use crate::provider::{ChatMessage, GenAIProvider};

use super::{Message, MessageRole};

/// Configuration for context compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactConfig {
    /// Custom instructions for what to preserve during compaction
    pub preserve_instructions: Option<String>,
    /// Whether to use the LLM for summarization (vs simple heuristics)
    pub use_llm: bool,
}

impl Default for CompactConfig {
    fn default() -> Self {
        Self {
            preserve_instructions: None,
            use_llm: true,
        }
    }
}

impl CompactConfig {
    /// Create a config for auto-compaction
    pub fn auto() -> Self {
        Self::default()
    }

    /// Create a config from a user command with optional instructions
    pub fn from_command(instructions: Option<String>) -> Self {
        Self {
            preserve_instructions: instructions,
            ..Default::default()
        }
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactResult {
    /// The summary message that replaces the conversation
    pub summary: Message,
    /// Character count before compaction
    pub chars_before: usize,
    /// Character count after compaction
    pub chars_after: usize,
    /// Number of messages that were summarized
    pub messages_summarized: usize,
}

/// Compact conversation history into a summary
pub async fn compact(
    messages: &[Message],
    config: CompactConfig,
    provider: Option<&GenAIProvider>,
) -> Result<CompactResult> {
    let chars_before: usize = messages.iter().map(|m| m.content.len()).sum();

    if messages.is_empty() {
        return Ok(CompactResult {
            summary: Message::new(MessageRole::User, "<summary>No prior context.</summary>"),
            chars_before: 0,
            chars_after: 0,
            messages_summarized: 0,
        });
    }

    let summary = match provider {
        Some(p) if config.use_llm => generate_llm_summary(messages, p, &config).await?,
        _ => generate_simple_summary(messages, &config),
    };

    let chars_after = summary.content.len();

    Ok(CompactResult {
        summary,
        chars_before,
        chars_after,
        messages_summarized: messages.len(),
    })
}

/// Generate LLM-powered summary
async fn generate_llm_summary(
    messages: &[Message],
    provider: &GenAIProvider,
    config: &CompactConfig,
) -> Result<Message> {
    let conversation_text = format_for_summarization(messages);

    let mut summary_prompt = CONVERSATION_SUMMARIZATION.to_string();

    if let Some(ref instructions) = config.preserve_instructions {
        summary_prompt = format!(
            "IMPORTANT: Pay special attention to and preserve details about: {}\n\n{}",
            instructions, summary_prompt
        );
    }

    let llm_messages = vec![ChatMessage::user(format!(
        "Here is the conversation history:\n\n{}\n\n{}",
        conversation_text, summary_prompt
    ))];

    let response = provider.chat(llm_messages, None).await?;

    let content = response.content.unwrap_or_else(|| {
        "<summary>Previous conversation involved various development tasks.</summary>".to_string()
    });

    Ok(Message::new(MessageRole::User, content))
}

/// Generate simple heuristic-based summary (fallback)
fn generate_simple_summary(messages: &[Message], config: &CompactConfig) -> Message {
    let mut files = Vec::new();
    let mut topics = Vec::new();

    for msg in messages {
        // Extract file paths
        for word in msg.content.split_whitespace() {
            if is_file_path(word) {
                let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-');
                if !files.contains(&clean.to_string()) && files.len() < 15 {
                    files.push(clean.to_string());
                }
            }
        }

        // Extract topics from user messages
        if matches!(msg.role, MessageRole::User) && msg.content.len() > 20 && topics.len() < 5 {
            let topic: String = msg.content.chars().take(80).collect();
            topics.push(topic);
        }
    }

    let mut summary = format!("Summary of {} messages:\n", messages.len());

    if let Some(ref instructions) = config.preserve_instructions {
        summary.push_str(&format!("\n## Preserved Context\n{}\n", instructions));
    }

    if !topics.is_empty() {
        summary.push_str("\n## Topics\n");
        for (i, topic) in topics.iter().enumerate() {
            summary.push_str(&format!("{}. {}...\n", i + 1, topic));
        }
    }

    if !files.is_empty() {
        summary.push_str("\n## Files\n");
        for file in &files {
            summary.push_str(&format!("- {}\n", file));
        }
    }

    Message::new(MessageRole::User, format!("<summary>\n{}</summary>", summary))
}

fn is_file_path(word: &str) -> bool {
    (word.contains('/') || word.contains('.'))
        && (word.ends_with(".rs")
            || word.ends_with(".ts")
            || word.ends_with(".tsx")
            || word.ends_with(".js")
            || word.ends_with(".jsx")
            || word.ends_with(".py")
            || word.ends_with(".json")
            || word.ends_with(".toml")
            || word.ends_with(".md"))
}

fn format_for_summarization(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::User => "Human",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => "System",
                MessageRole::Tool => "Tool",
            };
            format!("{}: {}", role, m.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
