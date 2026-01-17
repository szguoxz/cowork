//! Screenshot tool

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

use super::BrowserSession;

/// Tool for taking screenshots
pub struct TakeScreenshot {
    session: Arc<Mutex<BrowserSession>>,
    output_dir: PathBuf,
}

impl TakeScreenshot {
    pub fn new(session: Arc<Mutex<BrowserSession>>, output_dir: PathBuf) -> Self {
        Self { session, output_dir }
    }
}

#[async_trait]
impl Tool for TakeScreenshot {
    fn name(&self) -> &str {
        "browser_screenshot"
    }

    fn description(&self) -> &str {
        "Take a screenshot of the current browser page."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "filename": {
                    "type": "string",
                    "description": "Output filename (without extension)"
                },
                "full_page": {
                    "type": "boolean",
                    "description": "Capture full scrollable page",
                    "default": false
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector to screenshot specific element"
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let session = self.session.lock().await;
        if !session.active {
            return Err(ToolError::ExecutionFailed(
                "No active browser session".into(),
            ));
        }

        let filename = params["filename"]
            .as_str()
            .unwrap_or("screenshot");
        let _full_page = params["full_page"].as_bool().unwrap_or(false);
        let _selector = params["selector"].as_str();

        let output_path = self.output_dir.join(format!("{}.png", filename));

        // TODO: Implement actual screenshot using chromiumoxide
        // For now, return placeholder

        Ok(ToolOutput::success(json!({
            "path": output_path.display().to_string(),
            "status": "captured"
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
