//! Conversation summarization for context management
//!
//! Automatically summarizes older messages when approaching context limits.

use crate::error::Result;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

use super::{Message, MessageRole};
use super::tokens::TokenCounter;

/// Configuration for the summarizer
#[derive(Debug, Clone)]
pub struct SummarizerConfig {
    /// Number of recent messages to always keep unmodified
    pub keep_recent: usize,
    /// Target token count for summaries
    pub target_summary_tokens: usize,
    /// Minimum messages before attempting summarization
    pub min_messages_to_summarize: usize,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            keep_recent: 10,
            target_summary_tokens: 2000,
            min_messages_to_summarize: 20,
        }
    }
}

/// Summarizes conversation history
pub struct ConversationSummarizer {
    config: SummarizerConfig,
}

impl ConversationSummarizer {
    pub fn new(config: SummarizerConfig) -> Self {
        Self { config }
    }

    /// Check if summarization is needed based on token count
    pub fn needs_summarization(&self, messages: &[Message], counter: &TokenCounter) -> bool {
        if messages.len() < self.config.min_messages_to_summarize {
            return false;
        }

        let current_tokens = counter.count_messages(messages);
        counter.should_summarize(current_tokens)
    }

    /// Summarize older messages, keeping recent ones intact
    ///
    /// Returns: (summary_message, messages_to_keep)
    pub async fn summarize(
        &self,
        messages: &[Message],
        provider: &dyn LlmProvider,
    ) -> Result<(Message, Vec<Message>)> {
        if messages.len() <= self.config.keep_recent {
            // Nothing to summarize
            return Ok((
                Message {
                    role: MessageRole::System,
                    content: "No prior context.".to_string(),
                    timestamp: chrono::Utc::now(),
                },
                messages.to_vec(),
            ));
        }

        let split_point = messages.len() - self.config.keep_recent;
        let to_summarize = &messages[..split_point];
        let to_keep = &messages[split_point..];

        // Build summarization prompt
        let conversation_text = format_for_summarization(to_summarize);

        let summarization_prompt = format!(
            "Please provide a concise summary of the following conversation. \
             Focus on: key decisions made, files modified, commands executed, \
             and important context that should be remembered. \
             Keep the summary under {} tokens.\n\n\
             Conversation to summarize:\n{}",
            self.config.target_summary_tokens,
            conversation_text
        );

        let request = LlmRequest::new(vec![
            LlmMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant that summarizes conversations accurately and concisely.".to_string(),
            },
            LlmMessage {
                role: "user".to_string(),
                content: summarization_prompt,
            },
        ])
        .with_max_tokens(self.config.target_summary_tokens as u32);

        let response = provider.complete(request).await?;

        let summary_content = response.content.unwrap_or_else(|| {
            "Previous conversation involved various development tasks.".to_string()
        });

        let summary_message = Message {
            role: MessageRole::System,
            content: format!(
                "=== Summary of earlier conversation ({} messages) ===\n{}\n=== End of summary ===",
                to_summarize.len(),
                summary_content
            ),
            timestamp: chrono::Utc::now(),
        };

        Ok((summary_message, to_keep.to_vec()))
    }

    /// Create a simple summary without using the LLM
    /// Useful as a fallback or for offline operation
    pub fn simple_summary(&self, messages: &[Message]) -> (Message, Vec<Message>) {
        if messages.len() <= self.config.keep_recent {
            return (
                Message {
                    role: MessageRole::System,
                    content: "No prior context.".to_string(),
                    timestamp: chrono::Utc::now(),
                },
                messages.to_vec(),
            );
        }

        let split_point = messages.len() - self.config.keep_recent;
        let to_summarize = &messages[..split_point];
        let to_keep = &messages[split_point..];

        // Extract key information
        let mut files_mentioned = Vec::new();
        let mut commands_run = Vec::new();
        let mut topics = Vec::new();

        for msg in to_summarize {
            // Look for file paths
            for word in msg.content.split_whitespace() {
                if word.contains('/') || word.contains('.') {
                    if word.ends_with(".rs")
                        || word.ends_with(".ts")
                        || word.ends_with(".js")
                        || word.ends_with(".py")
                        || word.ends_with(".json")
                        || word.ends_with(".toml")
                        || word.ends_with(".md")
                    {
                        if !files_mentioned.contains(&word.to_string()) {
                            files_mentioned.push(word.to_string());
                        }
                    }
                }
            }

            // Look for commands
            if msg.content.contains("```") || msg.content.starts_with("$ ") {
                if let Some(cmd) = msg.content.lines().find(|l| l.starts_with("$ ")) {
                    commands_run.push(cmd.trim_start_matches("$ ").to_string());
                }
            }

            // Extract topic from user messages
            if msg.role == MessageRole::User && msg.content.len() > 10 {
                let topic: String = msg.content.chars().take(100).collect();
                topics.push(topic);
            }
        }

        let mut summary = format!(
            "=== Summary of earlier conversation ({} messages) ===\n",
            to_summarize.len()
        );

        if !topics.is_empty() {
            summary.push_str("\nTopics discussed:\n");
            for (i, topic) in topics.iter().take(5).enumerate() {
                summary.push_str(&format!("  {}. {}...\n", i + 1, topic.chars().take(80).collect::<String>()));
            }
        }

        if !files_mentioned.is_empty() {
            summary.push_str("\nFiles mentioned:\n");
            for file in files_mentioned.iter().take(10) {
                summary.push_str(&format!("  - {}\n", file));
            }
        }

        if !commands_run.is_empty() {
            summary.push_str("\nCommands executed:\n");
            for cmd in commands_run.iter().take(5) {
                summary.push_str(&format!("  - {}\n", cmd));
            }
        }

        summary.push_str("=== End of summary ===");

        let summary_message = Message {
            role: MessageRole::System,
            content: summary,
            timestamp: chrono::Utc::now(),
        };

        (summary_message, to_keep.to_vec())
    }
}

/// Format messages for summarization
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
