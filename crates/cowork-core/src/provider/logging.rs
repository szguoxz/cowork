//! LLM request/response logging utilities
//!
//! Provides shared logging functionality for all provider implementations.
//! Set the `LLM_LOG_FILE` environment variable to enable detailed logging
//! of all LLM requests and responses to a JSON file.
//!
//! Example: `LLM_LOG_FILE=/tmp/llm.log cowork`

use serde_json::json;
use std::io::Write;
use tracing::{debug, warn};

use crate::tools::ToolDefinition;
use super::genai_provider::CompletionResult;
use super::ChatMessage;

/// Convert ChatMessage to JSON for logging
fn message_to_json(msg: &ChatMessage) -> serde_json::Value {
    json!({
        "role": format!("{:?}", msg.role),
        "content": super::message_text_content(msg)
    })
}

/// Configuration for what to include in the log entry
#[derive(Default)]
pub struct LogConfig<'a> {
    /// The model used for this request
    pub model: &'a str,
    /// Provider name (e.g., "genai", "rig")
    pub provider: Option<&'a str>,
    /// System prompt if available
    pub system_prompt: Option<&'a str>,
    /// Messages in the request
    pub messages: &'a [ChatMessage],
    /// Tools available for the request
    pub tools: Option<&'a [ToolDefinition]>,
    /// Parsed completion result
    pub result: Option<&'a CompletionResult>,
    /// Raw response string (for debugging)
    pub raw_response: Option<&'a str>,
    /// Error message if the request failed
    pub error: Option<&'a str>,
}

/// Log an LLM request/response interaction to file if LLM_LOG_FILE is set
///
/// This is the unified logging function used by all providers. It writes
/// a JSON object for each interaction, appending to the log file.
///
/// # Arguments
/// * `config` - Configuration containing all data to log
pub fn log_llm_interaction(config: LogConfig<'_>) {
    let log_file = match std::env::var("LLM_LOG_FILE") {
        Ok(path) => path,
        Err(_) => return, // No logging if env var not set
    };

    let messages_json: Vec<serde_json::Value> = config.messages.iter()
        .map(message_to_json)
        .collect();

    let entry = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "model": config.model,
        "provider": config.provider,
        "request": {
            "system_prompt": config.system_prompt,
            "messages": messages_json,
            "message_count": config.messages.len(),
            "tools": config.tools.map(|t| t.iter().map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "schema": tool.schema
            })).collect::<Vec<_>>()),
            "tool_count": config.tools.map(|t| t.len()).unwrap_or(0),
        },
        "response": {
            "parsed": config.result.map(|r| json!({
                "type": if r.has_tool_calls() { "tool_calls" } else { "message" },
                "content": r.content,
                "tool_calls": r.tool_calls.iter().map(|c| json!({
                    "name": c.fn_name,
                    "call_id": c.call_id,
                    "arguments": c.fn_arguments
                })).collect::<Vec<_>>()
            })),
            "raw": config.raw_response,
        },
        "error": config.error,
    });

    // Append to log file
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        Ok(mut file) => {
            // Use pretty printing if raw_response is present (Rig provider), compact otherwise
            let json_str = if config.raw_response.is_some() {
                serde_json::to_string_pretty(&entry).unwrap_or_default()
            } else {
                serde_json::to_string(&entry).unwrap_or_default()
            };
            if let Err(e) = writeln!(file, "{}", json_str) {
                warn!("Failed to write to LLM log file: {}", e);
            }
        }
        Err(e) => {
            warn!("Failed to open LLM log file {}: {}", log_file, e);
        }
    }

    debug!("Logged LLM interaction to {}", log_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_default() {
        let config = LogConfig {
            model: "test-model",
            messages: &[],
            ..Default::default()
        };
        assert_eq!(config.model, "test-model");
        assert!(config.provider.is_none());
        assert!(config.tools.is_none());
    }
}
