//! Model listing from provider APIs
//!
//! Fetches available models from each provider's API endpoint.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::ProviderType;
use crate::error::{Error, Result};

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
/// For accurate real-time values, use `fetch_models` and check `context_window`.
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

fn get_gemini_context_window(model: &str) -> Option<usize> {
    if model.contains("gemini") {
        if model.contains("1.5") || model.contains("2.0") || model.contains("2.5") {
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

/// Fetch available models for a provider
pub async fn fetch_models(provider: ProviderType, api_key: &str) -> Result<Vec<ModelInfo>> {
    let client = Client::new();

    match provider {
        ProviderType::OpenAI => fetch_openai_models(&client, api_key).await,
        ProviderType::Anthropic => fetch_anthropic_models(&client, api_key).await,
        ProviderType::Gemini => fetch_gemini_models(&client, api_key).await,
        ProviderType::Groq => fetch_groq_models(&client, api_key).await,
        ProviderType::DeepSeek => fetch_deepseek_models(&client, api_key).await,
        ProviderType::XAI => fetch_xai_models(&client, api_key).await,
        ProviderType::Together => fetch_together_models(&client, api_key).await,
        ProviderType::Fireworks => fetch_fireworks_models(&client, api_key).await,
        ProviderType::Ollama => fetch_ollama_models(&client).await,
        // For providers without model listing APIs, return empty (will use defaults)
        _ => Ok(Vec::new()),
    }
}

// ============================================================================
// OpenAI
// ============================================================================

#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModel>,
}

#[derive(Deserialize)]
struct OpenAIModel {
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
    owned_by: String,
}

async fn fetch_openai_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch OpenAI models: {}", e)))?;

    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "OpenAI API error: {}",
            response.status()
        )));
    }

    let models_response: OpenAIModelsResponse = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse OpenAI models: {}", e)))?;

    // Filter to chat models and sort by relevance
    let chat_models: Vec<ModelInfo> = models_response
        .data
        .into_iter()
        .filter(|m| is_openai_chat_model(&m.id))
        .map(|m| {
            let mut info = ModelInfo::new(&m.id);
            if is_openai_recommended(&m.id) {
                info = info.recommended();
            }
            // Add context window if known
            if let Some(ctx) = get_openai_context_window(&m.id) {
                info = info.with_context_window(ctx);
            }
            info
        })
        .collect();

    let mut models = sort_models(chat_models);

    // Ensure we have at least the known good models at the top
    ensure_model_exists_with_context(&mut models, "gpt-4.1", true, 1_000_000);
    ensure_model_exists_with_context(&mut models, "gpt-4.1-mini", false, 1_000_000);
    ensure_model_exists_with_context(&mut models, "o3", false, 200_000);
    ensure_model_exists_with_context(&mut models, "o4-mini", false, 200_000);

    Ok(models)
}

fn is_openai_chat_model(id: &str) -> bool {
    // Include GPT-5, GPT-4, GPT-3.5, o1/o3/o4 models, exclude embeddings, whisper, dall-e, etc.
    (id.starts_with("gpt-5") || id.starts_with("gpt-4") || id.starts_with("gpt-3.5") || id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4"))
        && !id.contains("instruct")
        && !id.contains("vision")
        && !id.contains("realtime")
}

fn is_openai_recommended(id: &str) -> bool {
    id == "gpt-4.1" || id.starts_with("gpt-4.1-") || id == "o3" || id == "gpt-5" || id.starts_with("gpt-5-")
}

/// Get context window for an OpenAI model
fn get_openai_context_window(model_id: &str) -> Option<u32> {
    let id = model_id.to_lowercase();

    // GPT-5 series
    if id.starts_with("gpt-5") {
        return Some(256_000);
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

// ============================================================================
// Anthropic
// ============================================================================

#[derive(Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Deserialize)]
struct AnthropicModel {
    id: String,
    display_name: Option<String>,
}

async fn fetch_anthropic_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let response = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch Anthropic models: {}", e)))?;

    if !response.status().is_success() {
        // Anthropic might not have a public models endpoint yet - return known models
        return Ok(get_anthropic_known_models());
    }

    let models_response: AnthropicModelsResponse = response
        .json()
        .await
        .map_err(|_| Error::Provider("Failed to parse Anthropic models".into()))?;

    let models: Vec<ModelInfo> = models_response
        .data
        .into_iter()
        .map(|m| {
            let recommended = is_anthropic_recommended(&m.id);
            let mut info = ModelInfo::new(&m.id);
            if let Some(name) = m.display_name {
                info = info.with_name(name);
            }
            if recommended {
                info = info.recommended();
            }
            info
        })
        .collect();

    if models.is_empty() {
        return Ok(get_anthropic_known_models());
    }

    Ok(sort_models(models))
}

fn get_anthropic_known_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo::new("claude-opus-4-5-20251101")
            .with_name("Claude Opus 4.5")
            .with_context_window(200_000),
        ModelInfo::new("claude-sonnet-4-20250514")
            .with_name("Claude Sonnet 4")
            .with_context_window(200_000)
            .recommended(),
        ModelInfo::new("claude-3-5-haiku-20241022")
            .with_name("Claude 3.5 Haiku")
            .with_context_window(200_000),
    ]
}

fn is_anthropic_recommended(id: &str) -> bool {
    id.contains("sonnet-4") || id.contains("opus-4-5")
}

// ============================================================================
// Google Gemini
// ============================================================================

#[derive(Deserialize)]
struct GeminiModelsResponse {
    models: Vec<GeminiModel>,
}

#[derive(Deserialize)]
struct GeminiModel {
    name: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    description: Option<String>,
}

async fn fetch_gemini_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        api_key
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch Gemini models: {}", e)))?;

    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "Gemini API error: {}",
            response.status()
        )));
    }

    let models_response: GeminiModelsResponse = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse Gemini models: {}", e)))?;

    let models: Vec<ModelInfo> = models_response
        .models
        .into_iter()
        .filter(|m| is_gemini_chat_model(&m.name))
        .map(|m| {
            // Extract model ID from "models/gemini-1.5-pro" format
            let id = m.name.strip_prefix("models/").unwrap_or(&m.name);
            let recommended = is_gemini_recommended(id);
            let mut info = ModelInfo::new(id);
            if let Some(name) = m.display_name {
                info = info.with_name(name);
            }
            if let Some(desc) = m.description {
                info = info.with_description(desc);
            }
            if recommended {
                info = info.recommended();
            }
            info
        })
        .collect();

    Ok(sort_models(models))
}

fn is_gemini_chat_model(name: &str) -> bool {
    name.contains("gemini") && !name.contains("embedding") && !name.contains("aqa")
}

fn is_gemini_recommended(id: &str) -> bool {
    id == "gemini-2.5-flash" || id == "gemini-2.5-pro"
}

// ============================================================================
// Groq
// ============================================================================

#[derive(Deserialize)]
struct GroqModelsResponse {
    data: Vec<GroqModel>,
}

#[derive(Deserialize)]
struct GroqModel {
    id: String,
    #[allow(dead_code)]
    owned_by: Option<String>,
}

async fn fetch_groq_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let response = client
        .get("https://api.groq.com/openai/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch Groq models: {}", e)))?;

    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "Groq API error: {}",
            response.status()
        )));
    }

    let models_response: GroqModelsResponse = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse Groq models: {}", e)))?;

    let models: Vec<ModelInfo> = models_response
        .data
        .into_iter()
        .filter(|m| !m.id.contains("whisper"))
        .map(|m| {
            let recommended = is_groq_recommended(&m.id);
            let mut info = ModelInfo::new(&m.id);
            if recommended {
                info = info.recommended();
            }
            info
        })
        .collect();

    Ok(sort_models(models))
}

fn is_groq_recommended(id: &str) -> bool {
    id.contains("llama-3.3") || id.contains("llama-3.1-70b")
}

// ============================================================================
// DeepSeek
// ============================================================================

async fn fetch_deepseek_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    // DeepSeek uses OpenAI-compatible API
    let response = client
        .get("https://api.deepseek.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch DeepSeek models: {}", e)))?;

    if !response.status().is_success() {
        // Return known models if API fails
        return Ok(get_deepseek_known_models());
    }

    let models_response: OpenAIModelsResponse = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse DeepSeek models: {}", e)))?;

    let models: Vec<ModelInfo> = models_response
        .data
        .into_iter()
        .map(|m| {
            let recommended = m.id == "deepseek-chat";
            let mut info = ModelInfo::new(&m.id);
            if recommended {
                info = info.recommended();
            }
            // Add context window
            if let Some(ctx) = get_deepseek_context_window(&m.id) {
                info = info.with_context_window(ctx);
            }
            info
        })
        .collect();

    Ok(sort_models(models))
}

fn get_deepseek_known_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo::new("deepseek-chat")
            .with_name("DeepSeek Chat")
            .with_context_window(131_072)
            .recommended(),
        ModelInfo::new("deepseek-reasoner")
            .with_name("DeepSeek Reasoner (R1)")
            .with_context_window(131_072),
        ModelInfo::new("deepseek-coder")
            .with_name("DeepSeek Coder")
            .with_context_window(128_000),
    ]
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

// ============================================================================
// xAI (Grok)
// ============================================================================

async fn fetch_xai_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let response = client
        .get("https://api.x.ai/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch xAI models: {}", e)))?;

    if !response.status().is_success() {
        // Return known models if API fails
        return Ok(vec![
            ModelInfo::new("grok-2")
                .with_name("Grok 2")
                .recommended(),
            ModelInfo::new("grok-beta")
                .with_name("Grok Beta"),
        ]);
    }

    let models_response: OpenAIModelsResponse = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse xAI models: {}", e)))?;

    let models: Vec<ModelInfo> = models_response
        .data
        .into_iter()
        .map(|m| {
            let recommended = m.id == "grok-2";
            let mut info = ModelInfo::new(&m.id);
            if recommended {
                info = info.recommended();
            }
            info
        })
        .collect();

    Ok(sort_models(models))
}

// ============================================================================
// Together AI
// ============================================================================

#[derive(Deserialize)]
struct TogetherModel {
    id: String,
    display_name: Option<String>,
    #[serde(rename = "type")]
    model_type: Option<String>,
}

async fn fetch_together_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let response = client
        .get("https://api.together.xyz/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch Together models: {}", e)))?;

    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "Together API error: {}",
            response.status()
        )));
    }

    let models: Vec<TogetherModel> = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse Together models: {}", e)))?;

    let models: Vec<ModelInfo> = models
        .into_iter()
        .filter(|m| m.model_type.as_deref() == Some("chat"))
        .map(|m| {
            let recommended = is_together_recommended(&m.id);
            let mut info = ModelInfo::new(&m.id);
            if let Some(name) = m.display_name {
                info = info.with_name(name);
            }
            if recommended {
                info = info.recommended();
            }
            info
        })
        .collect();

    Ok(sort_models(models))
}

fn is_together_recommended(id: &str) -> bool {
    id.contains("Llama-3.1-70B") || id.contains("Llama-3.1-405B")
}

// ============================================================================
// Fireworks AI
// ============================================================================

async fn fetch_fireworks_models(client: &Client, api_key: &str) -> Result<Vec<ModelInfo>> {
    let response = client
        .get("https://api.fireworks.ai/inference/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to fetch Fireworks models: {}", e)))?;

    if !response.status().is_success() {
        // Return known models
        return Ok(vec![
            ModelInfo::new("accounts/fireworks/models/llama-v3p1-70b-instruct")
                .with_name("Llama 3.1 70B")
                .recommended(),
            ModelInfo::new("accounts/fireworks/models/llama-v3p1-8b-instruct")
                .with_name("Llama 3.1 8B"),
        ]);
    }

    let models_response: OpenAIModelsResponse = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse Fireworks models: {}", e)))?;

    let models: Vec<ModelInfo> = models_response
        .data
        .into_iter()
        .filter(|m| m.id.contains("instruct") || m.id.contains("chat"))
        .map(|m| {
            let recommended = m.id.contains("llama-v3p1-70b");
            let mut info = ModelInfo::new(&m.id);
            if recommended {
                info = info.recommended();
            }
            info
        })
        .collect();

    Ok(sort_models(models))
}

// ============================================================================
// Ollama (Local)
// ============================================================================

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

async fn fetch_ollama_models(client: &Client) -> Result<Vec<ModelInfo>> {
    let response = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|e| Error::Provider(format!("Failed to connect to Ollama: {}", e)))?;

    if !response.status().is_success() {
        return Err(Error::Provider(
            "Ollama not running or no models installed".into(),
        ));
    }

    let tags: OllamaTagsResponse = response
        .json()
        .await
        .map_err(|e| Error::Provider(format!("Failed to parse Ollama models: {}", e)))?;

    let models: Vec<ModelInfo> = tags
        .models
        .into_iter()
        .map(|m| {
            let recommended = m.name.contains("llama3") || m.name.contains("mistral");
            let mut info = ModelInfo::new(&m.name);
            if recommended {
                info = info.recommended();
            }
            info
        })
        .collect();

    Ok(sort_models(models))
}

// ============================================================================
// Helpers
// ============================================================================

/// Sort models with recommended first, then alphabetically
fn sort_models(mut models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    models.sort_by(|a, b| {
        match (a.recommended, b.recommended) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.id.cmp(&b.id),
        }
    });
    models
}

/// Ensure a model exists with context window, adding it if missing
fn ensure_model_exists_with_context(
    models: &mut Vec<ModelInfo>,
    id: &str,
    recommended: bool,
    context_window: u32,
) {
    if !models.iter().any(|m| m.id == id) {
        let mut info = ModelInfo::new(id).with_context_window(context_window);
        if recommended {
            info = info.recommended();
        }
        if recommended {
            models.insert(0, info);
        } else {
            models.push(info);
        }
    } else {
        // Update existing model's context window if not set
        if let Some(model) = models.iter_mut().find(|m| m.id == id)
            && model.context_window.is_none() {
                model.context_window = Some(context_window);
            }
    }
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
    fn test_sort_models() {
        let models = vec![
            ModelInfo::new("z-model"),
            ModelInfo::new("a-model").recommended(),
            ModelInfo::new("b-model"),
        ];

        let sorted = sort_models(models);
        assert_eq!(sorted[0].id, "a-model"); // recommended first
        assert_eq!(sorted[1].id, "b-model");
        assert_eq!(sorted[2].id, "z-model");
    }
}
