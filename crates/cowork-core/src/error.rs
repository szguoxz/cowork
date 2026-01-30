//! Error types for Cowork Core

use thiserror::Error;

/// Result type alias using Cowork Error
pub type Result<T> = std::result::Result<T, Error>;

/// Cowork error types
#[derive(Error, Debug)]
pub enum Error {
    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Tool error: {0}")]
    Tool(#[from] ToolError),

    #[error("Task error: {0}")]
    Task(String),

    #[error("Approval denied for action: {0}")]
    ApprovalDenied(String),

    #[error("Workspace error: {0}")]
    Workspace(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Path is outside workspace boundary: {0}")]
    PathOutsideWorkspace(String),

    #[error("Operation timed out after {0} seconds")]
    Timeout(u64),

    #[error("Operation cancelled")]
    Cancelled,
}

/// Tool-specific errors
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Rejected by user: {0}")]
    Rejected(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
