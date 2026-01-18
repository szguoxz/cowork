//! Screenshot tool


use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::BrowserSession;

/// Tool for taking screenshots
pub struct TakeScreenshot {
    session: Arc<Mutex<BrowserSession>>,
}

impl TakeScreenshot {
    pub fn new(session: Arc<Mutex<BrowserSession>>) -> Self {
        Self { session }
    }
}


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
                "path": {
                    "type": "string",
                    "description": "Output file path for the screenshot"
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
            },
            "required": ["path"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let output_path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
        let full_page = params["full_page"].as_bool().unwrap_or(false);
        let selector = params["selector"].as_str();

        let mut session = self.session.lock().await;

        if !session.active {
            return Err(ToolError::ExecutionFailed(
                "No active browser session. Navigate to a URL first.".into(),
            ));
        }

        #[cfg(feature = "browser")]
        {
            use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;

            let page = session.get_page().await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let screenshot_data = if let Some(sel) = selector {
                // Screenshot specific element
                let element = page.find_element(sel)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Element not found: {}", e)))?;

                element.screenshot(CaptureScreenshotFormat::Png)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Screenshot failed: {}", e)))?
            } else if full_page {
                // Full page screenshot
                page.screenshot(
                    chromiumoxide::page::ScreenshotParams::builder()
                        .full_page(true)
                        .format(CaptureScreenshotFormat::Png)
                        .build()
                )
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Screenshot failed: {}", e)))?
            } else {
                // Viewport screenshot
                page.screenshot(
                    chromiumoxide::page::ScreenshotParams::builder()
                        .format(CaptureScreenshotFormat::Png)
                        .build()
                )
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Screenshot failed: {}", e)))?
            };

            // Write to file
            tokio::fs::write(&output_path, &screenshot_data)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to save screenshot: {}", e)))?;

            Ok(ToolOutput::success(json!({
                "path": output_path,
                "size_bytes": screenshot_data.len(),
                "status": "captured"
            })))
        }

        #[cfg(not(feature = "browser"))]
        {
            // Fallback without browser feature
            Ok(ToolOutput::success(json!({
                "path": output_path,
                "status": "simulated",
                "note": "Browser feature not enabled - simulation only"
            })))
        }
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
