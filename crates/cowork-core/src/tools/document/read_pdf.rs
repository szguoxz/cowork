//! PDF reading tool


use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};
use crate::tools::filesystem::validate_path;

/// Tool for reading PDF documents
pub struct ReadPdf {
    workspace: PathBuf,
}

impl ReadPdf {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}


impl Tool for ReadPdf {
    fn name(&self) -> &str {
        "read_pdf"
    }

    fn description(&self) -> &str {
        "Extract text content from a PDF file."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the PDF file"
                },
                "pages": {
                    "type": "string",
                    "description": "Page range to extract (e.g., '1-5', 'all')",
                    "default": "all"
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

        let _pages = params["pages"].as_str().unwrap_or("all");

        let path = self.workspace.join(path_str);
        let validated = validate_path(&path, &self.workspace)?;

        // TODO: Implement actual PDF parsing using pdf-extract or similar
        // For now, return placeholder

        Ok(ToolOutput::success(json!({
            "path": validated.display().to_string(),
            "text": "PDF content extraction not yet implemented",
            "pages": 0
        })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
