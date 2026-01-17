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
        let selector = params["selector"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("selector is required".into()))?;

        let _wait_nav = params["wait_for_navigation"].as_bool().unwrap_or(false);
        let _timeout = params["timeout"].as_u64().unwrap_or(5000);

        let mut session = self.session.lock().await;

        if !session.active {
            return Err(ToolError::ExecutionFailed(
                "No active browser session. Navigate to a URL first.".into(),
            ));
        }

        #[cfg(feature = "browser")]
        {
            let page = session.get_page().await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            // Find the element
            let element = page.find_element(selector)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Element not found: {}", e)))?;

            // Click the element
            element.click()
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Click failed: {}", e)))?;

            // Get current URL after click (may have changed)
            let new_url = page.url()
                .await
                .ok()
                .flatten();

            if let Some(url) = &new_url {
                session.current_url = Some(url.clone());
            }

            Ok(ToolOutput::success(json!({
                "selector": selector,
                "status": "clicked",
                "current_url": new_url
            })))
        }

        #[cfg(not(feature = "browser"))]
        {
            Ok(ToolOutput::success(json!({
                "selector": selector,
                "status": "simulated",
                "note": "Browser feature not enabled - simulation only"
            })))
        }
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}

/// Tool for typing text into input fields
pub struct TypeText {
    session: Arc<Mutex<BrowserSession>>,
}

impl TypeText {
    pub fn new(session: Arc<Mutex<BrowserSession>>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl Tool for TypeText {
    fn name(&self) -> &str {
        "browser_type"
    }

    fn description(&self) -> &str {
        "Type text into an input field on the page."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the input element"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type"
                },
                "clear_first": {
                    "type": "boolean",
                    "description": "Clear the input before typing",
                    "default": true
                }
            },
            "required": ["selector", "text"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let selector = params["selector"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("selector is required".into()))?;

        let text = params["text"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("text is required".into()))?;

        let _clear_first = params["clear_first"].as_bool().unwrap_or(true);

        let mut session = self.session.lock().await;

        if !session.active {
            return Err(ToolError::ExecutionFailed(
                "No active browser session. Navigate to a URL first.".into(),
            ));
        }

        #[cfg(feature = "browser")]
        {
            let page = session.get_page().await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            // Find the element
            let element = page.find_element(selector)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Element not found: {}", e)))?;

            // Type the text
            element.type_str(text)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Type failed: {}", e)))?;

            Ok(ToolOutput::success(json!({
                "selector": selector,
                "text_length": text.len(),
                "status": "typed"
            })))
        }

        #[cfg(not(feature = "browser"))]
        {
            Ok(ToolOutput::success(json!({
                "selector": selector,
                "text_length": text.len(),
                "status": "simulated",
                "note": "Browser feature not enabled - simulation only"
            })))
        }
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}

/// Tool for getting page content
pub struct GetPageContent {
    session: Arc<Mutex<BrowserSession>>,
}

impl GetPageContent {
    pub fn new(session: Arc<Mutex<BrowserSession>>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl Tool for GetPageContent {
    fn name(&self) -> &str {
        "browser_get_content"
    }

    fn description(&self) -> &str {
        "Get the HTML content or text content of the current page or a specific element."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for specific element (optional, defaults to body)"
                },
                "format": {
                    "type": "string",
                    "description": "Output format: 'html' or 'text'",
                    "default": "text"
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let selector = params["selector"].as_str().unwrap_or("body");
        let format = params["format"].as_str().unwrap_or("text");

        let mut session = self.session.lock().await;

        if !session.active {
            return Err(ToolError::ExecutionFailed(
                "No active browser session. Navigate to a URL first.".into(),
            ));
        }

        #[cfg(feature = "browser")]
        {
            let page = session.get_page().await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let element = page.find_element(selector)
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Element not found: {}", e)))?;

            let content = if format == "html" {
                element.inner_html()
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get HTML: {}", e)))?
                    .unwrap_or_default()
            } else {
                element.inner_text()
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get text: {}", e)))?
                    .unwrap_or_default()
            };

            Ok(ToolOutput::success(json!({
                "selector": selector,
                "format": format,
                "content": content,
                "content_length": content.len()
            })))
        }

        #[cfg(not(feature = "browser"))]
        {
            Ok(ToolOutput::success(json!({
                "selector": selector,
                "format": format,
                "content": "Browser feature not enabled - simulation only",
                "status": "simulated"
            })))
        }
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
