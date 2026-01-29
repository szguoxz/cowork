//! Conversation summarization for context compaction

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::prompt::builtin::reminders::CONVERSATION_SUMMARIZATION;
use crate::provider::{ChatMessage, GenAIProvider};

use super::{Message, MessageRole};

/// Configuration for context compaction
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactConfig {
    /// Custom instructions for what to preserve during compaction
    pub preserve_instructions: Option<String>,
}

impl CompactConfig {
    /// Create a config for auto-compaction
    pub fn auto() -> Self {
        Self::default()
    }

    /// Create a config from a user command with optional instructions
    pub fn from_command(instructions: Option<String>) -> Self {
        Self { preserve_instructions: instructions }
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

/// Compact conversation history into a summary using LLM
pub async fn compact(
    messages: &[Message],
    config: CompactConfig,
    provider: &GenAIProvider,
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

    let summary = Message::new(MessageRole::User, content);
    let chars_after = summary.content.len();

    Ok(CompactResult {
        summary,
        chars_before,
        chars_after,
        messages_summarized: messages.len(),
    })
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
