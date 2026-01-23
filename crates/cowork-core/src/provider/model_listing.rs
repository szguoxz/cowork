//! Model listing from catalog constants
//!
//! Returns known models for each provider from the centralized model catalog.

use serde::{Deserialize, Serialize};

use super::model_catalog;
use super::ProviderType;

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

/// Get context window limit for a model without making API calls
///
/// This uses hardcoded known values for common models. Returns None if unknown.
pub fn get_model_context_limit(provider: ProviderType, model: &str) -> Option<usize> {
    let model_lower = model.to_lowercase();

    match provider {
        ProviderType::Anthropic => get_anthropic_context_window(&model_lower),
        ProviderType::OpenAI => get_openai_context_window(&model_lower).map(|v| v as usize),
        ProviderType::DeepSeek => get_deepseek_context_window(&model_lower).map(|v| v as usize),
        ProviderType::Gemini => get_gemini_context_window(&model_lower),
        ProviderType::Groq => get_groq_context_window(&model_lower),
        ProviderType::XAI => Some(131_072), // Grok models
        ProviderType::Together | ProviderType::Fireworks | ProviderType::Nebius => {
            get_open_source_context_window(&model_lower)
        }
        ProviderType::Ollama => get_ollama_context_window(&model_lower),
        _ => None,
    }
}

fn get_anthropic_context_window(model: &str) -> Option<usize> {
    // All current Claude models have 200K context
    if model.contains("claude") {
        // Claude 2.0/instant had 100K
        if model.contains("2.0") || model.contains("instant") {
            return Some(100_000);
        }
        return Some(200_000);
    }
    None
}

fn get_openai_context_window(model_id: &str) -> Option<u32> {
    let id = model_id.to_lowercase();

    // GPT-5 series
    if id.starts_with("gpt-5") {
        return Some(model_catalog::OPENAI_BALANCED.2 as u32);
    }

    // GPT-4.1 series (1M context window)
    if id.starts_with("gpt-4.1") {
        return Some(1_000_000);
    }

    // o-series reasoning models
    if id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4") {
        return Some(200_000);
    }

    // GPT-4o and variants
    if id.contains("gpt-4o") || id.contains("4o-") {
        return Some(128_000);
    }

    // GPT-4 Turbo
    if id.contains("gpt-4-turbo") || id.contains("gpt-4-1106") || id.contains("gpt-4-0125") {
        return Some(128_000);
    }

    // GPT-4 32K
    if id.contains("gpt-4-32k") {
        return Some(32_768);
    }

    // Base GPT-4
    if id.starts_with("gpt-4") {
        return Some(8_192);
    }

    // GPT-3.5 16K
    if id.contains("gpt-3.5") && id.contains("16k") {
        return Some(16_385);
    }

    // Base GPT-3.5
    if id.contains("gpt-3.5") {
        return Some(4_096);
    }

    None
}

fn get_deepseek_context_window(model_id: &str) -> Option<u32> {
    let id = model_id.to_lowercase();
    if id.contains("coder") {
        Some(128_000)
    } else {
        // deepseek-chat, deepseek-reasoner, etc.
        Some(131_072)
    }
}

fn get_gemini_context_window(model: &str) -> Option<usize> {
    if model.contains("gemini") {
        if model.contains("1.5") || model.contains("2.0") || model.contains("2.5") || model.contains("3") {
            return Some(1_000_000);
        }
        if model.contains("1.0") {
            return Some(32_000);
        }
        // Default for newer Gemini
        return Some(1_000_000);
    }
    None
}

fn get_groq_context_window(model: &str) -> Option<usize> {
    // Groq hosts various open source models
    if model.contains("llama") {
        if model.contains("3.1") || model.contains("3.2") || model.contains("3.3") {
            return Some(128_000);
        }
        return Some(8_192);
    }
    if model.contains("mixtral") {
        return Some(32_000);
    }
    Some(32_000) // Conservative default for Groq
}

fn get_open_source_context_window(model: &str) -> Option<usize> {
    // Common open source models hosted on Together, Fireworks, etc.
    if model.contains("llama") {
        if model.contains("llama-4") || model.contains("llama4") {
            return Some(1_000_000); // Llama 4 Maverick: 1M context
        }
        if model.contains("3.1") || model.contains("3.2") || model.contains("3.3") {
            return Some(128_000);
        }
        return Some(8_192);
    }
    if model.contains("mistral") || model.contains("mixtral") {
        if model.contains("large") {
            return Some(128_000);
        }
        return Some(32_000);
    }
    if model.contains("qwen") {
        return Some(32_000);
    }
    if model.contains("codellama") || model.contains("deepseek") {
        return Some(16_000);
    }
    None
}

fn get_ollama_context_window(model: &str) -> Option<usize> {
    // Ollama uses default context of 2048, but can be configured
    // These are the model's native context limits
    if model.contains("llama3") {
        return Some(8_192);
    }
    if model.contains("mistral") || model.contains("mixtral") {
        return Some(32_000);
    }
    if model.contains("codellama") {
        return Some(16_000);
    }
    if model.contains("qwen") {
        return Some(32_000);
    }
    // Conservative default for local models
    Some(4_096)
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

/// Helper to build a ModelInfo from a catalog entry and mark as recommended
fn from_entry(entry: model_catalog::ModelEntry, recommended: bool) -> ModelInfo {
    let mut info = ModelInfo::new(entry.0)
        .with_name(entry.1)
        .with_context_window(entry.2 as u32);
    if recommended {
        info = info.recommended();
    }
    info
}

/// Get known models for a provider from the catalog.
///
/// Returns deduplicated entries (fast/balanced/powerful tiers) as `Vec<ModelInfo>`.
/// The balanced tier is marked as recommended.
pub fn get_known_models(provider: ProviderType) -> Vec<ModelInfo> {
    let (fast, balanced, powerful) = match provider {
        ProviderType::Anthropic => (
            model_catalog::ANTHROPIC_FAST,
            model_catalog::ANTHROPIC_BALANCED,
            model_catalog::ANTHROPIC_POWERFUL,
        ),
        ProviderType::OpenAI => (
            model_catalog::OPENAI_FAST,
            model_catalog::OPENAI_BALANCED,
            model_catalog::OPENAI_POWERFUL,
        ),
        ProviderType::Gemini => (
            model_catalog::GEMINI_FAST,
            model_catalog::GEMINI_BALANCED,
            model_catalog::GEMINI_POWERFUL,
        ),
        ProviderType::DeepSeek => (
            model_catalog::DEEPSEEK_FAST,
            model_catalog::DEEPSEEK_BALANCED,
            model_catalog::DEEPSEEK_POWERFUL,
        ),
        ProviderType::Groq => (
            model_catalog::GROQ_FAST,
            model_catalog::GROQ_BALANCED,
            model_catalog::GROQ_POWERFUL,
        ),
        ProviderType::XAI => (
            model_catalog::XAI_FAST,
            model_catalog::XAI_BALANCED,
            model_catalog::XAI_POWERFUL,
        ),
        ProviderType::Cohere => (
            model_catalog::COHERE_FAST,
            model_catalog::COHERE_BALANCED,
            model_catalog::COHERE_POWERFUL,
        ),
        ProviderType::Perplexity => (
            model_catalog::PERPLEXITY_FAST,
            model_catalog::PERPLEXITY_BALANCED,
            model_catalog::PERPLEXITY_POWERFUL,
        ),
        ProviderType::Together => (
            model_catalog::TOGETHER_FAST,
            model_catalog::TOGETHER_BALANCED,
            model_catalog::TOGETHER_POWERFUL,
        ),
        ProviderType::Fireworks => (
            model_catalog::FIREWORKS_FAST,
            model_catalog::FIREWORKS_BALANCED,
            model_catalog::FIREWORKS_POWERFUL,
        ),
        ProviderType::Zai => (
            model_catalog::ZAI_FAST,
            model_catalog::ZAI_BALANCED,
            model_catalog::ZAI_POWERFUL,
        ),
        ProviderType::Nebius => (
            model_catalog::NEBIUS_FAST,
            model_catalog::NEBIUS_BALANCED,
            model_catalog::NEBIUS_POWERFUL,
        ),
        ProviderType::MIMO => (
            model_catalog::MIMO_FAST,
            model_catalog::MIMO_BALANCED,
            model_catalog::MIMO_POWERFUL,
        ),
        ProviderType::BigModel => (
            model_catalog::BIGMODEL_FAST,
            model_catalog::BIGMODEL_BALANCED,
            model_catalog::BIGMODEL_POWERFUL,
        ),
        ProviderType::Ollama => (
            model_catalog::OLLAMA_FAST,
            model_catalog::OLLAMA_BALANCED,
            model_catalog::OLLAMA_POWERFUL,
        ),
    };

    // Deduplicate: balanced is always included (recommended), then add fast/powerful if distinct
    let mut models = vec![from_entry(balanced, true)];

    if fast.0 != balanced.0 {
        models.insert(0, from_entry(fast, false));
    }
    if powerful.0 != balanced.0 {
        models.push(from_entry(powerful, false));
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
        let models = get_known_models(ProviderType::Anthropic);
        // Anthropic has 3 distinct models
        assert_eq!(models.len(), 3);
        // Balanced should be recommended
        assert!(models.iter().any(|m| m.id == model_catalog::ANTHROPIC_BALANCED.0 && m.recommended));
    }

    #[test]
    fn test_get_known_models_deduplicates() {
        // DeepSeek: fast == balanced, so should deduplicate
        let models = get_known_models(ProviderType::DeepSeek);
        assert_eq!(models.len(), 2); // balanced + powerful
        // MIMO: all same model
        let models = get_known_models(ProviderType::MIMO);
        assert_eq!(models.len(), 1); // just balanced
    }

    #[test]
    fn test_get_known_models_all_providers() {
        // Ensure no panics for any provider
        let providers = [
            ProviderType::Anthropic,
            ProviderType::OpenAI,
            ProviderType::Gemini,
            ProviderType::DeepSeek,
            ProviderType::Groq,
            ProviderType::XAI,
            ProviderType::Cohere,
            ProviderType::Perplexity,
            ProviderType::Together,
            ProviderType::Fireworks,
            ProviderType::Zai,
            ProviderType::Nebius,
            ProviderType::MIMO,
            ProviderType::BigModel,
            ProviderType::Ollama,
        ];
        for provider in providers {
            let models = get_known_models(provider);
            assert!(!models.is_empty(), "Provider {:?} returned no models", provider);
            assert!(models.iter().any(|m| m.recommended), "Provider {:?} has no recommended model", provider);
        }
    }
}
