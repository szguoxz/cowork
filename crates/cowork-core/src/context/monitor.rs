//! Context monitoring for tracking token usage and triggering auto-compact
//!
//! Tracks context usage across conversations and provides signals
//! for when compaction should be triggered.

use serde::{Deserialize, Serialize};

use super::tokens::TokenCounter;
use super::Message;

/// Configuration for the context monitor
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Threshold percentage at which auto-compact should trigger (0.0 - 1.0)
    pub auto_compact_threshold: f64,
    /// Minimum tokens remaining before forcing compaction
    pub min_remaining_tokens: usize,
    /// Check interval (every N iterations of the agentic loop)
    pub check_interval: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            auto_compact_threshold: 0.75,
            min_remaining_tokens: 20_000,
            check_interval: 5,
        }
    }
}

/// Current context usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsage {
    /// Total tokens currently used
    pub used_tokens: usize,
    /// Maximum tokens available for the provider
    pub limit_tokens: usize,
    /// Percentage of context used (0.0 - 1.0)
    pub used_percentage: f64,
    /// Remaining tokens available
    pub remaining_tokens: usize,
    /// Whether context should be compacted
    pub should_compact: bool,

    /// Breakdown of token usage
    pub breakdown: ContextBreakdown,
}

/// Breakdown of token usage by category
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextBreakdown {
    /// Tokens used by system prompt and instructions
    pub system_tokens: usize,
    /// Tokens used by conversation history
    pub conversation_tokens: usize,
    /// Tokens used by tool calls and results
    pub tool_tokens: usize,
    /// Tokens used by memory files (CLAUDE.md, etc.)
    pub memory_tokens: usize,
    /// Input tokens (system + memory + user messages + tool results)
    pub input_tokens: usize,
    /// Output tokens (all assistant messages accumulated)
    pub output_tokens: usize,
    /// Output tokens for the current response only
    pub current_output_tokens: usize,
}

/// Context monitor that tracks token usage and signals when compaction is needed
pub struct ContextMonitor {
    counter: TokenCounter,
    config: MonitorConfig,
    iteration_count: usize,
}

impl ContextMonitor {
    /// Create a new context monitor for a provider
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            counter: TokenCounter::new(provider_id),
            config: MonitorConfig::default(),
            iteration_count: 0,
        }
    }

    /// Create a new context monitor with a specific model
    ///
    /// This provides more accurate context limits based on the model name.
    pub fn with_model(provider_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            counter: TokenCounter::with_model(provider_id, model),
            config: MonitorConfig::default(),
            iteration_count: 0,
        }
    }

    /// Create a new context monitor with custom config
    pub fn with_config(provider_id: impl Into<String>, config: MonitorConfig) -> Self {
        Self {
            counter: TokenCounter::new(provider_id),
            config,
            iteration_count: 0,
        }
    }

    /// Get the context limit for this provider/model
    pub fn context_limit(&self) -> usize {
        self.counter.context_limit()
    }

    /// Create a new context monitor with model and custom config
    pub fn with_model_and_config(
        provider_id: impl Into<String>,
        model: impl Into<String>,
        config: MonitorConfig,
    ) -> Self {
        Self {
            counter: TokenCounter::with_model(provider_id, model),
            config,
            iteration_count: 0,
        }
    }

    /// Get the token counter
    pub fn counter(&self) -> &TokenCounter {
        &self.counter
    }

    /// Get the monitor config
    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }

    /// Update the config
    pub fn set_config(&mut self, config: MonitorConfig) {
        self.config = config;
    }

    /// Calculate context usage from messages
    pub fn calculate_usage(
        &self,
        messages: &[Message],
        system_prompt: &str,
        memory_content: Option<&str>,
    ) -> ContextUsage {
        let limit_tokens = self.counter.context_limit();

        // Count system tokens
        let system_tokens = self.counter.count(system_prompt);

        // Count memory tokens
        let memory_tokens = memory_content.map(|c| self.counter.count(c)).unwrap_or(0);

        // Count conversation and tool tokens, separating input from output
        let mut conversation_tokens = 0;
        let mut tool_tokens = 0;
        let mut user_tokens = 0;
        let mut assistant_tokens = 0;

        for msg in messages {
            let tokens = self.counter.count(&msg.content) + 4; // +4 for message overhead

            match msg.role {
                super::MessageRole::Tool => {
                    tool_tokens += tokens;
                }
                super::MessageRole::User => {
                    user_tokens += tokens;
                    conversation_tokens += tokens;
                }
                super::MessageRole::Assistant => {
                    assistant_tokens += tokens;
                    conversation_tokens += tokens;
                }
                super::MessageRole::System => {
                    // System messages in conversation count as input
                    user_tokens += tokens;
                    conversation_tokens += tokens;
                }
            }
        }

        // Input = system prompt + memory + user messages + tool results
        // Output = assistant messages
        let input_tokens = system_tokens + memory_tokens + user_tokens + tool_tokens;
        let output_tokens = assistant_tokens;

        let used_tokens = system_tokens + memory_tokens + conversation_tokens + tool_tokens;
        let remaining_tokens = limit_tokens.saturating_sub(used_tokens);
        let used_percentage = if limit_tokens > 0 {
            used_tokens as f64 / limit_tokens as f64
        } else {
            0.0
        };

        let should_compact = used_percentage >= self.config.auto_compact_threshold
            || remaining_tokens < self.config.min_remaining_tokens;

        ContextUsage {
            used_tokens,
            limit_tokens,
            used_percentage,
            remaining_tokens,
            should_compact,
            breakdown: ContextBreakdown {
                system_tokens,
                conversation_tokens,
                tool_tokens,
                memory_tokens,
                input_tokens,
                output_tokens,
                current_output_tokens: 0,  // Set by caller if known
            },
        }
    }

    /// Check if we should evaluate context usage based on iteration count
    pub fn should_check(&mut self) -> bool {
        self.iteration_count += 1;
        self.iteration_count.is_multiple_of(self.config.check_interval)
    }

    /// Reset the iteration counter
    pub fn reset_counter(&mut self) {
        self.iteration_count = 0;
    }

    /// Format context usage as a human-readable string
    pub fn format_usage(&self, usage: &ContextUsage) -> String {
        let bar_width: usize = 20;
        let filled = (usage.used_percentage * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

        let status = if usage.should_compact {
            "⚠️ Compaction recommended"
        } else if usage.used_percentage >= 0.5 {
            "Context usage moderate"
        } else {
            "Context usage low"
        };

        format!(
            r#"Context Usage: {:.1}% {} {}

Tokens: {}/{} ({} remaining)

Breakdown:
  System:       {:>6} tokens
  Memory:       {:>6} tokens
  Conversation: {:>6} tokens
  Tool calls:   {:>6} tokens"#,
            usage.used_percentage * 100.0,
            bar,
            status,
            usage.used_tokens,
            usage.limit_tokens,
            usage.remaining_tokens,
            usage.breakdown.system_tokens,
            usage.breakdown.memory_tokens,
            usage.breakdown.conversation_tokens,
            usage.breakdown.tool_tokens,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::MessageRole;
    use chrono::Utc;

    fn create_test_message(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_calculate_usage_empty() {
        let monitor = ContextMonitor::new("anthropic");
        let usage = monitor.calculate_usage(&[], "You are a helpful assistant.", None);

        assert!(usage.used_tokens > 0);
        assert!(usage.used_percentage < 0.01);
        assert!(!usage.should_compact);
    }

    #[test]
    fn test_calculate_usage_with_messages() {
        let monitor = ContextMonitor::new("anthropic");
        let messages = vec![
            create_test_message(MessageRole::User, "Hello, how are you?"),
            create_test_message(MessageRole::Assistant, "I'm doing well, thank you!"),
        ];

        let usage = monitor.calculate_usage(&messages, "You are a helpful assistant.", None);

        assert!(usage.breakdown.conversation_tokens > 0);
        assert!(usage.breakdown.system_tokens > 0);
        assert_eq!(usage.breakdown.tool_tokens, 0);
    }

    #[test]
    fn test_calculate_usage_with_memory() {
        let monitor = ContextMonitor::new("anthropic");
        let memory = "# Project\nThis is a Rust project using Tokio.";

        let usage = monitor.calculate_usage(&[], "System prompt", Some(memory));

        assert!(usage.breakdown.memory_tokens > 0);
    }

    #[test]
    fn test_should_compact_threshold() {
        // Test compaction threshold by using a low percentage
        let threshold = 0.0001; // Very low threshold - will always trigger
        let config = MonitorConfig {
            auto_compact_threshold: threshold,
            min_remaining_tokens: 0, // Don't use this condition
            check_interval: 5,
        };

        let monitor = ContextMonitor::with_config("anthropic", config);

        // Any messages should trigger compaction due to low threshold
        let messages = vec![
            create_test_message(MessageRole::User, "Hello, this is a message."),
            create_test_message(MessageRole::Assistant, "Hi there, thanks for reaching out!"),
        ];

        let usage = monitor.calculate_usage(&messages, "System prompt", None);
        // Should trigger because any usage exceeds 0.01% threshold
        assert!(
            usage.should_compact,
            "Should compact due to low threshold. used_percentage: {}, threshold: {}",
            usage.used_percentage,
            threshold
        );
    }

    #[test]
    fn test_should_check_interval() {
        let mut monitor = ContextMonitor::new("anthropic");

        // Default interval is 5
        assert!(!monitor.should_check()); // 1
        assert!(!monitor.should_check()); // 2
        assert!(!monitor.should_check()); // 3
        assert!(!monitor.should_check()); // 4
        assert!(monitor.should_check()); // 5 - should trigger

        assert!(!monitor.should_check()); // 6
        assert!(!monitor.should_check()); // 7
        assert!(!monitor.should_check()); // 8
        assert!(!monitor.should_check()); // 9
        assert!(monitor.should_check()); // 10 - should trigger
    }

    #[test]
    fn test_format_usage() {
        let monitor = ContextMonitor::new("anthropic");
        let usage = monitor.calculate_usage(&[], "System", None);
        let formatted = monitor.format_usage(&usage);

        assert!(formatted.contains("Context Usage:"));
        assert!(formatted.contains("Breakdown:"));
    }
}
