//! Delete file tool

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

use super::validate_path;

/// Tool for deleting files and directories
pub struct DeleteFile {
    workspace: PathBuf,
}

impl DeleteFile {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for DeleteFile {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &str {
        "Delete a file or directory. Use with caution - this operation cannot be undone."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file or directory to delete"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "For directories, delete contents recursively",
                    "default": false
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

        let recursive = params["recursive"].as_bool().unwrap_or(false);

        let path = self.workspace.join(path_str);
        let validated = validate_path(&path, &self.workspace)?;

        let metadata = tokio::fs::metadata(&validated).await.map_err(ToolError::Io)?;

        if metadata.is_dir() {
            if recursive {
                tokio::fs::remove_dir_all(&validated).await.map_err(ToolError::Io)?;
            } else {
                tokio::fs::remove_dir(&validated).await.map_err(ToolError::Io)?;
            }
        } else {
            tokio::fs::remove_file(&validated).await.map_err(ToolError::Io)?;
        }

        Ok(ToolOutput::success(json!({
            "deleted": validated.display().to_string(),
            "was_directory": metadata.is_dir()
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::High
    }
}
