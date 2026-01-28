//! Simple CLI chat example to test cowork-core functionality
//!
//! Run with:
//! ANTHROPIC_API_KEY="your-key" cargo run -p cowork-core --example chat_cli
//!
//! Or with OpenAI:
//! OPENAI_API_KEY="your-key" cargo run -p cowork-core --example chat_cli -- --provider openai

use std::io::{self, Write};
use std::path::Path;

use cowork_core::provider::{GenAIProvider, ChatMessage};
use cowork_core::tools::ToolRegistry;
use cowork_core::tools::filesystem::{ReadFile, WriteFile, GlobFiles, GrepFiles};
use cowork_core::tools::shell::ExecuteCommand;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse args
    let args: Vec<String> = std::env::args().collect();
    let provider_id = if args.iter().any(|a| a == "--provider") {
        let idx = args.iter().position(|a| a == "--provider").unwrap();
        match args.get(idx + 1).map(|s| s.as_str()) {
            Some("openai") => "openai",
            Some("anthropic") | None => "anthropic",
            Some(other) => {
                eprintln!("Unknown provider: {}. Using Anthropic.", other);
                "anthropic"
            }
        }
    } else {
        "anthropic"
    };

    println!("=== Cowork CLI Chat ===");
    println!("Provider: {}", provider_id);
    println!("Type 'quit' or 'exit' to quit");
    println!("Type '/help' for available commands");
    println!();

    // Create provider
    let provider = GenAIProvider::new(provider_id, None)
        .with_system_prompt(SYSTEM_PROMPT);

    // Create tool registry
    let workspace = std::env::current_dir()?;
    let tool_registry = create_tool_registry(&workspace);
    let tool_definitions = tool_registry.list();

    // Chat history
    let mut messages: Vec<ChatMessage> = Vec::new();

    loop {
        // Print prompt
        print!("You: ");
        io::stdout().flush()?;

        // Read input
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // Check for quit
        if input == "quit" || input == "exit" {
            println!("Goodbye!");
            break;
        }

        // Check for slash commands
        if input.starts_with('/') {
            handle_slash_command(input, &workspace).await;
            continue;
        }

        // Add user message
        messages.push(ChatMessage::user(input));

        // Get response
        print!("Assistant: ");
        io::stdout().flush()?;

        match provider.chat(messages.clone(), Some(tool_definitions.clone())).await {
            Ok(result) => {
                // Print any content from the assistant
                if let Some(text) = &result.content {
                    println!("{}", text);
                }

                if result.has_tool_calls() {
                    println!("(wants to use {} tool(s))", result.tool_calls.len());

                    for call in &result.tool_calls {
                        println!("\n  Tool: {}", call.fn_name);
                        println!("  Args: {}", serde_json::to_string_pretty(&call.fn_arguments)?);

                        // Ask for approval
                        print!("  Approve? [y/n]: ");
                        io::stdout().flush()?;

                        let mut approval = String::new();
                        io::stdin().read_line(&mut approval)?;

                        if approval.trim().to_lowercase() == "y" {
                            // Execute tool
                            if let Some(tool) = tool_registry.get(&call.fn_name) {
                                match tool.execute(call.fn_arguments.clone()).await {
                                    Ok(output) => {
                                        println!("  Result: {}",
                                            if output.content.to_string().len() > 200 {
                                                format!("{}... (truncated)", &output.content.to_string()[..200])
                                            } else {
                                                output.content.to_string()
                                            }
                                        );

                                        // Add tool result to messages
                                        messages.push(ChatMessage::assistant(
                                            format!("Used tool {} with result: {}", call.fn_name, output.content)
                                        ));
                                    }
                                    Err(e) => {
                                        println!("  Error: {}", e);
                                        messages.push(ChatMessage::assistant(
                                            format!("Tool {} failed: {}", call.fn_name, e)
                                        ));
                                    }
                                }
                            } else {
                                println!("  Unknown tool: {}", call.fn_name);
                            }
                        } else {
                            println!("  Rejected");
                            messages.push(ChatMessage::assistant(
                                format!("User rejected tool call: {}", call.fn_name)
                            ));
                        }
                    }
                } else if let Some(text) = result.content {
                    // No tool calls, just add the message
                    messages.push(ChatMessage::assistant(text));
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        println!();
    }

    Ok(())
}

fn create_tool_registry(workspace: &Path) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    registry.register(std::sync::Arc::new(ReadFile::new(workspace.to_path_buf())));
    registry.register(std::sync::Arc::new(WriteFile::new(workspace.to_path_buf())));
    registry.register(std::sync::Arc::new(GlobFiles::new(workspace.to_path_buf())));
    registry.register(std::sync::Arc::new(GrepFiles::new(workspace.to_path_buf())));
    registry.register(std::sync::Arc::new(ExecuteCommand::new(workspace.to_path_buf())));

    registry
}

async fn handle_slash_command(cmd: &str, workspace: &Path) {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let command = parts[0];
    let args = parts.get(1).unwrap_or(&"").to_string();

    let registry = cowork_core::skills::SkillRegistry::with_builtins(workspace.to_path_buf());

    match command {
        "/help" => {
            println!("\nAvailable commands:");
            println!("  /help          - Show this help");
            println!("  /commit        - Create a git commit");
            println!("  /push          - Push to remote");
            println!("  /pr [title]    - Create a pull request");
            println!("  /review        - Review staged changes");
            println!("  /clean-gone    - Clean up deleted branches");
            println!();
        }
        "/commit" | "/push" | "/pr" | "/review" | "/clean-gone" => {
            let skill_name = &command[1..]; // Remove leading /
            let ctx = cowork_core::skills::SkillContext {
                workspace: workspace.to_path_buf(),
                args,
                data: std::collections::HashMap::new(),
            };

            let result = registry.execute(skill_name, ctx).await;
            if result.success {
                println!("\n{}", result.response);
            } else {
                println!("\nError: {}", result.error.unwrap_or_default());
            }
            println!();
        }
        _ => {
            println!("Unknown command: {}. Type /help for available commands.", command);
        }
    }
}

const SYSTEM_PROMPT: &str = r#"You are Cowork, an AI coding assistant.

You have access to these tools:
- read_file: Read file contents
- write_file: Write content to a file
- glob: Search for files by pattern
- grep: Search file contents
- execute_command: Run shell commands

When the user asks you to perform a task:
1. Think about what tools you need
2. Use tools to accomplish the task
3. Explain what you're doing

Be concise. Ask for clarification if the request is ambiguous."#;
