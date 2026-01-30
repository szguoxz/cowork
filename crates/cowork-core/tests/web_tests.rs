//! Web tool tests
//!
//! Tests for WebFetch and WebSearch tools.

use cowork_core::tools::{Tool, ToolExecutionContext};
use cowork_core::tools::web::{WebFetch, WebSearch};
use serde_json::json;

fn test_ctx() -> ToolExecutionContext {
    ToolExecutionContext::standalone("test", "test")
}

mod web_fetch_tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_valid_url() {
        let tool = WebFetch::new();

        let result = tool.execute(json!({
            "url": "https://httpbin.org/html",
            "prompt": "Extract the main text content"
        }), test_ctx()).await;

        assert!(result.is_ok(), "WebFetch failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_fetch_json_api() {
        let tool = WebFetch::new();

        let result = tool.execute(json!({
            "url": "https://httpbin.org/json",
            "prompt": "Parse the JSON response",
            "extract_text": false
        }), test_ctx()).await;

        assert!(result.is_ok(), "JSON fetch failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_fetch_invalid_url() {
        let tool = WebFetch::new();

        let result = tool.execute(json!({
            "url": "not-a-valid-url",
            "prompt": "test"
        }), test_ctx()).await;

        assert!(result.is_err(), "Should fail for invalid URL");
    }

    #[tokio::test]
    async fn test_fetch_non_http_scheme() {
        let tool = WebFetch::new();

        let result = tool.execute(json!({
            "url": "ftp://example.com/file",
            "prompt": "test"
        }), test_ctx()).await;

        assert!(result.is_err(), "Should reject non-HTTP URLs");
    }

    #[tokio::test]
    async fn test_fetch_with_max_length() {
        let tool = WebFetch::new();

        let result = tool.execute(json!({
            "url": "https://httpbin.org/html",
            "prompt": "Extract content",
            "max_length": 100
        }), test_ctx()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }
}

mod web_search_tests {
    use super::*;

    #[test]
    fn test_search_tool_schema() {
        let tool = WebSearch::new();

        let schema = tool.parameters_schema();
        assert!(schema["properties"]["query"].is_object());
    }

    #[tokio::test]
    async fn test_search_requires_query() {
        let tool = WebSearch::new();

        let result = tool.execute(json!({}), test_ctx()).await;
        assert!(result.is_err(), "Should require query parameter");
    }
}
