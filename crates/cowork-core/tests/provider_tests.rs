//! Provider integration tests
//!
//! These tests verify that the LLM providers work correctly.
//! Set environment variables before running:
//! - ANTHROPIC_API_KEY
//! - OPENAI_API_KEY

use cowork_core::provider::{GenAIProvider, ProviderType};

/// Helper to check if Anthropic API key is available
fn has_anthropic_key() -> bool {
    std::env::var("ANTHROPIC_API_KEY").is_ok()
}

/// Helper to check if OpenAI API key is available
fn has_openai_key() -> bool {
    std::env::var("OPENAI_API_KEY").is_ok()
}

mod provider_type_tests {
    use super::*;

    #[test]
    fn test_provider_types() {
        let providers = [
            ProviderType::Anthropic,
            ProviderType::OpenAI,
            ProviderType::Gemini,
            ProviderType::Cohere,
            ProviderType::Groq,
            ProviderType::DeepSeek,
        ];

        for provider_type in providers {
            println!("Testing provider: {:?}", provider_type);
            let model = provider_type.default_model();
            println!("  Default model: {}", model);
            assert!(!model.is_empty());
        }
    }

    #[test]
    fn test_api_key_env_vars() {
        assert_eq!(ProviderType::Anthropic.api_key_env(), Some("ANTHROPIC_API_KEY"));
        assert_eq!(ProviderType::OpenAI.api_key_env(), Some("OPENAI_API_KEY"));
        assert_eq!(ProviderType::Gemini.api_key_env(), Some("GEMINI_API_KEY"));
    }

    #[test]
    fn test_default_models() {
        assert!(ProviderType::Anthropic.default_model().contains("claude"));
        assert!(ProviderType::OpenAI.default_model().contains("gpt"));
        assert!(ProviderType::Gemini.default_model().contains("gemini"));
    }
}

mod provider_creation_tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = GenAIProvider::new(ProviderType::Anthropic, None);
        assert_eq!(provider.provider_type(), ProviderType::Anthropic);
        assert!(provider.model().contains("claude"));
    }

    #[test]
    fn test_openai_provider_creation() {
        let provider = GenAIProvider::new(ProviderType::OpenAI, None);
        assert_eq!(provider.provider_type(), ProviderType::OpenAI);
        assert!(provider.model().contains("gpt"));
    }

    #[test]
    fn test_provider_with_custom_model() {
        let provider = GenAIProvider::new(ProviderType::Anthropic, Some("claude-3-opus-20240229"));
        assert_eq!(provider.model(), "claude-3-opus-20240229");
    }

    #[test]
    fn test_provider_with_system_prompt() {
        let provider = GenAIProvider::new(ProviderType::Anthropic, None)
            .with_system_prompt("You are a helpful assistant.");
        assert_eq!(provider.provider_type(), ProviderType::Anthropic);
    }

    #[test]
    fn test_provider_with_api_key() {
        // This test just verifies the constructor works
        let provider = GenAIProvider::with_api_key(
            ProviderType::Anthropic,
            "test-key-not-real",
            None,
        );
        assert_eq!(provider.provider_type(), ProviderType::Anthropic);
    }
}

mod integration_tests {
    use super::*;

    // Note: These tests require actual API keys and will make real API calls
    // They are marked with #[ignore] by default

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_anthropic_api_call() {
        if !has_anthropic_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        let provider = GenAIProvider::new(ProviderType::Anthropic, None);
        // This would require implementing the actual completion call
        // For now, just verify provider creation works with the actual key
        println!("Provider created: {:?}", provider.provider_type());
    }

    #[tokio::test]
    #[ignore]
    async fn test_openai_api_call() {
        if !has_openai_key() {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }

        let provider = GenAIProvider::new(ProviderType::OpenAI, None);
        println!("Provider created: {:?}", provider.provider_type());
    }
}
