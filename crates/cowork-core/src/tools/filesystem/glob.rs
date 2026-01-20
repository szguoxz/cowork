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
        "glob"
    }

    fn description(&self) -> &str {
        "Fast file pattern matching tool that works with any codebase size. \
         Supports glob patterns like '**/*.js' or 'src/**/*.ts'. \
         Returns matching file paths sorted by modification time (newest first)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g., '**/*.rs', 'src/**/*.ts')"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Defaults to workspace root."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of files to return. Defaults to 100.",
                    "default": 100
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
