//! LSP (Language Server Protocol) tools for code intelligence
//!
//! Provides integration with language servers like rust-analyzer, tsserver, etc.

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

#[cfg(feature = "lsp")]
mod client;

#[cfg(feature = "lsp")]
pub use client::LspClient;

/// LSP operations supported by the tool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspOperation {
    GoToDefinition,
    FindReferences,
    Hover,
    DocumentSymbol,
    WorkspaceSymbol,
    GoToImplementation,
    PrepareCallHierarchy,
    IncomingCalls,
    OutgoingCalls,
}

impl std::str::FromStr for LspOperation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "goToDefinition" => Ok(LspOperation::GoToDefinition),
            "findReferences" => Ok(LspOperation::FindReferences),
            "hover" => Ok(LspOperation::Hover),
            "documentSymbol" => Ok(LspOperation::DocumentSymbol),
            "workspaceSymbol" => Ok(LspOperation::WorkspaceSymbol),
            "goToImplementation" => Ok(LspOperation::GoToImplementation),
            "prepareCallHierarchy" => Ok(LspOperation::PrepareCallHierarchy),
            "incomingCalls" => Ok(LspOperation::IncomingCalls),
            "outgoingCalls" => Ok(LspOperation::OutgoingCalls),
            _ => Err(format!("Unknown LSP operation: {}", s)),
        }
    }
}

/// Tool for interacting with Language Server Protocol servers
pub struct LspTool {
    workspace: PathBuf,
    #[cfg(feature = "lsp")]
    client: std::sync::Arc<tokio::sync::Mutex<Option<LspClient>>>,
}

impl LspTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            #[cfg(feature = "lsp")]
            client: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    #[cfg(feature = "lsp")]
    async fn get_or_init_client(&self, file_path: &str) -> Result<(), ToolError> {
        let mut client_guard = self.client.lock().await;

        if client_guard.is_none() {
            // Detect language server based on file extension
            let server_cmd = Self::detect_language_server(file_path)?;

            let client = LspClient::new(&self.workspace, &server_cmd[0], &server_cmd[1..])
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to start language server: {}", e)))?;

            *client_guard = Some(client);
        }

        Ok(())
    }

    #[cfg(feature = "lsp")]
    fn detect_language_server(file_path: &str) -> Result<Vec<String>, ToolError> {
        let ext = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        match ext {
            "rs" => Ok(vec!["rust-analyzer".to_string()]),
            "ts" | "tsx" | "js" | "jsx" => Ok(vec![
                "typescript-language-server".to_string(),
                "--stdio".to_string(),
            ]),
            "py" => Ok(vec!["pylsp".to_string()]),
            "go" => Ok(vec!["gopls".to_string()]),
            "c" | "cpp" | "cc" | "h" | "hpp" => Ok(vec!["clangd".to_string()]),
            _ => Err(ToolError::ExecutionFailed(format!(
                "No language server configured for .{} files. Supported: .rs (rust-analyzer), .ts/.js (typescript-language-server), .py (pylsp), .go (gopls), .c/.cpp (clangd)",
                ext
            ))),
        }
    }
}

impl Tool for LspTool {
    fn name(&self) -> &str {
        "LSP"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::LSP
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "The LSP operation to perform",
                    "enum": [
                        "goToDefinition",
                        "findReferences",
                        "hover",
                        "documentSymbol",
                        "workspaceSymbol",
                        "goToImplementation",
                        "prepareCallHierarchy",
                        "incomingCalls",
                        "outgoingCalls"
                    ]
                },
                "filePath": {
                    "type": "string",
                    "description": "The file to operate on (relative or absolute path)"
                },
                "line": {
                    "type": "integer",
                    "description": "The line number (1-based)"
                },
                "character": {
                    "type": "integer",
                    "description": "The character offset (1-based)"
                }
            },
            "required": ["operation", "filePath", "line", "character"]
        })
    }

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let operation_str = params["operation"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("operation is required".into()))?;

            let operation: LspOperation = operation_str
                .parse()
                .map_err(|e: String| ToolError::InvalidParams(e))?;

            let file_path = params["filePath"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("filePath is required".into()))?;

            let line = params["line"]
                .as_u64()
                .ok_or_else(|| ToolError::InvalidParams("line is required".into()))? as u32;

            let character = params["character"]
                .as_u64()
                .ok_or_else(|| ToolError::InvalidParams("character is required".into()))? as u32;

            #[cfg(feature = "lsp")]
            {
                // Initialize client if needed
                self.get_or_init_client(file_path).await?;

                let mut client_guard = self.client.lock().await;
                let client = client_guard.as_mut().ok_or_else(|| {
                    ToolError::ExecutionFailed("LSP client not initialized".into())
                })?;

                // Resolve file path
                let full_path = if std::path::Path::new(file_path).is_absolute() {
                    PathBuf::from(file_path)
                } else {
                    self.workspace.join(file_path)
                };

                // Convert to 0-based line/character for LSP
                let line_0 = line.saturating_sub(1);
                let char_0 = character.saturating_sub(1);

                let result = match operation {
                    LspOperation::GoToDefinition => {
                        client.go_to_definition(&full_path, line_0, char_0).await
                    }
                    LspOperation::FindReferences => {
                        client.find_references(&full_path, line_0, char_0).await
                    }
                    LspOperation::Hover => {
                        client.hover(&full_path, line_0, char_0).await
                    }
                    LspOperation::DocumentSymbol => {
                        client.document_symbols(&full_path).await
                    }
                    LspOperation::WorkspaceSymbol => {
                        let query = params["query"].as_str().unwrap_or("");
                        client.workspace_symbols(query).await
                    }
                    LspOperation::GoToImplementation => {
                        client.go_to_implementation(&full_path, line_0, char_0).await
                    }
                    LspOperation::PrepareCallHierarchy => {
                        client.prepare_call_hierarchy(&full_path, line_0, char_0).await
                    }
                    LspOperation::IncomingCalls => {
                        client.incoming_calls(&full_path, line_0, char_0).await
                    }
                    LspOperation::OutgoingCalls => {
                        client.outgoing_calls(&full_path, line_0, char_0).await
                    }
                };

                result.map_err(ToolError::ExecutionFailed)
                    .map(ToolOutput::success)
            }

            #[cfg(not(feature = "lsp"))]
            {
                let _ = (operation, file_path, line, character);
                Err(ToolError::ExecutionFailed(
                    "LSP support not compiled. Rebuild with --features lsp".into()
                ))
            }
        })
    }
}
