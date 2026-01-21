//! Navigate to URL tool


use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

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


impl Tool for NavigateTo {
    fn name(&self) -> &str {
        "BrowserNavigate"
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

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("url is required".into()))?;

        let _wait_for = params["wait_for"].as_str().unwrap_or("load");

        let mut session = self.session.lock().await;

        #[cfg(feature = "browser")]
        {
            // Ensure browser is started and get page
            session.ensure_browser().await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let page = session.get_page().await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            // Navigate to URL
            page.goto(url)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Navigation failed: {}", e)))?;

            // Get page info
            let title = page.get_title()
                .await
                .ok()
                .flatten();

            session.current_url = Some(url.to_string());
            session.title = title.clone();

            Ok(ToolOutput::success(json!({
                "url": url,
                "title": title,
                "status": "navigated"
            })))
        }

        #[cfg(not(feature = "browser"))]
        {
            // Fallback without browser feature
            session.current_url = Some(url.to_string());
            session.active = true;

            Ok(ToolOutput::success(json!({
                "url": url,
                "status": "navigated",
                "note": "Browser feature not enabled - simulation only"
            })))
        }
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}
