//! Context management for agent execution
//!
//! Context holds the runtime state for task execution including:
//! - Workspace configuration
//! - Conversation history
//! - Variables and state
//! - Token counting and summarization
//! - Project context gathering

pub mod gather;
pub mod monitor;
pub mod summarizer;
pub mod tokens;

pub use gather::{ContextGatherer, MemoryFile, MemoryHierarchy, MemoryTier, ProjectContext};
pub use monitor::{ContextBreakdown, ContextMonitor, ContextUsage, MonitorConfig};
pub use summarizer::{CompactConfig, CompactResult, ConversationSummarizer, SummarizerConfig};
pub use tokens::TokenCounter;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Workspace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Root directory for operations
    pub root: PathBuf,
    /// Allowed directories (if empty, only root is allowed)
    pub allowed_dirs: Vec<PathBuf>,
    /// Directories that are always blocked
    pub blocked_dirs: Vec<PathBuf>,
}

impl Workspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed_dirs: Vec::new(),
            blocked_dirs: Vec::new(),
        }
    }

    /// Check if a path is within the workspace
    pub fn contains(&self, path: &std::path::Path) -> bool {
        if let Ok(canonical) = path.canonicalize() {
            if let Ok(root) = self.root.canonicalize() {
                // Check blocked directories first
                for blocked in &self.blocked_dirs {
                    if let Ok(blocked_canonical) = blocked.canonicalize() {
                        if canonical.starts_with(&blocked_canonical) {
                            return false;
                        }
                    }
                }

                // Check if in root
                if canonical.starts_with(&root) {
                    return true;
                }

                // Check allowed directories
                for allowed in &self.allowed_dirs {
                    if let Ok(allowed_canonical) = allowed.canonicalize() {
                        if canonical.starts_with(&allowed_canonical) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// A message in the conversation history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

// ============================================================================
// Message Conversion Utilities
// ============================================================================
// These functions provide standardized conversion between the core Message type
// and UI-specific string-based role representations, eliminating duplicate
// conversion code across CLI and UI modules.

impl MessageRole {
    /// Parse a role string into MessageRole
    ///
    /// # Examples
    /// ```
    /// use cowork_core::context::MessageRole;
    /// assert_eq!(MessageRole::parse("user"), MessageRole::User);
    /// assert_eq!(MessageRole::parse("assistant"), MessageRole::Assistant);
    /// assert_eq!(MessageRole::parse("system"), MessageRole::System);
    /// assert_eq!(MessageRole::parse("tool"), MessageRole::Tool);
    /// assert_eq!(MessageRole::parse("unknown"), MessageRole::Tool); // default
    /// ```
    pub fn parse(s: &str) -> Self {
        match s {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            "tool" => MessageRole::Tool,
            _ => MessageRole::Tool, // Default for unknown roles
        }
    }

    /// Convert MessageRole to a string
    ///
    /// # Examples
    /// ```
    /// use cowork_core::context::MessageRole;
    /// assert_eq!(MessageRole::User.as_str(), "user");
    /// assert_eq!(MessageRole::Assistant.as_str(), "assistant");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        }
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for MessageRole {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MessageRole::parse(s))
    }
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

    /// Create a new message with a specific timestamp
    pub fn with_timestamp(
        role: MessageRole,
        content: impl Into<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            role,
            content: content.into(),
            timestamp,
        }
    }

    /// Create a message from string-based role (used by UI)
    ///
    /// This is a convenience constructor for converting from UI representations
    /// where roles are stored as strings.
    pub fn from_str_role(
        role: &str,
        content: impl Into<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            role: MessageRole::parse(role),
            content: content.into(),
            timestamp,
        }
    }

    /// Get the role as a string (for UI serialization)
    pub fn role_str(&self) -> &'static str {
        self.role.as_str()
    }
}

/// Convert a collection of UI-style messages to context Messages
///
/// This function is designed to work with UI message types that have:
/// - A `role` field as a string
/// - A `content` field as a string
/// - A `timestamp` field as `DateTime<Utc>`
///
/// # Type Parameters
/// - `T`: Any type that can provide role, content, and timestamp via the accessor function
/// - `F`: Function to extract (role, content, timestamp) from T
pub fn messages_from_ui<T, F>(messages: &[T], accessor: F) -> Vec<Message>
where
    F: Fn(&T) -> (&str, &str, chrono::DateTime<chrono::Utc>),
{
    messages
        .iter()
        .map(|m| {
            let (role, content, timestamp) = accessor(m);
            Message::from_str_role(role, content, timestamp)
        })
        .collect()
}

/// Runtime context for task execution
#[derive(Debug, Clone)]
pub struct Context {
    /// Workspace configuration
    pub workspace: Workspace,
    /// Conversation history
    pub messages: Vec<Message>,
    /// Variables and state
    pub variables: HashMap<String, Value>,
    /// Maximum messages to keep in history
    pub max_history: usize,
}

impl Context {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            messages: Vec::new(),
            variables: HashMap::new(),
            max_history: 100,
        }
    }

    /// Add a message to the conversation history
    pub fn add_message(&mut self, role: MessageRole, content: impl Into<String>) {
        let message = Message {
            role,
            content: content.into(),
            timestamp: chrono::Utc::now(),
        };

        self.messages.push(message);

        // Trim history if needed
        while self.messages.len() > self.max_history {
            self.messages.remove(0);
        }
    }

    /// Set a variable
    pub fn set_var(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.variables.insert(key.into(), value.into());
    }

    /// Get a variable
    pub fn get_var(&self, key: &str) -> Option<&Value> {
        self.variables.get(key)
    }

    /// Clear all variables
    pub fn clear_vars(&mut self) {
        self.variables.clear();
    }

    /// Get recent messages for context
    pub fn recent_messages(&self, count: usize) -> &[Message] {
        let start = self.messages.len().saturating_sub(count);
        &self.messages[start..]
    }

    /// Format messages for LLM consumption
    pub fn format_history(&self) -> String {
        self.messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                    MessageRole::Tool => "Tool",
                };
                format!("{}: {}", role, m.content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
