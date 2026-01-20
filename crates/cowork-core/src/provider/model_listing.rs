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
            info
        })
        .collect();

    let mut models = sort_models(chat_models);

    // Ensure we have at least the known good models at the top
    ensure_model_exists(&mut models, "gpt-4o", true);
    ensure_model_exists(&mut models, "gpt-4o-mini", false);
    ensure_model_exists(&mut models, "o1", false);
    ensure_model_exists(&mut models, "o1-mini", false);

    Ok(models)
}

fn is_openai_chat_model(id: &str) -> bool {
    // Include GPT-4, GPT-3.5, o1 models, exclude embeddings, whisper, dall-e, etc.
    (id.starts_with("gpt-4") || id.starts_with("gpt-3.5") || id.starts_with("o1") || id.starts_with("o3"))
        && !id.contains("instruct")
        && !id.contains("vision")
        && !id.contains("realtime")
}

fn is_openai_recommended(id: &str) -> bool {
    id == "gpt-4o" || id == "gpt-4o-mini" || id == "o1"
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
        ModelInfo::new("claude-sonnet-4-20250514")
            .with_name("Claude Sonnet 4")
            .recommended(),
        ModelInfo::new("claude-3-5-sonnet-20241022")
            .with_name("Claude 3.5 Sonnet"),
        ModelInfo::new("claude-3-5-haiku-20241022")
            .with_name("Claude 3.5 Haiku"),
        ModelInfo::new("claude-3-opus-20240229")
            .with_name("Claude 3 Opus"),
    ]
}

fn is_anthropic_recommended(id: &str) -> bool {
    id.contains("sonnet-4") || id.contains("claude-3-5-sonnet")
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
    id == "gemini-2.0-flash" || id == "gemini-1.5-pro"
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
        return Ok(vec![
            ModelInfo::new("deepseek-chat")
                .with_name("DeepSeek Chat")
                .recommended(),
            ModelInfo::new("deepseek-reasoner")
                .with_name("DeepSeek Reasoner"),
        ]);
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
            info
        })
        .collect();

    Ok(sort_models(models))
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

/// Ensure a model exists in the list, adding it if missing
fn ensure_model_exists(models: &mut Vec<ModelInfo>, id: &str, recommended: bool) {
    if !models.iter().any(|m| m.id == id) {
        let mut info = ModelInfo::new(id);
        if recommended {
            info = info.recommended();
        }
        if recommended {
            models.insert(0, info);
        } else {
            models.push(info);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info() {
        let model = ModelInfo::new("gpt-4o")
            .with_name("GPT-4o")
            .with_description("Latest GPT-4 model")
            .recommended();

        assert_eq!(model.id, "gpt-4o");
        assert_eq!(model.display_name(), "GPT-4o");
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
