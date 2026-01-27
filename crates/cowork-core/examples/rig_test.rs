//! Simple test example for the rig provider integration
//!
//! Run with:
//! DEEPSEEK_API_KEY="your-key" cargo run -p cowork-core --example rig_test
//!
//! Or with OpenAI:
//! OPENAI_API_KEY="your-key" cargo run -p cowork-core --example rig_test -- --provider openai
//!
//! Add --stream flag to test streaming:
//! DEEPSEEK_API_KEY="your-key" cargo run -p cowork-core --example rig_test -- --stream

use cowork_core::provider::{LlmMessage, ProviderType, RigProvider, StreamEvent};
use cowork_core::tools::ToolDefinition;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse args for provider selection
    let args: Vec<String> = std::env::args().collect();
    let provider_type = if args.iter().any(|a| a == "--provider") {
        let idx = args.iter().position(|a| a == "--provider").unwrap();
        match args.get(idx + 1).map(|s| s.as_str()) {
            Some("openai") => ProviderType::OpenAI,
            Some("anthropic") => ProviderType::Anthropic,
            Some("deepseek") | None => ProviderType::DeepSeek,
            Some(other) => {
                eprintln!("Unknown provider: {}. Using DeepSeek.", other);
                ProviderType::DeepSeek
            }
        }
    } else {
        ProviderType::DeepSeek
    };

    println!("=== Rig Provider Test ===");
    println!("Provider: {:?}", provider_type);
    println!();

    // Create the provider
    let provider = RigProvider::new(provider_type, None)
        .with_system_prompt("You are a helpful assistant. Be concise.");

    // Test simple chat
    let messages = vec![LlmMessage::user("What is 2 + 2? Reply with just the number.")];

    println!("Testing simple chat...");
    let result = provider.chat(messages.clone(), None).await?;
    println!("Response: {:?}", result.content);
    println!();

    // Test with a tool definition
    let tool = ToolDefinition {
        name: "get_weather".to_string(),
        description: "Get the current weather for a location".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                }
            },
            "required": ["location"]
        }),
    };

    let messages_with_tool = vec![LlmMessage::user("What's the weather in San Francisco?")];

    println!("Testing chat with tools...");
    let result = provider
        .chat(messages_with_tool, Some(vec![tool]))
        .await?;

    if let Some(content) = result.content {
        println!("Text response: {}", content);
    }
    if !result.tool_calls.is_empty() {
        println!("Tool calls:");
        for tc in &result.tool_calls {
            println!("  - {} (id: {})", tc.name, tc.call_id);
            println!("    args: {}", tc.arguments);
        }
    }

    // Test streaming if requested
    if args.iter().any(|a| a == "--stream") {
        println!();
        println!("Testing streaming chat...");
        let stream_messages = vec![LlmMessage::user("Count from 1 to 5, one number per line.")];

        let mut stream = provider.chat_stream(stream_messages, None).await?;
        print!("Streaming response: ");
        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::TextDelta(text) => {
                    print!("{}", text);
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
                StreamEvent::ToolCall(tc) => {
                    println!("\n[Tool call: {} ({})]", tc.name, tc.call_id);
                }
                StreamEvent::Reasoning(r) => {
                    println!("\n[Reasoning: {}]", r);
                }
                StreamEvent::Done(result) => {
                    println!("\n[Done: {:?}]", result);
                }
                StreamEvent::Error(e) => {
                    println!("\n[Error: {}]", e);
                }
            }
        }
        println!();
    }

    println!();
    println!("=== Test Complete ===");

    Ok(())
}
