//! WebFetch tool - fetch URLs and convert to markdown

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

/// Tool for fetching and processing web content
pub struct WebFetch;

impl WebFetch {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetch {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetches content from a specified URL. \
         Retrieves the page content and converts HTML to a readable text format. \
         Use this tool when you need to retrieve and analyze web content."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from. Must be a fully-formed valid URL."
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to run on the fetched content. Describes what information to extract."
                },
                "extract_text": {
                    "type": "boolean",
                    "description": "Extract main text content only (strip HTML tags). Default true.",
                    "default": true
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum content length to return. Default 50000 characters.",
                    "default": 50000
                }
            },
            "required": ["url", "prompt"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("url is required".into()))?;

        let prompt = params["prompt"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("prompt is required".into()))?;

        let extract_text = params["extract_text"].as_bool().unwrap_or(true);
        let max_length = params["max_length"].as_u64().unwrap_or(50000) as usize;

        // Validate URL
        let parsed_url = url::Url::parse(url)
            .map_err(|e| ToolError::InvalidParams(format!("Invalid URL: {}", e)))?;

        // Only allow http/https
        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(ToolError::InvalidParams(
                "Only HTTP and HTTPS URLs are supported".into(),
            ));
        }

        // Fetch the URL
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Cowork/1.0")
            .build()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create client: {}", e)))?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to fetch URL: {}", e)))?;

        // Check for redirect
        let final_url = response.url().to_string();
        let status = response.status();

        if !status.is_success() {
            return Err(ToolError::ExecutionFailed(format!(
                "HTTP error: {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {}", e)))?;

        // Process content
        let processed = if extract_text && content_type.contains("text/html") {
            extract_text_from_html(&body)
        } else {
            body
        };

        // Truncate if needed
        let truncated = processed.len() > max_length;
        let final_content = if truncated {
            processed.chars().take(max_length).collect()
        } else {
            processed
        };

        Ok(ToolOutput::success(json!({
            "content": final_content,
            "url": url,
            "final_url": final_url,
            "content_type": content_type,
            "truncated": truncated,
            "length": final_content.len(),
            "prompt": prompt,
            "note": "Use the content above to answer the prompt"
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}

/// Simple HTML to text extraction
fn extract_text_from_html(html: &str) -> String {
    // Remove script and style tags with content
    let mut result = html.to_string();

    // Remove script tags
    while let Some(start) = result.find("<script") {
        if let Some(end) = result[start..].find("</script>") {
            result = format!("{}{}", &result[..start], &result[start + end + 9..]);
        } else {
            break;
        }
    }

    // Remove style tags
    while let Some(start) = result.find("<style") {
        if let Some(end) = result[start..].find("</style>") {
            result = format!("{}{}", &result[..start], &result[start + end + 8..]);
        } else {
            break;
        }
    }

    // Remove all HTML tags
    let mut output = String::new();
    let mut in_tag = false;

    for c in result.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(c),
            _ => {}
        }
    }

    // Decode common HTML entities
    let output = output
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    // Normalize whitespace
    let lines: Vec<&str> = output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    lines.join("\n")
}
