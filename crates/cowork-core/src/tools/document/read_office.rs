//! Office document reading tool

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};
use crate::tools::filesystem::validate_path;

use super::DocumentFormat;

/// Tool for reading Office documents (Word, Excel, PowerPoint)
pub struct ReadOfficeDoc {
    workspace: PathBuf,
}

impl ReadOfficeDoc {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for ReadOfficeDoc {
    fn name(&self) -> &str {
        "read_office_doc"
    }

    fn description(&self) -> &str {
        "Extract content from Office documents (Word, Excel, PowerPoint)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the Office document"
                },
                "extract_images": {
                    "type": "boolean",
                    "description": "Also extract embedded images",
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

        let _extract_images = params["extract_images"].as_bool().unwrap_or(false);

        let path = self.workspace.join(path_str);
        let validated = validate_path(&path, &self.workspace)?;

        let ext = validated
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let format = DocumentFormat::from_extension(ext);

        // TODO: Implement actual Office document parsing
        // For now, return placeholder

        Ok(ToolOutput::success(json!({
            "path": validated.display().to_string(),
            "format": format!("{:?}", format),
            "content": "Office document extraction not yet implemented"
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
