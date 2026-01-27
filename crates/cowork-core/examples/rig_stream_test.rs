//! Comprehensive streaming test for the rig provider
//!
//! Run with:
//! ANTHROPIC_API_KEY="your-key" cargo run -p cowork-core --example rig_stream_test

use cowork_core::provider::{LlmMessage, ProviderType, RigProvider, StreamEvent};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = RigProvider::new(ProviderType::Anthropic, None)
        .with_system_prompt("You are a helpful assistant. Be concise but complete.");

    println!("=== Rig Streaming Test ===\n");

    // Test 1: Simple question
    println!("Test 1: Simple question");
    println!("Q: What is the capital of France?");
    print!("A: ");
    let messages = vec![LlmMessage::user("What is the capital of France? Answer briefly.")];
    stream_response(&provider, messages).await?;
    println!("\n");

    // Test 2: Multi-line response
    println!("Test 2: Multi-line response");
    println!("Q: List 3 programming languages and their main use cases");
    print!("A: ");
    let messages = vec![LlmMessage::user(
        "List 3 programming languages and their main use cases. Be brief, one line each.",
    )];
    stream_response(&provider, messages).await?;
    println!("\n");

    // Test 3: Code generation
    println!("Test 3: Code generation");
    println!("Q: Write a Rust function to check if a number is prime");
    print!("A: ");
    let messages = vec![LlmMessage::user(
        "Write a short Rust function to check if a number is prime. Just the function, no explanation.",
    )];
    stream_response(&provider, messages).await?;
    println!("\n");

    // Test 4: Conversation continuation
    println!("Test 4: Conversation continuation");
    let messages = vec![
        LlmMessage::user("My name is Alice."),
        LlmMessage::assistant("Nice to meet you, Alice!"),
        LlmMessage::user("What's my name?"),
    ];
    println!("Q: [After saying 'My name is Alice'] What's my name?");
    print!("A: ");
    stream_response(&provider, messages).await?;
    println!("\n");

    println!("=== All tests complete ===");
    Ok(())
}

async fn stream_response(
    provider: &RigProvider,
    messages: Vec<LlmMessage>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = provider.chat_stream(messages, None).await?;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(text) => {
                print!("{}", text);
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
            StreamEvent::ToolCall(tc) => {
                println!("\n[Tool call: {}]", tc.name);
            }
            StreamEvent::Reasoning(r) => {
                print!("[Reasoning: {}]", r);
            }
            StreamEvent::Done(_) => {}
            StreamEvent::Error(e) => {
                println!("\n[Error: {}]", e);
            }
        }
    }

    Ok(())
}
