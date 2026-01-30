//! Read file tool

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

use super::{path_to_display, validate_path};

/// Maximum tokens allowed in file read output (matches Claude Code's limit)
const MAX_OUTPUT_TOKENS: usize = 25000;

/// Estimate token count for a string
/// Uses tiktoken if available, otherwise falls back to char/4 approximation
#[cfg(feature = "tiktoken")]
fn estimate_tokens(text: &str) -> usize {
    use std::sync::OnceLock;
    static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();

    let bpe = BPE.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("Failed to load cl100k_base tokenizer")
    });
    bpe.encode_with_special_tokens(text).len()
}

#[cfg(not(feature = "tiktoken"))]
fn estimate_tokens(text: &str) -> usize {
    // Approximate: ~4 characters per token
    text.len() / 4
}

/// Tool for reading file contents
pub struct ReadFile {
    workspace: PathBuf,
}

impl ReadFile {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for ReadFile {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::READ
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute or relative path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "The line number to start reading from. Only provide if the file is too large to read at once"
                },
                "limit": {
                    "type": "integer",
                    "description": "The number of lines to read. Only provide if the file is too large to read at once"
                }
            },
            "required": ["file_path"]
        })
    }

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path_str = params["file_path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("file_path is required".into()))?;

            let path = self.workspace.join(path_str);
            let validated = validate_path(&path, &self.workspace)?;

            // Reject directories with a helpful message
            if validated.is_dir() {
                return Err(ToolError::InvalidParams(format!(
                    "{} is a directory, not a file. Use the Bash tool with `ls` to list directory contents.",
                    path_to_display(&validated)
                )));
            }

            // Check if this is a document file (PDF, Word, Excel, PowerPoint)
            let ext = validated
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if super::document::is_document(&ext) {
                return super::document::extract_document(&validated);
            }

            let content = tokio::fs::read_to_string(&validated)
                .await
                .map_err(ToolError::Io)?;

            // Handle offset and limit for large files
            // Default to 2000 lines as per Claude Code behavior
            const DEFAULT_LINE_LIMIT: usize = 2000;

            let offset = params["offset"].as_u64().unwrap_or(0) as usize;
            let limit = params["limit"]
                .as_u64()
                .map(|l| l as usize)
                .unwrap_or(DEFAULT_LINE_LIMIT);

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            // Build output lines with line numbers
            let mut output_lines: Vec<String> = Vec::new();
            let mut token_count = 0;
            let mut truncated_by_tokens = false;

            for (i, line) in lines.into_iter().skip(offset).take(limit).enumerate() {
                let line_num = offset + i + 1;
                // Truncate lines longer than 2000 chars
                let truncated = if line.len() > 2000 {
                    format!("{}...", &line[..2000])
                } else {
                    line.to_string()
                };
                let formatted_line = format!("{:>6}\t{}", line_num, truncated);

                // Check token limit before adding
                let line_tokens = estimate_tokens(&formatted_line);
                if token_count + line_tokens > MAX_OUTPUT_TOKENS {
                    truncated_by_tokens = true;
                    break;
                }

                token_count += line_tokens;
                output_lines.push(formatted_line);
            }

            let formatted_content = output_lines.join("\n");
            let lines_returned = output_lines.len();
            let has_more = offset + lines_returned < total_lines || truncated_by_tokens;

            Ok(ToolOutput::success(json!({
                "content": formatted_content,
                "path": path_to_display(&validated),
                "total_lines": total_lines,
                "offset": offset,
                "lines_returned": lines_returned,
                "has_more": has_more
            })))
        })
    }
}
