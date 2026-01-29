//! Context management for agent execution
//!
//! This module provides:
//! - Token counting and context compaction (summarizer)
//! - Context limit checking (monitor)
//! - Project context gathering (gather)

pub mod gather;
pub mod monitor;
pub mod summarizer;

pub use gather::{ContextGatherer, MemoryFile, MemoryHierarchy, MemoryTier, ProjectContext};
pub use monitor::{context_limit, should_compact, usage_stats, ContextUsage};
pub use summarizer::{compact, CompactResult};

use serde::{Deserialize, Serialize};

// Re-export ChatRole from genai as MessageRole for compatibility
pub use genai::chat::ChatRole as MessageRole;

/// A message in the conversation history (used for summarization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Message {
    /// Create a new message with the current timestamp
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            timestamp: chrono::Utc::now(),
        }
    }
}
