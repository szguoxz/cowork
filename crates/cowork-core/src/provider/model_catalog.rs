//! Centralized model catalog
//!
//! Single source of truth for all model IDs, display names, and context windows.
//! All other files import from here. Update this file when models change.
//!
//! Each constant is a tuple: (model_id, display_name, context_window)

/// Model entry: (model_id, display_name, context_window)
pub type ModelEntry = (&'static str, &'static str, usize);

// ============================================================================
// Anthropic
// ============================================================================

pub const ANTHROPIC_FAST: ModelEntry = ("claude-haiku-4-5-20251001", "Claude Haiku 4.5", 200_000);
pub const ANTHROPIC_BALANCED: ModelEntry = ("claude-sonnet-4-5-20250929", "Claude Sonnet 4.5", 200_000);
pub const ANTHROPIC_POWERFUL: ModelEntry = ("claude-opus-4-5-20251101", "Claude Opus 4.5", 200_000);

// ============================================================================
// OpenAI
// ============================================================================

pub const OPENAI_FAST: ModelEntry = ("gpt-5-mini", "GPT-5 Mini", 400_000);
pub const OPENAI_BALANCED: ModelEntry = ("gpt-5.2", "GPT-5.2", 400_000);
pub const OPENAI_POWERFUL: ModelEntry = ("gpt-5.2-pro", "GPT-5.2 Pro", 400_000);

// ============================================================================
// Google Gemini
// ============================================================================

pub const GEMINI_FAST: ModelEntry = ("gemini-3-flash-preview", "Gemini 3 Flash", 1_000_000);
pub const GEMINI_BALANCED: ModelEntry = ("gemini-3-pro-preview", "Gemini 3 Pro", 1_000_000);
pub const GEMINI_POWERFUL: ModelEntry = ("gemini-3-pro-preview", "Gemini 3 Pro", 1_000_000);

// ============================================================================
// DeepSeek
// ============================================================================

pub const DEEPSEEK_FAST: ModelEntry = ("deepseek-chat", "DeepSeek Chat", 131_072);
pub const DEEPSEEK_BALANCED: ModelEntry = ("deepseek-chat", "DeepSeek Chat", 131_072);
pub const DEEPSEEK_POWERFUL: ModelEntry = ("deepseek-reasoner", "DeepSeek Reasoner (R1)", 131_072);

// ============================================================================
// Groq
// ============================================================================

pub const GROQ_FAST: ModelEntry = ("llama-3.1-8b-instant", "Llama 3.1 8B", 128_000);
pub const GROQ_BALANCED: ModelEntry = ("llama-3.3-70b-versatile", "Llama 3.3 70B", 128_000);
pub const GROQ_POWERFUL: ModelEntry = ("llama-3.3-70b-versatile", "Llama 3.3 70B", 128_000);

// ============================================================================
// xAI (Grok)
// ============================================================================

pub const XAI_FAST: ModelEntry = ("grok-3-mini-beta", "Grok 3 Mini", 131_072);
pub const XAI_BALANCED: ModelEntry = ("grok-3-beta", "Grok 3", 131_072);
pub const XAI_POWERFUL: ModelEntry = ("grok-3-beta", "Grok 3", 131_072);

// ============================================================================
// Cohere
// ============================================================================

pub const COHERE_FAST: ModelEntry = ("command-r", "Command R", 128_000);
pub const COHERE_BALANCED: ModelEntry = ("command-r-plus", "Command R+", 128_000);
pub const COHERE_POWERFUL: ModelEntry = ("command-r-plus", "Command R+", 128_000);

// ============================================================================
// Perplexity
// ============================================================================

pub const PERPLEXITY_FAST: ModelEntry = ("sonar", "Sonar", 128_000);
pub const PERPLEXITY_BALANCED: ModelEntry = ("sonar-pro", "Sonar Pro", 128_000);
pub const PERPLEXITY_POWERFUL: ModelEntry = ("sonar-reasoning", "Sonar Reasoning", 128_000);

// ============================================================================
// Ollama (Local)
// ============================================================================

pub const OLLAMA_FAST: ModelEntry = ("llama3.2:3b", "Llama 3.2 3B", 8_192);
pub const OLLAMA_BALANCED: ModelEntry = ("llama3.2", "Llama 3.2", 8_192);
pub const OLLAMA_POWERFUL: ModelEntry = ("llama3.3:70b", "Llama 3.3 70B", 128_000);

// ============================================================================
// Together AI
// ============================================================================

pub const TOGETHER_FAST: ModelEntry = ("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B Turbo", 128_000);
pub const TOGETHER_BALANCED: ModelEntry = ("meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8", "Llama 4 Maverick", 1_000_000);
pub const TOGETHER_POWERFUL: ModelEntry = ("deepseek-ai/DeepSeek-R1", "DeepSeek R1", 131_072);

// ============================================================================
// Fireworks AI
// ============================================================================

pub const FIREWORKS_FAST: ModelEntry = ("accounts/fireworks/models/llama-v3p3-70b-instruct", "Llama 3.3 70B", 128_000);
pub const FIREWORKS_BALANCED: ModelEntry = ("accounts/fireworks/models/llama-v3p3-70b-instruct", "Llama 3.3 70B", 128_000);
pub const FIREWORKS_POWERFUL: ModelEntry = ("accounts/fireworks/models/deepseek-r1", "DeepSeek R1", 131_072);

// ============================================================================
// Zai (Zhipu AI)
// ============================================================================

pub const ZAI_FAST: ModelEntry = ("glm-4-flash", "GLM-4 Flash", 128_000);
pub const ZAI_BALANCED: ModelEntry = ("glm-4-plus", "GLM-4 Plus", 128_000);
pub const ZAI_POWERFUL: ModelEntry = ("glm-4-plus", "GLM-4 Plus", 128_000);

// ============================================================================
// Nebius
// ============================================================================

pub const NEBIUS_FAST: ModelEntry = ("meta-llama/Meta-Llama-3.1-8B-Instruct", "Llama 3.1 8B", 128_000);
pub const NEBIUS_BALANCED: ModelEntry = ("meta-llama/Meta-Llama-3.1-70B-Instruct", "Llama 3.1 70B", 128_000);
pub const NEBIUS_POWERFUL: ModelEntry = ("deepseek-ai/DeepSeek-R1", "DeepSeek R1", 131_072);

// ============================================================================
// MIMO (Xiaomi)
// ============================================================================

pub const MIMO_FAST: ModelEntry = ("mimo-v2-flash", "MIMO v2 Flash", 128_000);
pub const MIMO_BALANCED: ModelEntry = ("mimo-v2-flash", "MIMO v2 Flash", 128_000);
pub const MIMO_POWERFUL: ModelEntry = ("mimo-v2-flash", "MIMO v2 Flash", 128_000);

// ============================================================================
// BigModel.cn
// ============================================================================

pub const BIGMODEL_FAST: ModelEntry = ("glm-4-flash", "GLM-4 Flash", 128_000);
pub const BIGMODEL_BALANCED: ModelEntry = ("glm-4-plus", "GLM-4 Plus", 128_000);
pub const BIGMODEL_POWERFUL: ModelEntry = ("glm-4-plus", "GLM-4 Plus", 128_000);
