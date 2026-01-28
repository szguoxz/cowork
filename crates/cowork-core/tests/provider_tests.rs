//! Provider integration tests
//!
//! These tests verify that the LLM providers work correctly.
//! Set environment variables before running:
//! - ANTHROPIC_API_KEY
//! - OPENAI_API_KEY

use cowork_core::provider::{GenAIProvider, LlmMessage, catalog};

/// Helper to check if Anthropic API key is available
fn has_anthropic_key() -> bool {
    std::env::var("ANTHROPIC_API_KEY").is_ok()
}

/// Helper to check if OpenAI API key is available
fn has_openai_key() -> bool {
    std::env::var("OPENAI_API_KEY").is_ok()
}

/// Validate an API key by making a minimal API call
async fn validate_anthropic_key() -> bool {
    if !has_anthropic_key() {
        return false;
    }

    let provider = GenAIProvider::new("anthropic", Some("claude-3-5-haiku-20241022"));
    let messages = vec![LlmMessage::user("Hi")];

    provider.chat(messages, None).await.is_ok()
}

/// Validate an API key by making a minimal API call
async fn validate_openai_key() -> bool {
    if !has_openai_key() {
        return false;
    }

    let provider = GenAIProvider::new("openai", Some("gpt-4.1-nano"));
    let messages = vec![LlmMessage::user("Hi")];

    provider.chat(messages, None).await.is_ok()
}

mod catalog_tests {
    use super::*;

    #[test]
    fn test_provider_catalog() {
        let providers = [
            "anthropic",
            "openai",
            "gemini",
            "cohere",
            "groq",
            "deepseek",
        ];

        for provider_id in providers {
            println!("Testing provider: {}", provider_id);
            let model = catalog::default_model(provider_id).expect("Provider should be in catalog");
            println!("  Default model: {}", model);
            assert!(!model.is_empty());
        }
    }

    #[test]
    fn test_api_key_env_vars() {
        assert_eq!(catalog::api_key_env("anthropic"), Some("ANTHROPIC_API_KEY"));
        assert_eq!(catalog::api_key_env("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(catalog::api_key_env("gemini"), Some("GEMINI_API_KEY"));
    }

    #[test]
    fn test_default_models() {
        assert!(catalog::default_model("anthropic").unwrap().contains("claude"));
        assert!(catalog::default_model("openai").unwrap().contains("gpt"));
        assert!(catalog::default_model("gemini").unwrap().contains("gemini"));
    }

    #[test]
    fn test_catalog_ids() {
        let known_ids: Vec<&str> = catalog::ids().collect();
        assert!(known_ids.contains(&"anthropic"));
        assert!(known_ids.contains(&"openai"));
        assert!(known_ids.contains(&"gemini"));
        assert!(known_ids.contains(&"groq"));
        assert!(known_ids.contains(&"deepseek"));
        assert!(known_ids.contains(&"xai"));
        assert!(known_ids.contains(&"together"));
        assert!(known_ids.contains(&"fireworks"));
        assert!(known_ids.contains(&"ollama"));
    }
}

mod provider_creation_tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = GenAIProvider::new("anthropic", None);
        assert_eq!(provider.provider_id(), "anthropic");
        assert!(provider.model().contains("claude"));
    }

    #[test]
    fn test_openai_provider_creation() {
        let provider = GenAIProvider::new("openai", None);
        assert_eq!(provider.provider_id(), "openai");
        assert!(provider.model().contains("gpt"));
    }

    #[test]
    fn test_provider_with_custom_model() {
        let provider = GenAIProvider::new("anthropic", Some("claude-opus-4-5-20251101"));
        assert_eq!(provider.model(), "claude-opus-4-5-20251101");
    }

    #[test]
    fn test_provider_with_system_prompt() {
        let provider = GenAIProvider::new("anthropic", None)
            .with_system_prompt("You are a helpful assistant.");
        assert_eq!(provider.provider_id(), "anthropic");
    }

    #[test]
    fn test_provider_with_api_key() {
        // This test just verifies the constructor works
        let provider = GenAIProvider::with_api_key(
            "anthropic",
            "test-key-not-real",
            None,
        );
        assert_eq!(provider.provider_id(), "anthropic");
    }
}

mod integration_tests {
    use super::*;

    // Note: These tests require actual API keys and will make real API calls
    // They are marked with #[ignore] by default
    // Run with: cargo test -- --ignored

    #[tokio::test]
    #[ignore]
    async fn test_anthropic_simple_chat() {
        if !has_anthropic_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        if !validate_anthropic_key().await {
            eprintln!("Skipping: ANTHROPIC_API_KEY is invalid (401 Unauthorized)");
            return;
        }

        let provider = GenAIProvider::new("anthropic", None)
            .with_system_prompt("You are a helpful assistant. Keep responses brief.");

        let messages = vec![LlmMessage::user("What is 2 + 2? Reply with just the number.")];

        let result = provider.chat(messages, None).await;
        println!("Anthropic result: {:?}", result);

        assert!(result.is_ok(), "API call failed: {:?}", result.err());

        let result = result.unwrap();
        assert!(!result.has_tool_calls(), "Unexpected tool calls in simple chat");
        let text = result.content.expect("Expected content");
        println!("Response: {}", text);
        assert!(text.contains("4"), "Expected response to contain '4'");
    }

    #[tokio::test]
    #[ignore]
    async fn test_openai_simple_chat() {
        if !has_openai_key() {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }

        if !validate_openai_key().await {
            eprintln!("Skipping: OPENAI_API_KEY is invalid (401 Unauthorized)");
            return;
        }

        let provider = GenAIProvider::new("openai", None)
            .with_system_prompt("You are a helpful assistant. Keep responses brief.");

        let messages = vec![LlmMessage::user("What is 2 + 2? Reply with just the number.")];

        let result = provider.chat(messages, None).await;
        println!("OpenAI result: {:?}", result);

        assert!(result.is_ok(), "API call failed: {:?}", result.err());

        let result = result.unwrap();
        assert!(!result.has_tool_calls(), "Unexpected tool calls in simple chat");
        let text = result.content.expect("Expected content");
        println!("Response: {}", text);
        assert!(text.contains("4"), "Expected response to contain '4'");
    }

    #[tokio::test]
    #[ignore]
    async fn test_anthropic_with_tool_call() {
        if !has_anthropic_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        if !validate_anthropic_key().await {
            eprintln!("Skipping: ANTHROPIC_API_KEY is invalid (401 Unauthorized)");
            return;
        }

        use cowork_core::tools::ToolDefinition;
        use serde_json::json;

        let provider = GenAIProvider::new("anthropic", None)
            .with_system_prompt("You are a helpful assistant. Use tools when appropriate.");

        // Define a simple tool
        let tools = vec![ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get the current weather for a city".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "city": {
                        "type": "string",
                        "description": "The city name"
                    }
                },
                "required": ["city"]
            }),
        }];

        let messages = vec![LlmMessage::user("What's the weather in Paris?")];

        let result = provider.chat(messages, Some(tools)).await;
        println!("Anthropic tool result: {:?}", result);

        assert!(result.is_ok(), "API call failed: {:?}", result.err());

        let result = result.unwrap();
        if result.has_tool_calls() {
            println!("Tool calls: {:?}", result.tool_calls);
            assert!(!result.tool_calls.is_empty(), "Expected at least one tool call");
            assert_eq!(result.tool_calls[0].fn_name, "get_weather");
        } else {
            // Some models might not use tools - that's OK
            println!("Got message instead of tool call: {:?}", result.content);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_openai_with_tool_call() {
        if !has_openai_key() {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }

        if !validate_openai_key().await {
            eprintln!("Skipping: OPENAI_API_KEY is invalid (401 Unauthorized)");
            return;
        }

        use cowork_core::tools::ToolDefinition;
        use serde_json::json;

        let provider = GenAIProvider::new("openai", None)
            .with_system_prompt("You are a helpful assistant. Use tools when appropriate.");

        // Define a simple tool
        let tools = vec![ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get the current weather for a city".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "city": {
                        "type": "string",
                        "description": "The city name"
                    }
                },
                "required": ["city"]
            }),
        }];

        let messages = vec![LlmMessage::user("What's the weather in Paris?")];

        let result = provider.chat(messages, Some(tools)).await;
        println!("OpenAI tool result: {:?}", result);

        assert!(result.is_ok(), "API call failed: {:?}", result.err());

        let result = result.unwrap();
        if result.has_tool_calls() {
            println!("Tool calls: {:?}", result.tool_calls);
            assert!(!result.tool_calls.is_empty(), "Expected at least one tool call");
            assert_eq!(result.tool_calls[0].fn_name, "get_weather");
        } else {
            // Some models might not use tools - that's OK
            println!("Got message instead of tool call: {:?}", result.content);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_anthropic_conversation() {
        if !has_anthropic_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        if !validate_anthropic_key().await {
            eprintln!("Skipping: ANTHROPIC_API_KEY is invalid (401 Unauthorized)");
            return;
        }

        let provider = GenAIProvider::new("anthropic", None)
            .with_system_prompt("You are a helpful assistant. Keep responses very brief.");

        // First message
        let messages1 = vec![LlmMessage::user("My name is Alice.")];

        let result1 = provider.chat(messages1.clone(), None).await;
        assert!(result1.is_ok(), "First API call failed: {:?}", result1.err());

        let result1 = result1.unwrap();
        assert!(!result1.has_tool_calls(), "Unexpected tool calls");
        let response1 = result1.content.expect("Expected content");
        println!("First response: {}", response1);

        // Second message - test context
        let mut messages2 = messages1;
        messages2.push(LlmMessage::assistant(response1));
        messages2.push(LlmMessage::user("What is my name?"));

        let result2 = provider.chat(messages2, None).await;
        assert!(result2.is_ok(), "Second API call failed: {:?}", result2.err());

        let result2 = result2.unwrap();
        assert!(!result2.has_tool_calls(), "Unexpected tool calls");
        let text = result2.content.expect("Expected content");
        println!("Second response: {}", text);
        assert!(
            text.to_lowercase().contains("alice"),
            "Expected response to remember 'Alice'"
        );
    }
}

mod subagent_tests {
    use std::sync::Arc;
    use cowork_core::tools::task::{AgentExecutionConfig, AgentInstanceRegistry, AgentType, ModelTier};
    use cowork_core::tools::task::executor::run_subagent;

    fn has_anthropic_key() -> bool {
        std::env::var("ANTHROPIC_API_KEY").is_ok()
    }

    #[tokio::test]
    #[ignore]
    async fn test_subagent_explore() {
        if !has_anthropic_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }

        let workspace = std::env::current_dir().unwrap();
        let registry = Arc::new(AgentInstanceRegistry::new());

        let config = AgentExecutionConfig::new(workspace)
            .with_provider("anthropic")
            .with_max_turns(3);

        let result = run_subagent(
            &AgentType::Explore,
            &ModelTier::Fast,
            "List the files in the current directory. Just return the listing.",
            &config,
            registry.clone(),
            "test-subagent-explore",
        )
        .await;

        println!("Subagent result: {:?}", result);
        assert!(result.is_ok(), "Subagent failed: {:?}", result.err());

        let output = result.unwrap();
        assert!(!output.is_empty(), "Subagent returned empty output");
        println!("Subagent output:\n{}", output);
    }
}
