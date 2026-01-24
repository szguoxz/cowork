//! WebSearch tool - Search the web for information
//!
//! Provides web search capability with support for multiple search providers.
//! For providers with native web search (Anthropic, OpenAI, Groq, xAI, Gemini, Cohere),
//! native search is preferred. For others, falls back to external search APIs.

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

/// Tool for searching the web
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

    /// Perform search using fallback provider
    async fn search_fallback(
        &self,
        query: &str,
        allowed_domains: &[String],
        blocked_domains: &[String],
    ) -> Result<Vec<SearchResult>, String> {
        let endpoint = self.config.get_fallback_endpoint()
            .ok_or_else(|| "No search endpoint configured. Set fallback_endpoint in web_search config.".to_string())?;

        let api_key = self.config.get_fallback_api_key();

        // Check if API key is required but missing
        if self.config.fallback_provider != "searxng" && api_key.is_none() {
            return Err(format!(
                "No API key configured for {} search. Set fallback_api_key or {} environment variable.",
                self.config.fallback_provider,
                self.config.fallback_api_key_env.as_deref().unwrap_or("BRAVE_API_KEY")
            ));
        }

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

        // Build request based on provider
        let response = match self.config.fallback_provider.as_str() {
            "brave" => self.search_brave(&client, &endpoint, &search_query, api_key.as_deref()).await?,
            "serper" => self.search_serper(&client, &endpoint, &search_query, api_key.as_deref()).await?,
            "tavily" => self.search_tavily(&client, &endpoint, &search_query, api_key.as_deref()).await?,
            "searxng" => self.search_searxng(&client, &endpoint, &search_query).await?,
            _ => return Err(format!("Unknown search provider: {}", self.config.fallback_provider)),
        };

        Ok(response)
    }

    async fn search_brave(
        &self,
        client: &reqwest::Client,
        endpoint: &str,
        query: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<SearchResult>, String> {
        let api_key = api_key.ok_or("Brave API key required")?;

        let response = client
            .get(endpoint)
            .header("X-Subscription-Token", api_key)
            .query(&[
                ("q", query),
                ("count", &self.config.max_results.to_string()),
            ])
            .send()
            .await
            .map_err(|e| format!("Brave search failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Brave API error: {}", response.status()));
        }

        let body: Value = response.json().await
            .map_err(|e| format!("Failed to parse Brave response: {}", e))?;

        let mut results = Vec::new();
        if let Some(web) = body.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()) {
            for item in web.iter().take(self.config.max_results) {
                results.push(SearchResult {
                    title: item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                    url: item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                    snippet: item.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                });
            }
        }

        Ok(results)
    }

    async fn search_serper(
        &self,
        client: &reqwest::Client,
        endpoint: &str,
        query: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<SearchResult>, String> {
        let api_key = api_key.ok_or("Serper API key required")?;

        let response = client
            .post(endpoint)
            .header("X-API-KEY", api_key)
            .header("Content-Type", "application/json")
            .json(&json!({
                "q": query,
                "num": self.config.max_results
            }))
            .send()
            .await
            .map_err(|e| format!("Serper search failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Serper API error: {}", response.status()));
        }

        let body: Value = response.json().await
            .map_err(|e| format!("Failed to parse Serper response: {}", e))?;

        let mut results = Vec::new();
        if let Some(organic) = body.get("organic").and_then(|o| o.as_array()) {
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

    async fn search_tavily(
        &self,
        client: &reqwest::Client,
        endpoint: &str,
        query: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<SearchResult>, String> {
        let api_key = api_key.ok_or("Tavily API key required")?;

        let response = client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .json(&json!({
                "api_key": api_key,
                "query": query,
                "max_results": self.config.max_results,
                "include_answer": false
            }))
            .send()
            .await
            .map_err(|e| format!("Tavily search failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Tavily API error: {}", response.status()));
        }

        let body: Value = response.json().await
            .map_err(|e| format!("Failed to parse Tavily response: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = body.get("results").and_then(|r| r.as_array()) {
            for item in items.iter().take(self.config.max_results) {
                results.push(SearchResult {
                    title: item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                    url: item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                    snippet: item.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string(),
                });
            }
        }

        Ok(results)
    }

    async fn search_searxng(
        &self,
        client: &reqwest::Client,
        endpoint: &str,
        query: &str,
    ) -> Result<Vec<SearchResult>, String> {
        let response = client
            .get(endpoint)
            .query(&[
                ("q", query),
                ("format", "json"),
                ("categories", "general"),
            ])
            .send()
            .await
            .map_err(|e| format!("SearXNG search failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("SearXNG error: {}", response.status()));
        }

        let body: Value = response.json().await
            .map_err(|e| format!("Failed to parse SearXNG response: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = body.get("results").and_then(|r| r.as_array()) {
            for item in items.iter().take(self.config.max_results) {
                results.push(SearchResult {
                    title: item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                    url: item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                    snippet: item.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string(),
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

            // For now, always use fallback search
            // Native search integration will be handled at the provider level
            match self.search_fallback(query, &allowed_domains, &blocked_domains).await {
                Ok(results) => {
                    let count = results.len();
                    Ok(ToolOutput::success(json!({
                        "query": query,
                        "results": results,
                        "count": count,
                        "provider": self.config.fallback_provider
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
    fn test_default_endpoints() {
        let config = WebSearchConfig::default();
        assert!(config.get_fallback_endpoint().is_some());
        assert!(config.get_fallback_endpoint().unwrap().contains("brave"));
    }
}
