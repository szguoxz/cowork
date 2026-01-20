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
    /// Model name for precise context limits
    model: Option<String>,
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
            Self {
                provider,
                model: None,
                encoder,
            }
        }

        #[cfg(not(feature = "tiktoken"))]
        {
            Self {
                provider,
                model: None,
            }
        }
    }

    /// Create a token counter with a specific model
    pub fn with_model(provider: ProviderType, model: impl Into<String>) -> Self {
        #[cfg(feature = "tiktoken")]
        {
            let encoder = cl100k_base().ok();
            Self {
                provider,
                model: Some(model.into()),
                encoder,
            }
        }

        #[cfg(not(feature = "tiktoken"))]
        {
            Self {
                provider,
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
    ///
    /// Uses model-specific limits when available, falls back to provider defaults.
    pub fn context_limit(&self) -> usize {
        use crate::provider::model_listing::get_model_context_limit;

        // Check model-specific limits first using centralized function
        if let Some(ref model) = self.model {
            if let Some(limit) = get_model_context_limit(self.provider, model) {
                return limit;
            }
        }

        // Fall back to provider defaults
        Self::provider_default_limit(self.provider)
    }

    /// Get default context limit for a provider (when model is unknown)
    fn provider_default_limit(provider: ProviderType) -> usize {
        match provider {
            ProviderType::Anthropic => 200_000,  // Claude 4.5/Sonnet 4 default
            ProviderType::OpenAI => 256_000,     // GPT-5 default
            ProviderType::Gemini => 1_000_000,   // Gemini 2.x
            ProviderType::Cohere => 128_000,     // Command R+
            ProviderType::Groq => 128_000,       // Llama 3
            ProviderType::DeepSeek => 131_072,   // DeepSeek V3
            ProviderType::XAI => 131_072,        // Grok 2
            ProviderType::Perplexity => 128_000, // Sonar
            ProviderType::Together => 128_000,   // Varies by model
            ProviderType::Fireworks => 128_000,  // Varies by model
            ProviderType::Zai => 128_000,        // GLM-4
            ProviderType::Nebius => 128_000,     // Varies by model
            ProviderType::MIMO => 32_000,        // MIMO
            ProviderType::BigModel => 128_000,   // GLM-4
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
        assert_eq!(openai.context_limit(), 256_000); // GPT-5 default

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

    #[test]
    fn test_model_specific_limits() {
        // Claude models
        let claude_sonnet = TokenCounter::with_model(ProviderType::Anthropic, "claude-3-5-sonnet-20241022");
        assert_eq!(claude_sonnet.context_limit(), 200_000);

        let claude_haiku = TokenCounter::with_model(ProviderType::Anthropic, "claude-3-haiku");
        assert_eq!(claude_haiku.context_limit(), 200_000);

        // GPT-4 models
        let gpt4o = TokenCounter::with_model(ProviderType::OpenAI, "gpt-4o");
        assert_eq!(gpt4o.context_limit(), 128_000);

        let gpt4_turbo = TokenCounter::with_model(ProviderType::OpenAI, "gpt-4-turbo");
        assert_eq!(gpt4_turbo.context_limit(), 128_000);

        let gpt4_base = TokenCounter::with_model(ProviderType::OpenAI, "gpt-4");
        assert_eq!(gpt4_base.context_limit(), 8_192);

        let gpt35 = TokenCounter::with_model(ProviderType::OpenAI, "gpt-3.5-turbo");
        assert_eq!(gpt35.context_limit(), 4_096);

        let gpt35_16k = TokenCounter::with_model(ProviderType::OpenAI, "gpt-3.5-turbo-16k");
        assert_eq!(gpt35_16k.context_limit(), 16_385);

        // GPT-5 models (new)
        let gpt5 = TokenCounter::with_model(ProviderType::OpenAI, "gpt-5");
        assert_eq!(gpt5.context_limit(), 256_000);

        // DeepSeek models
        let deepseek = TokenCounter::with_model(ProviderType::DeepSeek, "deepseek-chat");
        assert_eq!(deepseek.context_limit(), 131_072);

        let deepseek_coder = TokenCounter::with_model(ProviderType::DeepSeek, "deepseek-coder");
        assert_eq!(deepseek_coder.context_limit(), 128_000);

        // Gemini models
        let gemini_pro = TokenCounter::with_model(ProviderType::Gemini, "gemini-1.5-pro");
        assert_eq!(gemini_pro.context_limit(), 1_000_000);

        // Llama models
        let llama3 = TokenCounter::with_model(ProviderType::Groq, "llama-3.1-70b");
        assert_eq!(llama3.context_limit(), 128_000);

        // Unknown model should fall back to provider default
        let unknown = TokenCounter::with_model(ProviderType::OpenAI, "some-unknown-model");
        assert_eq!(unknown.context_limit(), 256_000); // OpenAI default (GPT-5 era)
    }

    #[test]
    fn test_model_limit_via_provider_module() {
        use crate::provider::model_listing::get_model_context_limit;

        // Test via the centralized function
        assert_eq!(get_model_context_limit(ProviderType::Anthropic, "claude-3-opus"), Some(200_000));
        assert_eq!(get_model_context_limit(ProviderType::OpenAI, "gpt-4o-mini"), Some(128_000));
        assert_eq!(get_model_context_limit(ProviderType::OpenAI, "gpt-5"), Some(256_000));
        assert_eq!(get_model_context_limit(ProviderType::OpenAI, "o1-preview"), Some(200_000));
        assert_eq!(get_model_context_limit(ProviderType::DeepSeek, "deepseek-chat"), Some(131_072));
    }
}
