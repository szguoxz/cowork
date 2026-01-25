//! Provider catalog — single source of truth for all provider configuration
//!
//! Loads provider data from embedded JSON at compile time.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Embedded JSON data
const PROVIDERS_JSON: &str = include_str!("providers.json");

/// Model tier (fast, balanced, powerful)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelTier {
    Fast,
    Balanced,
    Powerful,
}

/// Model information
#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub context: usize,
}

/// Provider information
#[derive(Debug, Clone)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key_env: Option<String>,
    pub native_web_search: bool,
    pub models: HashMap<ModelTier, Model>,
}

impl Provider {
    /// Get the default model (balanced tier)
    pub fn default_model(&self) -> &Model {
        self.models.get(&ModelTier::Balanced).unwrap_or_else(|| {
            self.models.values().next().expect("provider has no models")
        })
    }

    /// Get model by tier
    pub fn model(&self, tier: ModelTier) -> Option<&Model> {
        self.models.get(&tier)
    }

    /// Get model ID for a tier (falls back to balanced)
    pub fn model_id(&self, tier: ModelTier) -> &str {
        self.models
            .get(&tier)
            .or_else(|| self.models.get(&ModelTier::Balanced))
            .map(|m| m.id.as_str())
            .unwrap_or("")
    }
}

/// Raw JSON structure for deserialization
#[derive(Deserialize)]
struct RawCatalog {
    providers: HashMap<String, RawProvider>,
}

#[derive(Deserialize)]
struct RawProvider {
    name: String,
    base_url: String,
    api_key_env: Option<String>,
    native_web_search: bool,
    models: RawModels,
}

#[derive(Deserialize)]
struct RawModels {
    fast: Model,
    balanced: Model,
    powerful: Model,
}

/// Global provider catalog
static CATALOG: LazyLock<HashMap<String, Provider>> = LazyLock::new(|| {
    let raw: RawCatalog = serde_json::from_str(PROVIDERS_JSON)
        .expect("failed to parse providers.json");

    raw.providers
        .into_iter()
        .map(|(id, raw)| {
            let mut models = HashMap::new();
            models.insert(ModelTier::Fast, raw.models.fast);
            models.insert(ModelTier::Balanced, raw.models.balanced);
            models.insert(ModelTier::Powerful, raw.models.powerful);

            let provider = Provider {
                id: id.clone(),
                name: raw.name,
                base_url: raw.base_url,
                api_key_env: raw.api_key_env,
                native_web_search: raw.native_web_search,
                models,
            };
            (id, provider)
        })
        .collect()
});

/// Get a provider by ID
pub fn get(provider_id: &str) -> Option<&'static Provider> {
    CATALOG.get(provider_id)
}

/// Get all providers
pub fn all() -> impl Iterator<Item = &'static Provider> {
    CATALOG.values()
}

/// Get all provider IDs
pub fn ids() -> impl Iterator<Item = &'static str> {
    CATALOG.keys().map(|s: &String| s.as_str())
}

/// Check if a provider has native web search
pub fn has_native_search(provider_id: &str) -> bool {
    get(provider_id).map(|p| p.native_web_search).unwrap_or(false)
}

/// Get base URL for a provider
pub fn base_url(provider_id: &str) -> Option<&'static str> {
    get(provider_id).map(|p| p.base_url.as_str())
}

/// Get API key environment variable for a provider
pub fn api_key_env(provider_id: &str) -> Option<&'static str> {
    get(provider_id).and_then(|p| p.api_key_env.as_deref())
}

/// Get default model ID for a provider
pub fn default_model(provider_id: &str) -> Option<&'static str> {
    get(provider_id).map(|p| p.default_model().id.as_str())
}

/// Get model ID for a provider and tier
pub fn model_id(provider_id: &str, tier: ModelTier) -> Option<&'static str> {
    get(provider_id).map(|p| p.model_id(tier))
}

/// Get context window for a provider's default model
pub fn context_window(provider_id: &str) -> Option<usize> {
    get(provider_id).map(|p| p.default_model().context)
}

/// Get context window for a specific model tier
pub fn context_window_for_tier(provider_id: &str, tier: ModelTier) -> Option<usize> {
    get(provider_id).and_then(|p| p.model(tier).map(|m| m.context))
}

/// Get model name for a provider and tier
pub fn model_name(provider_id: &str, tier: ModelTier) -> Option<&'static str> {
    get(provider_id).and_then(|p| p.model(tier).map(|m| m.name.as_str()))
}

/// Get all three model tier IDs for a provider (fast, balanced, powerful)
/// Returns (fast_id, balanced_id, powerful_id)
pub fn model_tiers(provider_id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    get(provider_id).map(|p| {
        (
            p.model_id(ModelTier::Fast),
            p.model_id(ModelTier::Balanced),
            p.model_id(ModelTier::Powerful),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_loads() {
        assert!(get("anthropic").is_some());
        assert!(get("openai").is_some());
        assert!(get("deepseek").is_some());
    }

    #[test]
    fn test_native_search() {
        assert!(has_native_search("anthropic"));
        assert!(has_native_search("openai"));
        assert!(!has_native_search("deepseek"));
        assert!(!has_native_search("ollama"));
    }

    #[test]
    fn test_base_url() {
        assert_eq!(base_url("anthropic"), Some("https://api.anthropic.com"));
        assert_eq!(base_url("ollama"), Some("http://localhost:11434"));
    }

    #[test]
    fn test_default_model() {
        let model = default_model("anthropic");
        assert!(model.is_some());
        assert!(model.unwrap().contains("claude"));
    }

    #[test]
    fn test_model_tiers() {
        let provider = get("anthropic").unwrap();
        assert!(provider.model(ModelTier::Fast).is_some());
        assert!(provider.model(ModelTier::Balanced).is_some());
        assert!(provider.model(ModelTier::Powerful).is_some());
    }
}
