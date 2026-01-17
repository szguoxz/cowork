//! WebSearch tool - Search the web for information
//!
//! Provides web search capability using search APIs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

/// Search result from web search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Configuration for web search
#[derive(Debug, Clone)]
pub struct WebSearchConfig {
    /// API endpoint for search (e.g., SearXNG, Brave, etc.)
    pub api_endpoint: Option<String>,
    /// API key if required
    pub api_key: Option<String>,
    /// Maximum results to return
    pub max_results: usize,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            api_endpoint: None,
            api_key: None,
            max_results: 10,
        }
    }
}

/// Tool for searching the web
pub struct WebSearch {
    config: WebSearchConfig,
}

impl WebSearch {
    pub fn new() -> Self {
        Self {
            config: WebSearchConfig::default(),
        }
    }

    pub fn with_config(config: WebSearchConfig) -> Self {
        Self { config }
    }

    /// Perform the actual search
    async fn search(
        &self,
        query: &str,
        allowed_domains: &[String],
        blocked_domains: &[String],
    ) -> Result<Vec<SearchResult>, String> {
        // If no API endpoint configured, return a helpful message
        if self.config.api_endpoint.is_none() {
            return Ok(vec![SearchResult {
                title: "Web Search Not Configured".to_string(),
                url: "".to_string(),
                snippet: format!(
                    "Web search is not configured. To enable, set up a search API endpoint. \
                     Query was: {}",
                    query
                ),
            }]);
        }

        let endpoint = self.config.api_endpoint.as_ref().unwrap();

        // Build search URL with domain filters
        let mut search_query = query.to_string();

        // Add site restrictions for allowed domains
        if !allowed_domains.is_empty() {
            let site_filter = allowed_domains
                .iter()
                .map(|d| format!("site:{}", d))
                .collect::<Vec<_>>()
                .join(" OR ");
            search_query = format!("({}) {}", site_filter, search_query);
        }

        // Add negative site filters for blocked domains
        for domain in blocked_domains {
            search_query = format!("{} -site:{}", search_query, domain);
        }

        // Make HTTP request to search API
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let mut request = client.get(endpoint).query(&[
            ("q", search_query.as_str()),
            ("format", "json"),
            ("count", &self.config.max_results.to_string()),
        ]);

        // Add API key if configured
        if let Some(api_key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Search request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Search API returned error: {}", response.status()));
        }

        // Parse response - this depends on the search API format
        // For SearXNG-style response:
        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse search response: {}", e))?;

        let mut results = Vec::new();

        if let Some(items) = body.get("results").and_then(|r| r.as_array()) {
            for item in items.iter().take(self.config.max_results) {
                let title = item
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("No title")
                    .to_string();
                let url = item
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                let snippet = item
                    .get("content")
                    .or_else(|| item.get("snippet"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();

                results.push(SearchResult {
                    title,
                    url,
                    snippet,
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

#[async_trait]
impl Tool for WebSearch {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for up-to-date information. Returns search results with titles, URLs, \
         and snippets. Use for current events, documentation, or information beyond the knowledge cutoff."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query to use",
                    "minLength": 2
                },
                "allowed_domains": {
                    "type": "array",
                    "description": "Only include results from these domains",
                    "items": {
                        "type": "string"
                    }
                },
                "blocked_domains": {
                    "type": "array",
                    "description": "Never include results from these domains",
                    "items": {
                        "type": "string"
                    }
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
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
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let blocked_domains: Vec<String> = params
            .get("blocked_domains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        match self.search(query, &allowed_domains, &blocked_domains).await {
            Ok(results) => Ok(ToolOutput::success(json!({
                "query": query,
                "results": results,
                "count": results.len()
            }))),
            Err(e) => Err(ToolError::ExecutionFailed(e)),
        }
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Low
    }
}
