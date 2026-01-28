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

/// Default summary prompt matching Anthropic SDK
pub const DEFAULT_SUMMARY_PROMPT: &str = r#"You have been working on the task described above but have not yet completed it. Write a continuation summary that will allow you (or another instance of yourself) to resume work efficiently in a future context window where the conversation history will be replaced with this summary. Your summary should be structured, concise, and actionable. Include:

1. Task Overview
   - The user's core request and success criteria
   - Any clarifications or constraints they specified

2. Current State
   - What has been completed so far
   - Files created, modified, or analyzed (with paths if relevant)
   - Key outputs or artifacts produced

3. Important Discoveries
   - Technical constraints or requirements uncovered
   - Decisions made and their rationale
   - Errors encountered and how they were resolved
   - What approaches were tried that didn't work (and why)

4. Next Steps
   - Specific actions needed to complete the task
   - Any blockers or open questions to resolve
   - Priority order if multiple steps remain

5. Context to Preserve
   - User preferences or style requirements
   - Domain-specific details that aren't obvious
   - Any promises made to the user

Be concise but complete—err on the side of including information that would prevent duplicate work or repeated mistakes. Write in a way that enables immediate resumption of the task.

Wrap your summary in <summary></summary> tags."#;

/// Configuration for context compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactConfig {
    /// Custom instructions for what to preserve during compaction
    /// e.g., "/compact keep API changes" -> preserve_instructions = "keep API changes"
    pub preserve_instructions: Option<String>,
    /// Whether to use the LLM for summarization (vs simple heuristics)
    pub use_llm: bool,
    /// Custom summary prompt (if None, uses DEFAULT_SUMMARY_PROMPT)
    pub summary_prompt: Option<String>,
}

impl Default for CompactConfig {
    fn default() -> Self {
        Self {
            preserve_instructions: None,
            use_llm: true,
            summary_prompt: None, // Uses DEFAULT_SUMMARY_PROMPT
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

    /// Set custom summary prompt
    pub fn with_summary_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.summary_prompt = Some(prompt.into());
        self
    }

    /// Get the summary prompt to use
    pub fn get_summary_prompt(&self) -> &str {
        self.summary_prompt.as_deref().unwrap_or(DEFAULT_SUMMARY_PROMPT)
    }
}

/// Result of a compaction operation
///
/// Following Anthropic SDK approach: after compaction, the entire conversation
/// is replaced with a single user message containing the summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactResult {
    /// The summary message (role: User) that replaces the entire conversation
    pub summary: Message,
    /// Token count before compaction
    pub tokens_before: usize,
    /// Token count after compaction
    pub tokens_after: usize,
    /// Number of messages that were summarized
    pub messages_summarized: usize,
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
        provider: &impl LlmProvider,
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
                role: crate::provider::ChatRole::System,
                content: crate::provider::MessageContent::Text(
                    "You are a helpful assistant that summarizes conversations accurately and concisely.".to_string()
                ),
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
                "<summary>\n{}\n</summary>",
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
            if (msg.content.contains("```") || msg.content.starts_with("$ "))
                && let Some(cmd) = msg.content.lines().find(|l| l.starts_with("$ ")) {
                    commands_run.push(cmd.trim_start_matches("$ ").to_string());
                }

            // Extract topic from user messages
            if matches!(msg.role, MessageRole::User) && msg.content.len() > 10 {
                let topic: String = msg.content.chars().take(100).collect();
                topics.push(topic);
            }
        }

        let mut summary = format!(
            "Summary of {} earlier messages:\n",
            to_summarize.len()
        );

        if !topics.is_empty() {
            summary.push_str("\n## Topics Discussed\n");
            for (i, topic) in topics.iter().take(5).enumerate() {
                summary.push_str(&format!("{}. {}...\n", i + 1, topic.chars().take(80).collect::<String>()));
            }
        }

        if !files_mentioned.is_empty() {
            summary.push_str("\n## Files Mentioned\n");
            for file in files_mentioned.iter().take(10) {
                summary.push_str(&format!("- {}\n", file));
            }
        }

        if !commands_run.is_empty() {
            summary.push_str("\n## Commands Executed\n");
            for cmd in commands_run.iter().take(5) {
                summary.push_str(&format!("- {}\n", cmd));
            }
        }

        let summary_message = Message {
            role: MessageRole::System,
            content: format!("<summary>\n{}</summary>", summary),
            timestamp: chrono::Utc::now(),
        };

        (summary_message, to_keep.to_vec())
    }

    /// Compact the conversation using the provided configuration
    ///
    /// Following Anthropic SDK approach:
    /// 1. Append the summary prompt as a user message to the conversation
    /// 2. Call LLM to generate summary wrapped in <summary></summary> tags
    /// 3. Replace entire conversation with a single user message containing the summary
    ///
    /// This is the main entry point for context compaction, supporting both
    /// auto-compact and manual `/compact` command scenarios.
    pub async fn compact<P: LlmProvider>(
        &self,
        messages: &[Message],
        counter: &TokenCounter,
        config: CompactConfig,
        provider: Option<&P>,
    ) -> Result<CompactResult> {
        let tokens_before = counter.count_messages(messages);

        if messages.is_empty() {
            // Nothing to compact
            return Ok(CompactResult {
                summary: Message {
                    role: MessageRole::User,
                    content: "<summary>No prior context.</summary>".to_string(),
                    timestamp: chrono::Utc::now(),
                },
                tokens_before: 0,
                tokens_after: 0,
                messages_summarized: 0,
            });
        }

        // Generate summary - following Anthropic SDK approach
        let summary = match provider {
            Some(p) if config.use_llm => {
                self.generate_llm_compact_summary(messages, p, &config).await?
            }
            _ => self.generate_simple_compact_summary(messages, &config),
        };

        let tokens_after = counter.count(&summary.content);

        Ok(CompactResult {
            summary,
            tokens_before,
            tokens_after,
            messages_summarized: messages.len(),
        })
    }

    /// Generate an LLM-powered summary for compaction
    ///
    /// Following Anthropic SDK approach:
    /// 1. Format the conversation history
    /// 2. Append the summary prompt (DEFAULT_SUMMARY_PROMPT or custom)
    /// 3. Call LLM to generate summary wrapped in <summary></summary> tags
    /// 4. Return as a USER message (to be the new single message in conversation)
    async fn generate_llm_compact_summary(
        &self,
        messages: &[Message],
        provider: &impl LlmProvider,
        config: &CompactConfig,
    ) -> Result<Message> {
        let conversation_text = format_for_summarization(messages);

        // Build the messages for the summarization request
        // Following Anthropic SDK: append the summary prompt to the conversation
        let mut summary_prompt = config.get_summary_prompt().to_string();

        // Add custom preservation instructions if provided
        if let Some(ref instructions) = config.preserve_instructions {
            summary_prompt = format!(
                "IMPORTANT: Pay special attention to and preserve details about: {}\n\n{}",
                instructions,
                summary_prompt
            );
        }

        // Create the request with conversation + summary prompt
        let request = LlmRequest::new(vec![
            LlmMessage::user(format!(
                "Here is the conversation history:\n\n{}\n\n{}",
                conversation_text,
                summary_prompt
            )),
        ])
        .with_max_tokens(self.config.target_summary_tokens as u32);

        let response = provider.complete(request).await?;

        let summary_content = response.content.unwrap_or_else(|| {
            "<summary>Previous conversation involved various development tasks.</summary>".to_string()
        });

        // The summary becomes a USER message (following Anthropic SDK)
        Ok(Message {
            role: MessageRole::User,
            content: summary_content,
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
            if msg.content.starts_with("$ ")
                && let Some(cmd) = msg.content.lines().next() {
                    commands_run.push(cmd.trim_start_matches("$ ").to_string());
                }

            // Look for key action indicators
            if matches!(msg.role, MessageRole::Assistant) {
                if (msg.content.contains("created") || msg.content.contains("Created"))
                    && let Some(action) = extract_action_summary(&msg.content, "created") {
                        key_actions.push(action);
                    }
                if (msg.content.contains("modified") || msg.content.contains("Modified"))
                    && let Some(action) = extract_action_summary(&msg.content, "modified") {
                        key_actions.push(action);
                    }
                if (msg.content.contains("fixed") || msg.content.contains("Fixed"))
                    && let Some(action) = extract_action_summary(&msg.content, "fixed") {
                        key_actions.push(action);
                    }
            }

            // Look for decisions/conclusions
            if matches!(msg.role, MessageRole::User) && msg.content.len() > 20 {
                let topic: String = msg.content.chars().take(80).collect();
                decisions.push(topic);
            }
        }

        // Build summary using Claude Code format
        let mut summary = format!(
            "Summary of {} messages compacted:\n",
            messages.len()
        );

        // Add custom preservation note if specified
        if let Some(ref instructions) = config.preserve_instructions {
            summary.push_str(&format!("\n## Preserved Context\n{}\n", instructions));
        }

        if !decisions.is_empty() {
            summary.push_str("\n## Topics Discussed\n");
            for (i, topic) in decisions.iter().take(5).enumerate() {
                summary.push_str(&format!("{}. {}...\n", i + 1, topic));
            }
        }

        if !files_mentioned.is_empty() {
            summary.push_str("\n## Files Worked On\n");
            for file in files_mentioned.iter().take(15) {
                summary.push_str(&format!("- {}\n", file));
            }
        }

        if !key_actions.is_empty() {
            summary.push_str("\n## Key Actions\n");
            for action in key_actions.iter().take(10) {
                summary.push_str(&format!("- {}\n", action));
            }
        }

        if !commands_run.is_empty() {
            summary.push_str("\n## Commands Executed\n");
            for cmd in commands_run.iter().take(5) {
                summary.push_str(&format!("- {}\n", cmd));
            }
        }

        // The summary becomes a USER message (following Anthropic SDK)
        Message {
            role: MessageRole::User,
            content: format!("<summary>\n{}</summary>", summary),
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
