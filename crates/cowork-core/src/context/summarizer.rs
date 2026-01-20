//! Conversation summarization for context management
//!
//! Automatically summarizes older messages when approaching context limits.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

use super::tokens::TokenCounter;
use super::{Message, MessageRole};

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

/// Configuration for context compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactConfig {
    /// Custom instructions for what to preserve during compaction
    /// e.g., "/compact keep API changes" -> preserve_instructions = "keep API changes"
    pub preserve_instructions: Option<String>,
    /// Whether to use the LLM for summarization (vs simple heuristics)
    pub use_llm: bool,
    /// Target ratio of tokens to keep after compaction (0.0 - 1.0)
    /// e.g., 0.3 means keep approximately 30% of tokens
    pub target_ratio: f64,
    /// Minimum number of recent messages to always keep intact
    pub min_keep_recent: usize,
}

impl Default for CompactConfig {
    fn default() -> Self {
        Self {
            preserve_instructions: None,
            use_llm: true,
            target_ratio: 0.3,
            min_keep_recent: 5,
        }
    }
}

impl CompactConfig {
    /// Create a config for auto-compaction (uses defaults)
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

    /// Set custom preservation instructions
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.preserve_instructions = Some(instructions.into());
        self
    }

    /// Disable LLM and use simple heuristics
    pub fn without_llm(mut self) -> Self {
        self.use_llm = false;
        self
    }

    /// Set the target ratio
    pub fn with_target_ratio(mut self, ratio: f64) -> Self {
        self.target_ratio = ratio.clamp(0.1, 0.9);
        self
    }

    /// Set minimum recent messages to keep
    pub fn with_min_keep_recent(mut self, count: usize) -> Self {
        self.min_keep_recent = count;
        self
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactResult {
    /// The summary message to prepend to the conversation
    pub summary: Message,
    /// Messages that were kept (not summarized)
    pub kept_messages: Vec<Message>,
    /// Token count before compaction
    pub tokens_before: usize,
    /// Token count after compaction
    pub tokens_after: usize,
    /// Number of messages that were summarized
    pub messages_summarized: usize,
    /// Number of messages kept
    pub messages_kept: usize,
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
                tool_calls: None,
                tool_call_id: None,
            },
            LlmMessage::user(summarization_prompt),
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
                if (word.contains('/') || word.contains('.'))
                    && (word.ends_with(".rs")
                        || word.ends_with(".ts")
                        || word.ends_with(".js")
                        || word.ends_with(".py")
                        || word.ends_with(".json")
                        || word.ends_with(".toml")
                        || word.ends_with(".md"))
                    && !files_mentioned.contains(&word.to_string())
                {
                    files_mentioned.push(word.to_string());
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

    /// Compact the conversation using the provided configuration
    ///
    /// This is the main entry point for context compaction, supporting both
    /// auto-compact and manual `/compact` command scenarios.
    pub async fn compact(
        &self,
        messages: &[Message],
        counter: &TokenCounter,
        config: CompactConfig,
        provider: Option<&dyn LlmProvider>,
    ) -> Result<CompactResult> {
        let tokens_before = counter.count_messages(messages);

        // Determine how many messages to keep based on target ratio
        let target_tokens = (tokens_before as f64 * config.target_ratio) as usize;

        // Calculate split point, ensuring we keep at least min_keep_recent
        let split_point = self.calculate_split_point(messages, counter, target_tokens, config.min_keep_recent);

        if split_point == 0 {
            // Nothing to compact - return all messages
            return Ok(CompactResult {
                summary: Message {
                    role: MessageRole::System,
                    content: "No prior context to summarize.".to_string(),
                    timestamp: chrono::Utc::now(),
                },
                kept_messages: messages.to_vec(),
                tokens_before,
                tokens_after: tokens_before,
                messages_summarized: 0,
                messages_kept: messages.len(),
            });
        }

        let to_summarize = &messages[..split_point];
        let to_keep = &messages[split_point..];

        // Generate summary
        let summary = match provider {
            Some(p) if config.use_llm => {
                self.generate_llm_compact_summary(to_summarize, p, &config).await?
            }
            _ => self.generate_simple_compact_summary(to_summarize, &config),
        };

        let tokens_after = counter.count(&summary.content) + counter.count_messages(to_keep);

        Ok(CompactResult {
            summary,
            kept_messages: to_keep.to_vec(),
            tokens_before,
            tokens_after,
            messages_summarized: to_summarize.len(),
            messages_kept: to_keep.len(),
        })
    }

    /// Calculate the split point for compaction
    fn calculate_split_point(
        &self,
        messages: &[Message],
        counter: &TokenCounter,
        target_tokens: usize,
        min_keep_recent: usize,
    ) -> usize {
        // Always keep at least min_keep_recent messages
        if messages.len() <= min_keep_recent {
            return 0;
        }

        // Start from the end and work backwards, counting tokens
        let mut kept_tokens = 0;
        let mut keep_count = 0;

        for msg in messages.iter().rev() {
            let msg_tokens = counter.count(&msg.content) + 4; // +4 for message overhead

            if kept_tokens + msg_tokens > target_tokens && keep_count >= min_keep_recent {
                break;
            }

            kept_tokens += msg_tokens;
            keep_count += 1;
        }

        // Return the split point
        messages.len().saturating_sub(keep_count)
    }

    /// Generate an LLM-powered summary for compaction
    async fn generate_llm_compact_summary(
        &self,
        messages: &[Message],
        provider: &dyn LlmProvider,
        config: &CompactConfig,
    ) -> Result<Message> {
        let conversation_text = format_for_summarization(messages);

        let mut prompt = "Please provide a concise summary of the following conversation. \
             Focus on: key decisions made, files modified, code changes, commands executed, \
             and important context that should be remembered for continuing the work.\n\n"
            .to_string();

        // Add custom preservation instructions if provided
        if let Some(ref instructions) = config.preserve_instructions {
            prompt.push_str(&format!(
                "IMPORTANT: Pay special attention to and preserve details about: {}\n\n",
                instructions
            ));
        }

        prompt.push_str(&format!(
            "Keep the summary under {} tokens.\n\n\
             Conversation to summarize:\n{}",
            self.config.target_summary_tokens,
            conversation_text
        ));

        let request = LlmRequest::new(vec![
            LlmMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant that summarizes conversations accurately and concisely. \
                         Focus on preserving actionable context needed to continue the work.".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            LlmMessage::user(prompt),
        ])
        .with_max_tokens(self.config.target_summary_tokens as u32);

        let response = provider.complete(request).await?;

        let summary_content = response.content.unwrap_or_else(|| {
            "Previous conversation involved various development tasks.".to_string()
        });

        Ok(Message {
            role: MessageRole::System,
            content: format!(
                "=== Conversation Summary ({} messages compacted) ===\n{}\n=== End of Summary ===",
                messages.len(),
                summary_content
            ),
            timestamp: chrono::Utc::now(),
        })
    }

    /// Generate a simple heuristic-based summary for compaction
    fn generate_simple_compact_summary(&self, messages: &[Message], config: &CompactConfig) -> Message {
        let mut files_mentioned = Vec::new();
        let mut commands_run = Vec::new();
        let mut key_actions = Vec::new();
        let mut decisions = Vec::new();

        for msg in messages {
            // Extract file paths
            for word in msg.content.split_whitespace() {
                if (word.contains('/') || word.contains('.'))
                    && (word.ends_with(".rs")
                        || word.ends_with(".ts")
                        || word.ends_with(".js")
                        || word.ends_with(".py")
                        || word.ends_with(".json")
                        || word.ends_with(".toml")
                        || word.ends_with(".md")
                        || word.ends_with(".tsx")
                        || word.ends_with(".jsx"))
                {
                    let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-');
                    if !files_mentioned.contains(&clean_word.to_string()) {
                        files_mentioned.push(clean_word.to_string());
                    }
                }
            }

            // Extract commands
            if msg.content.starts_with("$ ") {
                if let Some(cmd) = msg.content.lines().next() {
                    commands_run.push(cmd.trim_start_matches("$ ").to_string());
                }
            }

            // Look for key action indicators
            if msg.role == MessageRole::Assistant {
                if msg.content.contains("created") || msg.content.contains("Created") {
                    if let Some(action) = extract_action_summary(&msg.content, "created") {
                        key_actions.push(action);
                    }
                }
                if msg.content.contains("modified") || msg.content.contains("Modified") {
                    if let Some(action) = extract_action_summary(&msg.content, "modified") {
                        key_actions.push(action);
                    }
                }
                if msg.content.contains("fixed") || msg.content.contains("Fixed") {
                    if let Some(action) = extract_action_summary(&msg.content, "fixed") {
                        key_actions.push(action);
                    }
                }
            }

            // Look for decisions/conclusions
            if msg.role == MessageRole::User && msg.content.len() > 20 {
                let topic: String = msg.content.chars().take(80).collect();
                decisions.push(topic);
            }
        }

        // Build summary
        let mut summary = format!(
            "=== Conversation Summary ({} messages compacted) ===\n",
            messages.len()
        );

        // Add custom preservation note if specified
        if let Some(ref instructions) = config.preserve_instructions {
            summary.push_str(&format!("\n[Preserved context: {}]\n", instructions));
        }

        if !decisions.is_empty() {
            summary.push_str("\nTopics discussed:\n");
            for (i, topic) in decisions.iter().take(5).enumerate() {
                summary.push_str(&format!("  {}. {}...\n", i + 1, topic));
            }
        }

        if !files_mentioned.is_empty() {
            summary.push_str("\nFiles worked on:\n");
            for file in files_mentioned.iter().take(15) {
                summary.push_str(&format!("  - {}\n", file));
            }
        }

        if !key_actions.is_empty() {
            summary.push_str("\nKey actions:\n");
            for action in key_actions.iter().take(10) {
                summary.push_str(&format!("  - {}\n", action));
            }
        }

        if !commands_run.is_empty() {
            summary.push_str("\nCommands executed:\n");
            for cmd in commands_run.iter().take(5) {
                summary.push_str(&format!("  - {}\n", cmd));
            }
        }

        summary.push_str("=== End of Summary ===");

        Message {
            role: MessageRole::System,
            content: summary,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Extract a brief action summary from content
fn extract_action_summary(content: &str, keyword: &str) -> Option<String> {
    for line in content.lines() {
        let lower = line.to_lowercase();
        if lower.contains(keyword) {
            let trimmed: String = line.chars().take(100).collect();
            return Some(trimmed);
        }
    }
    None
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
