//! Conversation summarization for context compaction

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::prompt::builtin::reminders::CONVERSATION_SUMMARIZATION;
use crate::provider::{message_text_content, ChatMessage, ChatRole, GenAIProvider};

/// Result of a compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactResult {
    /// The summary text that replaces the conversation
    pub summary: String,
    /// Character count before compaction
    pub chars_before: usize,
    /// Character count after compaction
    pub chars_after: usize,
    /// Number of messages that were summarized
    pub messages_summarized: usize,
}

/// Compact conversation history into a summary using LLM
///
/// `preserve_instructions` - optional hints about what to preserve (e.g. "API changes")
pub async fn compact(
    messages: &[ChatMessage],
    preserve_instructions: Option<&str>,
    provider: &GenAIProvider,
) -> Result<CompactResult> {
    let chars_before: usize = messages.iter().map(|m| message_text_content(m).len()).sum();

    if messages.is_empty() {
        return Ok(CompactResult {
            summary: "<summary>No prior context.</summary>".to_string(),
            chars_before: 0,
            chars_after: 0,
            messages_summarized: 0,
        });
    }

    let conversation_text = format_for_summarization(messages);

    let summary_prompt = match preserve_instructions {
        Some(instructions) => format!(
            "IMPORTANT: Pay special attention to and preserve details about: {}\n\n{}",
            instructions, CONVERSATION_SUMMARIZATION
        ),
        None => CONVERSATION_SUMMARIZATION.to_string(),
    };

    let llm_messages = vec![ChatMessage::user(format!(
        "Here is the conversation history:\n\n{}\n\n{}",
        conversation_text, summary_prompt
    ))];

    let response = provider.chat(llm_messages, None).await?;

    let summary = response.content.unwrap_or_else(|| {
        "<summary>Previous conversation involved various development tasks.</summary>".to_string()
    });
    let chars_after = summary.len();

    Ok(CompactResult {
        summary,
        chars_before,
        chars_after,
        messages_summarized: messages.len(),
    })
}

fn format_for_summarization(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                ChatRole::User => "Human",
                ChatRole::Assistant => "Assistant",
                ChatRole::System => "System",
                ChatRole::Tool => "Tool",
            };
            let content = message_text_content(m);
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
