//! Navigate to URL tool

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

use super::BrowserSession;

/// Tool for navigating to URLs
pub struct NavigateTo {
    session: Arc<Mutex<BrowserSession>>,
}

impl NavigateTo {
    pub fn new(session: Arc<Mutex<BrowserSession>>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl Tool for NavigateTo {
    fn name(&self) -> &str {
        "browser_navigate"
    }

    fn description(&self) -> &str {
        "Navigate the browser to a specified URL."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to navigate to"
                },
                "wait_for": {
                    "type": "string",
                    "description": "Wait condition: 'load', 'domcontentloaded', or 'networkidle'",
                    "default": "load"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("url is required".into()))?;

        let _wait_for = params["wait_for"].as_str().unwrap_or("load");

        // TODO: Implement actual browser navigation using chromiumoxide
        // For now, update session state
        let mut session = self.session.lock().await;
        session.current_url = Some(url.to_string());
        session.active = true;

        Ok(ToolOutput::success(json!({
            "url": url,
            "status": "navigated"
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}
