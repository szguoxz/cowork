//! WebSearch tool - Search the web using SerpAPI
//!
//! Provides web search capability via SerpAPI.
//! For providers with native web search (Anthropic, OpenAI, Groq, xAI, Gemini, Cohere),
//! native search is preferred. For others, this tool uses SerpAPI.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::approval::ApprovalLevel;
use crate::config::WebSearchConfig;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// Search result from web search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Providers that support native web search
pub const NATIVE_SEARCH_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "perplexity",
    "gemini",
    "google",
    "cohere",
    "groq",
    "xai",
    "grok",
];

/// Check if a provider supports native web search
pub fn supports_native_search(provider_type: &str) -> bool {
    NATIVE_SEARCH_PROVIDERS.contains(&provider_type.to_lowercase().as_str())
}

/// Tool for searching the web using SerpAPI
pub struct WebSearch {
    config: WebSearchConfig,
    provider_type: Option<String>,
}

impl WebSearch {
    pub fn new() -> Self {
        Self {
            config: WebSearchConfig::default(),
            provider_type: None,
        }
    }

    pub fn with_config(config: WebSearchConfig) -> Self {
        Self {
            config,
            provider_type: None,
        }
    }

    pub fn with_provider(mut self, provider_type: impl Into<String>) -> Self {
        self.provider_type = Some(provider_type.into());
        self
    }

    /// Check if this instance should use native search
    pub fn should_use_native(&self) -> bool {
        self.provider_type
            .as_ref()
            .map(|p| supports_native_search(p))
            .unwrap_or(false)
    }

    /// Perform search using SerpAPI
    async fn search(
        &self,
        query: &str,
        allowed_domains: &[String],
        blocked_domains: &[String],
    ) -> Result<Vec<SearchResult>, String> {
        let api_key = self.config.get_api_key()
            .ok_or_else(|| {
                "SerpAPI key not configured. Set SERPAPI_API_KEY environment variable or api_key in [web_search] config.".to_string()
            })?;

        // Build search query with domain filters
        let mut search_query = query.to_string();

        if !allowed_domains.is_empty() {
            let site_filter = allowed_domains
                .iter()
                .map(|d| format!("site:{}", d))
                .collect::<Vec<_>>()
                .join(" OR ");
            search_query = format!("({}) {}", site_filter, search_query);
        }

        for domain in blocked_domains {
            search_query = format!("{} -site:{}", search_query, domain);
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let response = client
            .get("https://serpapi.com/search")
            .query(&[
                ("q", search_query.as_str()),
                ("api_key", api_key.as_str()),
                ("engine", "google"),
                ("num", &self.config.max_results.to_string()),
            ])
            .send()
            .await
            .map_err(|e| format!("SerpAPI search failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("SerpAPI error: {}", response.status()));
        }

        let body: Value = response.json().await
            .map_err(|e| format!("Failed to parse SerpAPI response: {}", e))?;

        let mut results = Vec::new();
        if let Some(organic) = body.get("organic_results").and_then(|o| o.as_array()) {
            for item in organic.iter().take(self.config.max_results) {
                results.push(SearchResult {
                    title: item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                    url: item.get("link").and_then(|l| l.as_str()).unwrap_or("").to_string(),
                    snippet: item.get("snippet").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                });
            }
        }

        Ok(results)
    }
}

impl Default for WebSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for WebSearch {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::WEBSEARCH
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query",
                    "minLength": 2
                },
                "allowed_domains": {
                    "type": "array",
                    "description": "Only include results from these domains",
                    "items": { "type": "string" }
                },
                "blocked_domains": {
                    "type": "array",
                    "description": "Exclude results from these domains",
                    "items": { "type": "string" }
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let query = params["query"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("query is required".into()))?;

            if query.len() < 2 {
                return Err(ToolError::InvalidParams(
                    "query must be at least 2 characters".into(),
                ));
            }

            let allowed_domains: Vec<String> = params
                .get("allowed_domains")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let blocked_domains: Vec<String> = params
                .get("blocked_domains")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            // Use SerpAPI for search
            match self.search(query, &allowed_domains, &blocked_domains).await {
                Ok(results) => {
                    let count = results.len();
                    Ok(ToolOutput::success(json!({
                        "query": query,
                        "results": results,
                        "count": count,
                        "provider": "serpapi"
                    })))
                }
                Err(e) => Err(ToolError::ExecutionFailed(e)),
            }
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_search_providers() {
        assert!(supports_native_search("anthropic"));
        assert!(supports_native_search("openai"));
        assert!(supports_native_search("groq"));
        assert!(supports_native_search("xai"));
        assert!(supports_native_search("gemini"));
        assert!(supports_native_search("cohere"));
        assert!(supports_native_search("perplexity"));

        assert!(!supports_native_search("deepseek"));
        assert!(!supports_native_search("ollama"));
        assert!(!supports_native_search("together"));
    }

    #[test]
    fn test_config_default() {
        let config = WebSearchConfig::default();
        assert!(config.api_key.is_none());
        assert_eq!(config.max_results, 10);
    }
}
