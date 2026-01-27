//! Simple test example for the rig provider integration
//!
//! Run with:
//! DEEPSEEK_API_KEY="your-key" cargo run -p cowork-core --example rig_test
//!
//! Or with OpenAI:
//! OPENAI_API_KEY="your-key" cargo run -p cowork-core --example rig_test -- --provider openai

use std::sync::Arc;

use cowork_core::approval::{ApprovalLevel, ToolApprovalConfig};
use cowork_core::provider::rig_provider::{
    RigAgentConfig, RigProviderType, ToolContext, run_rig_agent,
};
use cowork_core::session::SessionOutput;
use cowork_core::tools::filesystem::{GlobFiles, GrepFiles, ReadFile};
use cowork_core::tools::Tool;
use tokio::sync::mpsc;

const SYSTEM_PROMPT: &str = r#"You are a helpful AI assistant. You have access to tools for reading files. Be concise in your responses."#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    // Parse args for provider selection
    let args: Vec<String> = std::env::args().collect();
    let provider = if args.iter().any(|a| a == "--provider") {
        let idx = args.iter().position(|a| a == "--provider").unwrap();
        match args.get(idx + 1).map(|s| s.as_str()) {
            Some("openai") => RigProviderType::OpenAI,
            Some("anthropic") => RigProviderType::Anthropic,
            Some("deepseek") | None => RigProviderType::DeepSeek,
            Some(other) => {
                eprintln!("Unknown provider: {}. Using DeepSeek.", other);
                RigProviderType::DeepSeek
            }
        }
    } else {
        RigProviderType::DeepSeek
    };

    println!("=== Rig Provider Test ===");
    println!("Provider: {:?}", provider);
    println!();

    // Create channels for tool context
    let (output_tx, mut output_rx) = mpsc::channel::<SessionOutput>(100);
    let (_input_tx, input_rx) = mpsc::channel(100);

    // Create tool context
    let workspace = std::env::current_dir()?;
    let approval_config = ToolApprovalConfig::new(ApprovalLevel::None); // Auto-approve all
    let context = ToolContext::new(output_tx, input_rx, approval_config, workspace.clone());

    // Create tools
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ReadFile::new(workspace.clone())),
        Arc::new(GlobFiles::new(workspace.clone())),
        Arc::new(GrepFiles::new(workspace)),
    ];

    // Spawn a task to print tool events
    tokio::spawn(async move {
        while let Some(output) = output_rx.recv().await {
            match output {
                SessionOutput::ToolStart { name, .. } => {
                    println!("[Tool Start] {}", name);
                }
                SessionOutput::ToolDone { name, success, .. } => {
                    println!("[Tool Done] {} - success: {}", name, success);
                }
                SessionOutput::ToolResult { name, summary, .. } => {
                    println!("[Tool Result] {} - {}", name, summary);
                }
                _ => {}
            }
        }
    });

    // Configure agent
    let config = RigAgentConfig {
        provider,
        api_key: None, // Use environment variable
        model: None,   // Use default model
        system_prompt: Some(SYSTEM_PROMPT.to_string()),
        max_iterations: 10,
    };

    // Test prompt
    let prompt = "List the files in the current directory using the Glob tool with pattern '*'. Then tell me how many files you found.";
    println!("Prompt: {}", prompt);
    println!();

    // Run the agent
    match run_rig_agent(config, tools, context, prompt).await {
        Ok(result) => {
            println!("\n=== Result ===");
            println!("{}", result);
        }
        Err(e) => {
            eprintln!("\n=== Error ===");
            eprintln!("{}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
