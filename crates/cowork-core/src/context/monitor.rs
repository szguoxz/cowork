//! Context monitoring for tracking token usage and triggering auto-compact
//!
//! Uses LLM-reported token counts for accurate context tracking.
//! The LLM's input_tokens IS the cumulative context size - no local counting needed.

use serde::{Deserialize, Serialize};

use crate::provider::{catalog, model_listing::get_model_context_limit};

/// Configuration for the context monitor
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Threshold percentage at which auto-compact should trigger (0.0 - 1.0)
    pub auto_compact_threshold: f64,
    /// Minimum tokens remaining before forcing compaction
    pub min_remaining_tokens: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            auto_compact_threshold: 0.75,
            min_remaining_tokens: 20_000,
        }
    }
}

/// Current context usage statistics (from LLM response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsage {
    /// Input tokens from last LLM response (cumulative context)
    pub input_tokens: u64,
    /// Output tokens from last LLM response
    pub output_tokens: u64,
    /// Maximum tokens available for the provider
    pub limit_tokens: usize,
    /// Percentage of context used (0.0 - 1.0)
    pub used_percentage: f64,
    /// Remaining tokens available
    pub remaining_tokens: usize,
    /// Whether context should be compacted
    pub should_compact: bool,
}

/// Context monitor that tracks token usage and signals when compaction is needed
///
/// Uses LLM-reported input_tokens for accurate tracking instead of local estimates.
pub struct ContextMonitor {
    /// Provider ID (e.g., "anthropic", "openai")
    provider_id: String,
    /// Model name for precise context limits
    model: Option<String>,
    config: MonitorConfig,
    /// Last reported input tokens from LLM (cumulative context size)
    last_input_tokens: u64,
    /// Last reported output tokens from LLM
    last_output_tokens: u64,
}

impl ContextMonitor {
    /// Create a new context monitor for a provider
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: None,
            config: MonitorConfig::default(),
            last_input_tokens: 0,
            last_output_tokens: 0,
        }
    }

    /// Create a new context monitor with a specific model
    ///
    /// This provides more accurate context limits based on the model name.
    pub fn with_model(provider_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: Some(model.into()),
            config: MonitorConfig::default(),
            last_input_tokens: 0,
            last_output_tokens: 0,
        }
    }

    /// Create a new context monitor with custom config
    pub fn with_config(provider_id: impl Into<String>, config: MonitorConfig) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: None,
            config,
            last_input_tokens: 0,
            last_output_tokens: 0,
        }
    }

    /// Create a new context monitor with model and custom config
    pub fn with_model_and_config(
        provider_id: impl Into<String>,
        model: impl Into<String>,
        config: MonitorConfig,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: Some(model.into()),
            config,
            last_input_tokens: 0,
            last_output_tokens: 0,
        }
    }

    /// Get the context limit for this provider/model
    pub fn context_limit(&self) -> usize {
        // Check model-specific limits first
        if let Some(ref model) = self.model {
            if let Some(limit) = get_model_context_limit(&self.provider_id, model) {
                if limit > 0 {
                    return limit;
                }
            }
        }

        // Fall back to provider defaults from catalog
        let limit = catalog::context_window(&self.provider_id).unwrap_or(128_000);

        // Guard against 0 (should never happen with valid catalog data)
        if limit == 0 {
            tracing::warn!(
                provider_id = %self.provider_id,
                model = ?self.model,
                "Context limit is 0 - returning fallback 128000"
            );
            return 128_000;
        }

        limit
    }

    /// Update token counts from LLM response
    ///
    /// Call this after every LLM call with the reported token counts.
    /// input_tokens is the cumulative context size (everything sent to LLM).
    pub fn update_from_response(&mut self, input_tokens: Option<u64>, output_tokens: Option<u64>) {
        if let Some(input) = input_tokens {
            self.last_input_tokens = input;
        }
        if let Some(output) = output_tokens {
            self.last_output_tokens = output;
        }
    }

    /// Get current context usage based on last LLM response
    pub fn current_usage(&self) -> ContextUsage {
        let limit = self.context_limit();
        let input = self.last_input_tokens;
        let output = self.last_output_tokens;
        let used = input + output;

        let used_percentage = if limit > 0 {
            used as f64 / limit as f64
        } else {
            0.0
        };

        let remaining = (limit as u64).saturating_sub(used) as usize;
        let should_compact = used_percentage >= self.config.auto_compact_threshold
            || remaining < self.config.min_remaining_tokens;

        ContextUsage {
            input_tokens: input,
            output_tokens: output,
            limit_tokens: limit,
            used_percentage,
            remaining_tokens: remaining,
            should_compact,
        }
    }

    /// Check if context should be compacted based on last LLM response
    pub fn should_compact(&self) -> bool {
        self.current_usage().should_compact
    }

    /// Get the monitor config
    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }

    /// Update the config
    pub fn set_config(&mut self, config: MonitorConfig) {
        self.config = config;
    }

    /// Reset token counts (e.g., after compaction)
    pub fn reset_tokens(&mut self) {
        self.last_input_tokens = 0;
        self.last_output_tokens = 0;
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
  Input:  {} tokens
  Output: {} tokens"#,
            usage.used_percentage * 100.0,
            bar,
            status,
            usage.input_tokens + usage.output_tokens,
            usage.limit_tokens,
            usage.remaining_tokens,
            usage.input_tokens,
            usage.output_tokens,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let monitor = ContextMonitor::new("anthropic");
        let usage = monitor.current_usage();

        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(!usage.should_compact);
    }

    #[test]
    fn test_update_from_response() {
        let mut monitor = ContextMonitor::new("anthropic");

        // Simulate LLM response with token counts
        monitor.update_from_response(Some(50_000), Some(1_000));

        let usage = monitor.current_usage();
        assert_eq!(usage.input_tokens, 50_000);
        assert_eq!(usage.output_tokens, 1_000);
        assert!(usage.used_percentage > 0.0);
    }

    #[test]
    fn test_should_compact_threshold() {
        let config = MonitorConfig {
            auto_compact_threshold: 0.75,
            min_remaining_tokens: 0,
        };

        let mut monitor = ContextMonitor::with_config("anthropic", config);

        // Below threshold (75% of 200k = 150k)
        monitor.update_from_response(Some(100_000), Some(0));
        assert!(!monitor.should_compact());

        // Above threshold
        monitor.update_from_response(Some(160_000), Some(0));
        assert!(monitor.should_compact());
    }

    #[test]
    fn test_should_compact_min_remaining() {
        let config = MonitorConfig {
            auto_compact_threshold: 1.0, // Never trigger by percentage
            min_remaining_tokens: 50_000,
        };

        let mut monitor = ContextMonitor::with_config("anthropic", config);
        // 200k limit, 50k min remaining means 150k max usage

        // Below limit
        monitor.update_from_response(Some(140_000), Some(0));
        assert!(!monitor.should_compact());

        // Above limit (less than 50k remaining)
        monitor.update_from_response(Some(160_000), Some(0));
        assert!(monitor.should_compact());
    }

    #[test]
    fn test_context_limit() {
        let monitor = ContextMonitor::new("anthropic");
        assert_eq!(monitor.context_limit(), 200_000);

        let monitor = ContextMonitor::new("openai");
        // OpenAI default from catalog
        assert!(monitor.context_limit() > 0);
    }

    #[test]
    fn test_reset_tokens() {
        let mut monitor = ContextMonitor::new("anthropic");
        monitor.update_from_response(Some(100_000), Some(5_000));

        monitor.reset_tokens();

        let usage = monitor.current_usage();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    #[test]
    fn test_format_usage() {
        let mut monitor = ContextMonitor::new("anthropic");
        monitor.update_from_response(Some(50_000), Some(1_000));

        let usage = monitor.current_usage();
        let formatted = monitor.format_usage(&usage);

        assert!(formatted.contains("Context Usage:"));
        assert!(formatted.contains("Input:"));
        assert!(formatted.contains("Output:"));
    }
}
