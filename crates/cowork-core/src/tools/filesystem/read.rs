//! Read file tool

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

use super::validate_path;

/// Tool for reading file contents
pub struct ReadFile {
    workspace: PathBuf,
}

impl ReadFile {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns the file content as text."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read (relative to workspace)"
                },
                "encoding": {
                    "type": "string",
                    "description": "Text encoding (default: utf-8)",
                    "default": "utf-8"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

        let path = self.workspace.join(path_str);
        let validated = validate_path(&path, &self.workspace)?;

        let content = tokio::fs::read_to_string(&validated)
            .await
            .map_err(ToolError::Io)?;

        Ok(ToolOutput::success(json!({
            "content": content,
            "path": validated.display().to_string(),
            "size": content.len()
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
