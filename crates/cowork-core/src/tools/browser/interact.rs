//! Browser interaction tools

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

use super::BrowserSession;

/// Tool for clicking elements
pub struct ClickElement {
    session: Arc<Mutex<BrowserSession>>,
}

impl ClickElement {
    pub fn new(session: Arc<Mutex<BrowserSession>>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl Tool for ClickElement {
    fn name(&self) -> &str {
        "browser_click"
    }

    fn description(&self) -> &str {
        "Click an element on the page using a CSS selector."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the element to click"
                },
                "wait_for_navigation": {
                    "type": "boolean",
                    "description": "Wait for navigation after click",
                    "default": false
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds",
                    "default": 5000
                }
            },
            "required": ["selector"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let session = self.session.lock().await;
        if !session.active {
            return Err(ToolError::ExecutionFailed(
                "No active browser session".into(),
            ));
        }

        let selector = params["selector"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("selector is required".into()))?;

        let _wait_nav = params["wait_for_navigation"].as_bool().unwrap_or(false);
        let _timeout = params["timeout"].as_u64().unwrap_or(5000);

        // TODO: Implement actual click using chromiumoxide

        Ok(ToolOutput::success(json!({
            "selector": selector,
            "status": "clicked"
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}
