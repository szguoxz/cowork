//! Read file tool

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::{path_to_display, validate_path};

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
        "Reads a file from the local filesystem. You can access any file directly by using this tool.\n\n\
         Usage:\n\
         - The file_path parameter can be absolute or relative to workspace\n\
         - By default, it reads the entire file\n\
         - You can optionally specify offset and limit for large files\n\
         - Any lines longer than 2000 characters will be truncated\n\
         - Results are returned with line numbers starting at 1\n\
         - This tool can read images (PNG, JPG, etc.) - they are presented visually\n\
         - This tool can read PDF files and Jupyter notebooks (.ipynb)\n\
         - This tool can only read files, not directories. Use Bash with ls for directories.\n\
         - You can call multiple tools in a single response to read multiple files in parallel."
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

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path_str = params["file_path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("file_path is required".into()))?;

            let path = self.workspace.join(path_str);
            let validated = validate_path(&path, &self.workspace)?;

            let content = tokio::fs::read_to_string(&validated)
                .await
                .map_err(ToolError::Io)?;

            // Handle offset and limit for large files
            let offset = params["offset"].as_u64().unwrap_or(0) as usize;
            let limit = params["limit"].as_u64().map(|l| l as usize);

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            let output_lines: Vec<String> = lines
                .into_iter()
                .skip(offset)
                .take(limit.unwrap_or(usize::MAX))
                .enumerate()
                .map(|(i, line)| {
                    let line_num = offset + i + 1;
                    // Truncate lines longer than 2000 chars
                    let truncated = if line.len() > 2000 {
                        format!("{}...", &line[..2000])
                    } else {
                        line.to_string()
                    };
                    format!("{:>6}\t{}", line_num, truncated)
                })
                .collect();

            let formatted_content = output_lines.join("\n");

            Ok(ToolOutput::success(json!({
                "content": formatted_content,
                "path": path_to_display(&validated),
                "total_lines": total_lines,
                "offset": offset,
                "lines_returned": output_lines.len()
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
