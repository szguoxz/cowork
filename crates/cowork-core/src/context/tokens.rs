//! Token counting utilities for context management
//!
//! Provides token counting for various LLM providers.
//! Uses tiktoken when available, falls back to heuristics otherwise.

use crate::provider::catalog;

#[cfg(feature = "tiktoken")]
use tiktoken_rs::{cl100k_base, CoreBPE};

/// Token counter for estimating context usage
pub struct TokenCounter {
    /// Provider ID (e.g., "anthropic", "openai")
    provider_id: String,
    /// Model name for precise context limits
    model: Option<String>,
    /// Tiktoken encoder (when feature is enabled)
    #[cfg(feature = "tiktoken")]
    encoder: Option<CoreBPE>,
}

impl TokenCounter {
    pub fn new(provider_id: impl Into<String>) -> Self {
        #[cfg(feature = "tiktoken")]
        {
            // Use cl100k_base encoding (used by GPT-4, Claude, etc.)
            let encoder = cl100k_base().ok();
            Self {
                provider_id: provider_id.into(),
                model: None,
                encoder,
            }
        }

        #[cfg(not(feature = "tiktoken"))]
        {
            Self {
                provider_id: provider_id.into(),
                model: None,
            }
        }
    }

    /// Create a token counter with a specific model
    pub fn with_model(provider_id: impl Into<String>, model: impl Into<String>) -> Self {
        #[cfg(feature = "tiktoken")]
        {
            let encoder = cl100k_base().ok();
            Self {
                provider_id: provider_id.into(),
                model: Some(model.into()),
                encoder,
            }
        }

        #[cfg(not(feature = "tiktoken"))]
        {
            Self {
                provider_id: provider_id.into(),
                model: Some(model.into()),
            }
        }
    }

    /// Count tokens for a string
    ///
    /// Uses tiktoken when available, otherwise falls back to heuristics:
    /// - For English text: ~4 characters per token
    /// - For code: ~3 characters per token (more symbols)
    pub fn count(&self, text: &str) -> usize {
        #[cfg(feature = "tiktoken")]
        {
            if let Some(ref encoder) = self.encoder {
                return encoder.encode_with_special_tokens(text).len();
            }
        }

        // Fallback: heuristic counting
        self.count_heuristic(text)
    }

    /// Heuristic token counting (fallback when tiktoken unavailable)
    fn count_heuristic(&self, text: &str) -> usize {
        // Count code-like characters (more tokens per character in code)
        let code_chars = text
            .chars()
            .filter(|c| {
                matches!(
                    c,
                    '{' | '}'
                        | '['
                        | ']'
                        | '('
                        | ')'
                        | ';'
                        | ':'
                        | ','
                        | '='
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                )
            })
            .count();

        let chars = text.chars().count();
        if chars == 0 {
            return 0;
        }

        // If more than 5% code characters, use code ratio
        let ratio = if code_chars as f64 / chars as f64 > 0.05 {
            3.0 // Code-heavy content
        } else {
            4.0 // Regular text
        };

        (chars as f64 / ratio).ceil() as usize
    }

    /// Count tokens in a list of messages
    pub fn count_messages(&self, messages: &[super::Message]) -> usize {
        let mut total = 0;

        for msg in messages {
            // Add overhead for message structure (role, separators, etc.)
            let overhead = match self.provider_id.as_str() {
                "anthropic" => 4, // Claude message overhead
                "openai" => 3,    // GPT message overhead
                _ => 3,
            };

            total += overhead;
            total += self.count(&msg.content);
        }

        total
    }

    /// Get the context limit for the current provider/model
    ///
    /// Uses model-specific limits when available, falls back to provider defaults.
    pub fn context_limit(&self) -> usize {
        use crate::provider::model_listing::get_model_context_limit;

        // Check model-specific limits first using centralized function
        if let Some(ref model) = self.model
            && let Some(limit) = get_model_context_limit(&self.provider_id, model) {
                return limit;
            }

        // Fall back to provider defaults from catalog
        catalog::context_window(&self.provider_id).unwrap_or(128_000)
    }

    /// Get recommended trigger threshold for summarization
    pub fn summarization_threshold(&self) -> usize {
        // Trigger summarization at 80% of context limit
        (self.context_limit() as f64 * 0.8) as usize
    }

    /// Check if context should be summarized
    pub fn should_summarize(&self, current_tokens: usize) -> bool {
        current_tokens >= self.summarization_threshold()
    }

    /// Check if tiktoken is being used
    pub fn is_using_tiktoken(&self) -> bool {
        #[cfg(feature = "tiktoken")]
        {
            self.encoder.is_some()
        }

        #[cfg(not(feature = "tiktoken"))]
        {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_text() {
        let counter = TokenCounter::new("anthropic");

        // Simple text
        let text = "Hello, this is a simple test message.";
        let tokens = counter.count(text);

        // With tiktoken, should be around 9 tokens
        // With heuristic, should be around 10 tokens
        assert!(tokens > 5 && tokens < 15);
    }

    #[test]
    fn test_count_code() {
        let counter = TokenCounter::new("anthropic");

        // Code-heavy content
        let code = "fn main() { let x = 1 + 2; println!(\"{}\", x); }";
        let tokens = counter.count(code);

        // Should be more tokens for code
        assert!(tokens > 10 && tokens < 30);
    }

    #[test]
    fn test_count_empty() {
        let counter = TokenCounter::new("anthropic");
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_context_limits() {
        let anthropic = TokenCounter::new("anthropic");
        assert_eq!(anthropic.context_limit(), 200_000);

        let openai = TokenCounter::new("openai");
        assert_eq!(openai.context_limit(), 1_000_000); // GPT-5 default from catalog

        let gemini = TokenCounter::new("gemini");
        assert_eq!(gemini.context_limit(), 1_000_000);
    }

    #[test]
    fn test_summarization_threshold() {
        let counter = TokenCounter::new("anthropic");
        // 80% of 200,000 = 160,000
        assert_eq!(counter.summarization_threshold(), 160_000);
    }

    #[test]
    fn test_should_summarize() {
        let counter = TokenCounter::new("anthropic");

        assert!(!counter.should_summarize(100_000)); // Below threshold
        assert!(counter.should_summarize(160_000)); // At threshold
        assert!(counter.should_summarize(180_000)); // Above threshold
    }

    #[cfg(feature = "tiktoken")]
    #[test]
    fn test_tiktoken_available() {
        let counter = TokenCounter::new("anthropic");
        // When tiktoken feature is enabled, it should be available
        assert!(counter.is_using_tiktoken());
    }

    #[test]
    fn test_model_specific_limits() {
        // Models in the catalog return their exact context
        let anthropic_ctx = catalog::context_window("anthropic").unwrap();
        let claude = TokenCounter::with_model("anthropic", catalog::default_model("anthropic").unwrap());
        assert_eq!(claude.context_limit(), anthropic_ctx);

        let openai_ctx = catalog::context_window("openai").unwrap();
        let gpt = TokenCounter::with_model("openai", catalog::default_model("openai").unwrap());
        assert_eq!(gpt.context_limit(), openai_ctx);

        let deepseek_ctx = catalog::context_window("deepseek").unwrap();
        let deepseek = TokenCounter::with_model("deepseek", catalog::default_model("deepseek").unwrap());
        assert_eq!(deepseek.context_limit(), deepseek_ctx);

        // Unknown models fall back to provider default
        let unknown = TokenCounter::with_model("openai", "some-unknown-model");
        assert_eq!(unknown.context_limit(), openai_ctx);
    }

    #[test]
    fn test_model_limit_via_provider_module() {
        use crate::provider::model_listing::get_model_context_limit;

        // Known models return their catalog context
        let anthropic_ctx = catalog::context_window("anthropic").unwrap();
        assert_eq!(get_model_context_limit("anthropic", catalog::default_model("anthropic").unwrap()), Some(anthropic_ctx));

        let deepseek_ctx = catalog::context_window("deepseek").unwrap();
        assert_eq!(get_model_context_limit("deepseek", "deepseek-chat"), Some(deepseek_ctx));

        // Unknown models fall back to provider default
        let openai_ctx = catalog::context_window("openai").unwrap();
        assert_eq!(get_model_context_limit("openai", "unknown-model"), Some(openai_ctx));
    }
}
