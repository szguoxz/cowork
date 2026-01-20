//! Move/rename file tool

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::{normalize_path, path_to_display, validate_path};

/// Tool for moving or renaming files
pub struct MoveFile {
    workspace: PathBuf,
}

impl MoveFile {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for MoveFile {
    fn name(&self) -> &str {
        "move_file"
    }

    fn description(&self) -> &str {
        "Move or rename a file or directory."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Source path (relative to workspace)"
                },
                "destination": {
                    "type": "string",
                    "description": "Destination path (relative to workspace)"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Overwrite destination if it exists",
                    "default": false
                }
            },
            "required": ["source", "destination"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let source_str = params["source"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("source is required".into()))?;

            let dest_str = params["destination"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("destination is required".into()))?;

            let overwrite = params["overwrite"].as_bool().unwrap_or(false);

            let source = self.workspace.join(source_str);
            let dest = self.workspace.join(dest_str);

            let validated_source = validate_path(&source, &self.workspace)?;

            // Normalize destination path for security check
            let normalized_dest = normalize_path(&dest);
            let normalized_workspace = normalize_path(&self.workspace);

            // Security check: ensure destination is within workspace
            if !normalized_dest.starts_with(&normalized_workspace) {
                return Err(ToolError::PermissionDenied(format!(
                    "Destination {} is outside workspace",
                    dest.display()
                )));
            }

            // For destination, validate parent exists and is in workspace
            if let Some(parent) = dest.parent() {
                if parent.exists() {
                    validate_path(parent, &self.workspace)?;
                }
            }

            if dest.exists() && !overwrite {
                return Err(ToolError::ExecutionFailed(format!(
                    "Destination {} already exists",
                    dest.display()
                )));
            }

            tokio::fs::rename(&validated_source, &dest)
                .await
                .map_err(ToolError::Io)?;

            Ok(ToolOutput::success(json!({
                "source": path_to_display(&validated_source),
                "destination": path_to_display(&dest)
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}
