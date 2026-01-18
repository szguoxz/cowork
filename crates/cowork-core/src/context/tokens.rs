//! Token counting utilities for context management
//!
//! Provides token counting for various LLM providers.
//! Uses tiktoken when available, falls back to heuristics otherwise.

use crate::provider::ProviderType;

#[cfg(feature = "tiktoken")]
use tiktoken_rs::{cl100k_base, CoreBPE};

/// Token counter for estimating context usage
pub struct TokenCounter {
    /// Provider type for model-specific counting
    provider: ProviderType,
    /// Tiktoken encoder (when feature is enabled)
    #[cfg(feature = "tiktoken")]
    encoder: Option<CoreBPE>,
}

impl TokenCounter {
    pub fn new(provider: ProviderType) -> Self {
        #[cfg(feature = "tiktoken")]
        {
            // Use cl100k_base encoding (used by GPT-4, Claude, etc.)
            let encoder = cl100k_base().ok();
            Self { provider, encoder }
        }

        #[cfg(not(feature = "tiktoken"))]
        {
            Self { provider }
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
            let overhead = match self.provider {
                ProviderType::Anthropic => 4, // Claude message overhead
                ProviderType::OpenAI => 3,    // GPT message overhead
                _ => 3,
            };

            total += overhead;
            total += self.count(&msg.content);
        }

        total
    }

    /// Get the context limit for the current provider/model
    pub fn context_limit(&self) -> usize {
        match self.provider {
            ProviderType::Anthropic => 200_000,  // Claude 3.5 Sonnet
            ProviderType::OpenAI => 128_000,     // GPT-4o
            ProviderType::Gemini => 1_000_000,   // Gemini 1.5
            ProviderType::Cohere => 128_000,     // Command R+
            ProviderType::Groq => 128_000,       // Llama 3
            ProviderType::DeepSeek => 64_000,    // DeepSeek
            ProviderType::XAI => 131_072,        // Grok 2
            ProviderType::Perplexity => 128_000, // Sonar
            ProviderType::Ollama => 32_000,      // Default for local models
        }
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
        let counter = TokenCounter::new(ProviderType::Anthropic);

        // Simple text
        let text = "Hello, this is a simple test message.";
        let tokens = counter.count(text);

        // With tiktoken, should be around 9 tokens
        // With heuristic, should be around 10 tokens
        assert!(tokens > 5 && tokens < 15);
    }

    #[test]
    fn test_count_code() {
        let counter = TokenCounter::new(ProviderType::Anthropic);

        // Code-heavy content
        let code = "fn main() { let x = 1 + 2; println!(\"{}\", x); }";
        let tokens = counter.count(code);

        // Should be more tokens for code
        assert!(tokens > 10 && tokens < 30);
    }

    #[test]
    fn test_count_empty() {
        let counter = TokenCounter::new(ProviderType::Anthropic);
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_context_limits() {
        let anthropic = TokenCounter::new(ProviderType::Anthropic);
        assert_eq!(anthropic.context_limit(), 200_000);

        let openai = TokenCounter::new(ProviderType::OpenAI);
        assert_eq!(openai.context_limit(), 128_000);

        let gemini = TokenCounter::new(ProviderType::Gemini);
        assert_eq!(gemini.context_limit(), 1_000_000);
    }

    #[test]
    fn test_summarization_threshold() {
        let counter = TokenCounter::new(ProviderType::Anthropic);
        // 80% of 200,000 = 160,000
        assert_eq!(counter.summarization_threshold(), 160_000);
    }

    #[test]
    fn test_should_summarize() {
        let counter = TokenCounter::new(ProviderType::Anthropic);

        assert!(!counter.should_summarize(100_000)); // Below threshold
        assert!(counter.should_summarize(160_000)); // At threshold
        assert!(counter.should_summarize(180_000)); // Above threshold
    }

    #[cfg(feature = "tiktoken")]
    #[test]
    fn test_tiktoken_available() {
        let counter = TokenCounter::new(ProviderType::Anthropic);
        // When tiktoken feature is enabled, it should be available
        assert!(counter.is_using_tiktoken());
    }
}
