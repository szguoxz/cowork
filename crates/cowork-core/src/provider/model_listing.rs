//! Model listing from catalog
//!
//! Returns known models for each provider from the centralized catalog.

use serde::{Deserialize, Serialize};

use super::catalog;

/// Information about an available model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID (what you pass to the API)
    pub id: String,
    /// Human-readable name (if available)
    pub name: Option<String>,
    /// Description (if available)
    pub description: Option<String>,
    /// Context window size (if available)
    pub context_window: Option<u32>,
    /// Whether this is a recommended/featured model
    pub recommended: bool,
}

/// Get context window limit for a model from the catalog.
///
/// Looks up the model in the provider's catalog. If the exact model isn't found,
/// falls back to the provider's default (balanced tier) context window.
pub fn get_model_context_limit(provider_id: &str, model: &str) -> Option<usize> {
    let cat_provider = catalog::get(provider_id)?;

    // Check if model matches any of the three tiers
    for tier in [catalog::ModelTier::Fast, catalog::ModelTier::Balanced, catalog::ModelTier::Powerful] {
        if let Some(m) = cat_provider.model(tier)
            && m.id == model {
                return Some(m.context);
            }
    }

    // Fall back to default (balanced) context window
    Some(cat_provider.default_model().context)
}

/// Get max output tokens for a model from the catalog.
///
/// Looks up the model in the provider's catalog. If the exact model isn't found,
/// falls back to the provider's default (balanced tier) max output.
pub fn get_model_max_output(provider_id: &str, model: &str) -> Option<usize> {
    let cat_provider = catalog::get(provider_id)?;

    // Check if model matches any of the three tiers
    for tier in [catalog::ModelTier::Fast, catalog::ModelTier::Balanced, catalog::ModelTier::Powerful] {
        if let Some(m) = cat_provider.model(tier)
            && m.id == model {
                return Some(m.max_output);
            }
    }

    // Fall back to default (balanced) max output
    Some(cat_provider.default_model().max_output)
}

impl ModelInfo {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            description: None,
            context_window: None,
            recommended: false,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_context_window(mut self, size: u32) -> Self {
        self.context_window = Some(size);
        self
    }

    pub fn recommended(mut self) -> Self {
        self.recommended = true;
        self
    }

    /// Get display name (name if available, otherwise id)
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

/// Get known models for a provider from the catalog.
///
/// Returns deduplicated entries (fast/balanced/powerful tiers) as `Vec<ModelInfo>`.
/// The balanced tier is marked as recommended.
pub fn get_known_models(provider_id: &str) -> Vec<ModelInfo> {
    // Get provider from catalog, return empty if not found
    let Some(cat_provider) = catalog::get(provider_id) else {
        return Vec::new();
    };

    // Get the three tier models
    let fast = cat_provider.model(catalog::ModelTier::Fast);
    let balanced = cat_provider.model(catalog::ModelTier::Balanced);
    let powerful = cat_provider.model(catalog::ModelTier::Powerful);

    // Build model info list - balanced is always included (recommended)
    let mut models = Vec::new();

    // Add fast if distinct from balanced
    if let (Some(f), Some(b)) = (fast, balanced)
        && f.id != b.id {
            models.push(
                ModelInfo::new(&f.id)
                    .with_name(&f.name)
                    .with_context_window(f.context as u32)
            );
        }

    // Add balanced (always, marked as recommended)
    if let Some(b) = balanced {
        models.push(
            ModelInfo::new(&b.id)
                .with_name(&b.name)
                .with_context_window(b.context as u32)
                .recommended()
        );
    }

    // Add powerful if distinct from balanced
    if let (Some(p), Some(b)) = (powerful, balanced)
        && p.id != b.id {
            models.push(
                ModelInfo::new(&p.id)
                    .with_name(&p.name)
                    .with_context_window(p.context as u32)
            );
        }

    models
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info() {
        let model = ModelInfo::new("gpt-4.1")
            .with_name("GPT-4.1")
            .with_description("Latest GPT-4.1 model")
            .recommended();

        assert_eq!(model.id, "gpt-4.1");
        assert_eq!(model.display_name(), "GPT-4.1");
        assert!(model.recommended);
    }

    #[test]
    fn test_get_known_models_anthropic() {
        let models = get_known_models("anthropic");
        // Anthropic has 3 distinct models
        assert_eq!(models.len(), 3);
        // Balanced should be recommended
        let balanced_id = catalog::default_model("anthropic").unwrap();
        assert!(models.iter().any(|m| m.id == balanced_id && m.recommended));
    }

    #[test]
    fn test_get_known_models_deduplicates() {
        // DeepSeek: fast == balanced, so should deduplicate
        let models = get_known_models("deepseek");
        assert_eq!(models.len(), 2); // balanced + powerful
        // MIMO: all same model
        let models = get_known_models("mimo");
        assert_eq!(models.len(), 1); // just balanced
    }

    #[test]
    fn test_get_model_max_output() {
        // Test Anthropic models have different max_output
        let opus_max = get_model_max_output("anthropic", "claude-opus-4-5-20251101");
        let sonnet_max = get_model_max_output("anthropic", "claude-sonnet-4-5-20250929");
        assert_eq!(opus_max, Some(32768)); // Opus: 32k
        assert_eq!(sonnet_max, Some(64000)); // Sonnet: 64k

        // Test OpenAI models
        let gpt5_max = get_model_max_output("openai", "gpt-5");
        assert_eq!(gpt5_max, Some(32768)); // GPT-5: 32k

        // Test fallback for unknown model returns balanced tier
        let unknown_max = get_model_max_output("anthropic", "unknown-model");
        assert_eq!(unknown_max, Some(64000)); // Falls back to balanced (Sonnet 64k)
    }

    #[test]
    fn test_get_known_models_all_providers() {
        // Ensure no panics for any provider
        let providers = [
            "anthropic",
            "openai",
            "gemini",
            "deepseek",
            "groq",
            "xai",
            "cohere",
            "perplexity",
            "together",
            "fireworks",
            "zai",
            "nebius",
            "mimo",
            "bigmodel",
            "ollama",
        ];
        for provider_id in providers {
            let models = get_known_models(provider_id);
            assert!(!models.is_empty(), "Provider {} returned no models", provider_id);
            assert!(models.iter().any(|m| m.recommended), "Provider {} has no recommended model", provider_id);
        }
    }
}
