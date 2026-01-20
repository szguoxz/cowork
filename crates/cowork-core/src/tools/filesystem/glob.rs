//! Glob file pattern matching tool

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::{path_to_display, path_to_glob_pattern};

/// Tool for fast file pattern matching using glob patterns
pub struct GlobFiles {
    workspace: PathBuf,
}

impl GlobFiles {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for GlobFiles {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Fast file pattern matching tool that works with any codebase size.\n\n\
         - Supports glob patterns like \"**/*.js\" or \"src/**/*.ts\"\n\
         - Returns matching file paths sorted by modification time\n\
         - Use this tool when you need to find files by name patterns\n\
         - When doing an open ended search that may require multiple rounds of globbing and grepping, use the Task tool instead\n\
         - You can call multiple tools in a single response. It is always better to speculatively perform multiple searches in parallel if they are potentially useful."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. IMPORTANT: Omit this field to use the default directory. DO NOT enter \"undefined\" or \"null\" - simply omit it for the default behavior. Must be a valid directory path if provided."
                }
            },
            "required": ["pattern"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let pattern = params["pattern"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("pattern is required".into()))?;

            let base_path = if let Some(path) = params["path"].as_str() {
                self.workspace.join(path)
            } else {
                self.workspace.clone()
            };

            let limit = params["limit"].as_u64().unwrap_or(100) as usize;

            // Construct full glob pattern with forward slashes (required by glob crate)
            let full_pattern = path_to_glob_pattern(&base_path.join(pattern));

            // Collect matching files with metadata
            let mut entries: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

            for path in glob::glob(&full_pattern)
                .map_err(|e| ToolError::InvalidParams(format!("Invalid glob pattern: {}", e)))?
                .flatten()
            {
                if path.is_file() {
                    let mtime = tokio::fs::metadata(&path)
                        .await
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    entries.push((path, mtime));
                }
            }

            // Sort by modification time (newest first)
            entries.sort_by(|a, b| b.1.cmp(&a.1));

            // Limit results
            let entries: Vec<_> = entries.into_iter().take(limit).collect();

            // Convert to relative paths with consistent forward slash separators
            let files: Vec<String> = entries
                .iter()
                .map(|(path, _)| {
                    path.strip_prefix(&self.workspace)
                        .map(path_to_display)
                        .unwrap_or_else(|_| path_to_display(path))
                })
                .collect();

            Ok(ToolOutput::success(json!({
                "files": files,
                "count": files.len(),
                "pattern": pattern
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
