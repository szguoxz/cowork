//! Tool system for Cowork agents
//!
//! Tools are the actions that agents can take. Each tool has:
//! - A name and description for the LLM
//! - A JSON schema for parameters
//! - An execute method
//! - An approval level

pub mod browser;
pub mod document;
pub mod filesystem;
pub mod interaction;
pub mod lsp;
pub mod notebook;
pub mod planning;
pub mod shell;
pub mod task;
pub mod web;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;

/// Boxed future type for object-safe async trait methods
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Output from a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Whether the tool succeeded
    pub success: bool,
    /// The output content (can be text, JSON, etc.)
    pub content: Value,
    /// Optional error message
    pub error: Option<String>,
    /// Metadata about the execution
    pub metadata: HashMap<String, Value>,
}

impl ToolOutput {
    pub fn success(content: impl Into<Value>) -> Self {
        Self {
            success: true,
            content: content.into(),
            error: None,
            metadata: HashMap::new(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            content: Value::Null,
            error: Some(message.into()),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Tool definition for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Core trait for all tools
pub trait Tool: Send + Sync {
    /// Tool name (used by LLM to invoke)
    fn name(&self) -> &str;

    /// Description of what the tool does
    fn description(&self) -> &str;

    /// JSON schema for parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with given parameters
    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>>;

    /// What level of approval this tool requires
    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }

    /// Convert to tool definition for LLM
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}

/// Registry of available tools
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all available tools
    pub fn list(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.to_definition()).collect()
    }

    /// Get all tools
    pub fn all(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }
}

/// Get standard tool definitions
///
/// Returns tool definitions for all standard tools available in the system.
/// This creates a temporary registry to extract the definitions from actual
/// tool implementations, ensuring consistency between definitions and behavior.
///
/// # Arguments
/// * `workspace` - The workspace path (used by filesystem and other tools)
///
/// # Returns
/// A vector of tool definitions that can be sent to the LLM
pub fn standard_tool_definitions(workspace: &std::path::Path) -> Vec<ToolDefinition> {
    use crate::orchestration::ToolRegistryBuilder;

    // Create a registry without provider-specific tools (like task/agent)
    // since those require API key and won't work without configuration
    let registry = ToolRegistryBuilder::new(workspace.to_path_buf())
        .with_task(false) // Skip task tools as they need provider config
        .build();

    registry.list()
}

/// Helper macro for creating tool parameter schemas
#[macro_export]
macro_rules! tool_params {
    ($($field:ident : $type:expr => $desc:expr),* $(,)?) => {
        serde_json::json!({
            "type": "object",
            "properties": {
                $( stringify!($field): { "type": $type, "description": $desc } ),*
            },
            "required": [ $( stringify!($field) ),* ]
        })
    };
}
