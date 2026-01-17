//! Token counting utilities for context management
//!
//! Provides approximate token counting for various LLM providers.
//! Uses simple heuristics when tiktoken is not available.

use crate::provider::ProviderType;

/// Token counter for estimating context usage
pub struct TokenCounter {
    /// Provider type for model-specific counting
    provider: ProviderType,
}

impl TokenCounter {
    pub fn new(provider: ProviderType) -> Self {
        Self { provider }
    }

    /// Estimate token count for a string
    ///
    /// Uses approximate counting based on common tokenization patterns:
    /// - For English text: ~4 characters per token
    /// - For code: ~3 characters per token (more symbols)
    /// - For mixed content: ~3.5 characters per token
    pub fn count(&self, text: &str) -> usize {
        // Simple heuristic: average of 4 chars per token for text
        // This is a rough approximation - actual tokenization varies by model

        // Count code-like characters (more tokens per character in code)
        let code_chars = text.chars().filter(|c| {
            matches!(c, '{' | '}' | '[' | ']' | '(' | ')' | ';' | ':' | ',' | '=' | '+' | '-' | '*' | '/')
        }).count();

        // If more than 5% code characters, use code ratio
        let chars = text.chars().count();
        if chars == 0 {
            return 0;
        }

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
                ProviderType::Anthropic => 4,  // Claude message overhead
                ProviderType::OpenAI => 3,     // GPT message overhead
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
            ProviderType::Anthropic => 200_000,    // Claude 3.5 Sonnet
            ProviderType::OpenAI => 128_000,       // GPT-4o
            ProviderType::Gemini => 1_000_000,     // Gemini 1.5
            ProviderType::Cohere => 128_000,       // Command R+
            ProviderType::Groq => 128_000,         // Llama 3
            ProviderType::DeepSeek => 64_000,      // DeepSeek
            ProviderType::XAI => 131_072,          // Grok 2
            ProviderType::Perplexity => 128_000,   // Sonar
            ProviderType::Ollama => 32_000,        // Default for local models
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_text() {
        let counter = TokenCounter::new(ProviderType::Anthropic);

        // Simple text ~4 chars per token
        let text = "Hello, this is a simple test message.";
        let tokens = counter.count(text);
        assert!(tokens > 5 && tokens < 15);
    }

    #[test]
    fn test_count_code() {
        let counter = TokenCounter::new(ProviderType::Anthropic);

        // Code-heavy content ~3 chars per token
        let code = "fn main() { let x = 1 + 2; println!(\"{}\", x); }";
        let tokens = counter.count(code);
        assert!(tokens > 10 && tokens < 25);
    }
}
