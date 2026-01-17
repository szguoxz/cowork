//! LSP (Language Server Protocol) tools for code intelligence


use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

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
}

impl LspTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}


impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }

    fn description(&self) -> &str {
        "Interact with Language Server Protocol (LSP) servers to get code intelligence features. \
         Supported operations: goToDefinition (find where a symbol is defined), \
         findReferences (find all references to a symbol), \
         hover (get documentation and type info), \
         documentSymbol (get all symbols in a file), \
         workspaceSymbol (search for symbols across the workspace), \
         goToImplementation (find implementations of an interface), \
         prepareCallHierarchy, incomingCalls, outgoingCalls (call hierarchy features)."
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
                    "description": "The file to operate on"
                },
                "line": {
                    "type": "integer",
                    "description": "The line number (1-based)"
                },
                "character": {
                    "type": "integer",
                    "description": "The character offset (1-based)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query for workspaceSymbol operation"
                }
            },
            "required": ["operation", "filePath", "line", "character"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
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

        // For now, return a placeholder - actual LSP implementation would connect to a language server
        // This is a stub that can be expanded when LSP servers are integrated

        let result = match operation {
            LspOperation::GoToDefinition => {
                json!({
                    "message": "LSP goToDefinition not yet implemented. This would find where the symbol at the given position is defined.",
                    "file": file_path,
                    "line": line,
                    "character": character
                })
            }
            LspOperation::FindReferences => {
                json!({
                    "message": "LSP findReferences not yet implemented. This would find all references to the symbol at the given position.",
                    "file": file_path,
                    "line": line,
                    "character": character
                })
            }
            LspOperation::Hover => {
                json!({
                    "message": "LSP hover not yet implemented. This would show documentation and type information for the symbol.",
                    "file": file_path,
                    "line": line,
                    "character": character
                })
            }
            LspOperation::DocumentSymbol => {
                json!({
                    "message": "LSP documentSymbol not yet implemented. This would list all symbols in the document.",
                    "file": file_path
                })
            }
            LspOperation::WorkspaceSymbol => {
                let query = params["query"].as_str().unwrap_or("");
                json!({
                    "message": "LSP workspaceSymbol not yet implemented. This would search for symbols across the workspace.",
                    "query": query
                })
            }
            LspOperation::GoToImplementation => {
                json!({
                    "message": "LSP goToImplementation not yet implemented. This would find implementations of the interface/trait.",
                    "file": file_path,
                    "line": line,
                    "character": character
                })
            }
            LspOperation::PrepareCallHierarchy => {
                json!({
                    "message": "LSP prepareCallHierarchy not yet implemented. This would prepare call hierarchy information.",
                    "file": file_path,
                    "line": line,
                    "character": character
                })
            }
            LspOperation::IncomingCalls => {
                json!({
                    "message": "LSP incomingCalls not yet implemented. This would find all callers of the function.",
                    "file": file_path,
                    "line": line,
                    "character": character
                })
            }
            LspOperation::OutgoingCalls => {
                json!({
                    "message": "LSP outgoingCalls not yet implemented. This would find all callees of the function.",
                    "file": file_path,
                    "line": line,
                    "character": character
                })
            }
        };

        Ok(ToolOutput::success(result)
            .with_metadata("note", "LSP integration requires language server setup"))
            })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
