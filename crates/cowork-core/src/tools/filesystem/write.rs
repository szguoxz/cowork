//! Write file tool

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::{normalize_path, path_to_display, validate_path};

/// Tool for writing file contents
pub struct WriteFile {
    workspace: PathBuf,
}

impl WriteFile {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for WriteFile {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Writes a file to the local filesystem.\n\n\
         Usage:\n\
         - This tool will overwrite the existing file if there is one at the provided path.\n\
         - If this is an existing file, you MUST use the Read tool first to read the file's contents. This tool will fail if you did not read the file first.\n\
         - ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.\n\
         - NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.\n\
         - Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path_str = params["file_path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("file_path is required".into()))?;

            let content = params["content"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("content is required".into()))?;

            let create_dirs = params["create_dirs"].as_bool().unwrap_or(true);

            let path = self.workspace.join(path_str);

            // Normalize the path to resolve .. components for security check
            let normalized_path = normalize_path(&path);
            let normalized_workspace = normalize_path(&self.workspace);

            // Security check: ensure normalized path is within workspace
            if !normalized_path.starts_with(&normalized_workspace) {
                return Err(ToolError::PermissionDenied(format!(
                    "Path {} is outside workspace",
                    path.display()
                )));
            }

            // For new files, validate parent directory
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    if parent.exists() {
                        validate_path(parent, &self.workspace)?;
                    } else if create_dirs {
                        tokio::fs::create_dir_all(parent).await.map_err(ToolError::Io)?;
                    }
                }
            } else {
                validate_path(&path, &self.workspace)?;
            }

            tokio::fs::write(&path, content).await.map_err(ToolError::Io)?;

            Ok(ToolOutput::success(json!({
                "path": path_to_display(&path),
                "bytes_written": content.len()
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}
