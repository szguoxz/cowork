//! LSP client implementation
//!
//! Manages communication with a language server process via JSON-RPC over stdin/stdout.

use lsp_types::{
    request::{
        DocumentSymbolRequest, GotoDefinition, GotoImplementation, HoverRequest, References,
        WorkspaceSymbolRequest,
    },
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverParams, InitializeParams, InitializeResult, Location, Position,
    ReferenceContext, ReferenceParams, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkspaceSymbolParams,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tracing::debug;

/// LSP client for communicating with a language server
pub struct LspClient {
    process: Child,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    request_id: AtomicU64,
    workspace_root: PathBuf,
    #[allow(dead_code)]
    pending_responses: Mutex<HashMap<u64, Value>>,
}

/// Convert a file path to a file:// URI string
fn path_to_uri(path: &Path) -> Result<String, String> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };

    // Convert to file:// URI
    #[cfg(windows)]
    {
        Ok(format!("file:///{}", abs_path.display().to_string().replace('\\', "/")))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("file://{}", abs_path.display()))
    }
}

/// Extract the file path from a URI
fn uri_to_path(uri: &str) -> String {
    uri.strip_prefix("file://")
        .unwrap_or(uri)
        .to_string()
}

impl LspClient {
    /// Start a new language server and initialize it
    pub async fn new(workspace: &Path, command: &str, args: &[String]) -> Result<Self, String> {
        // Spawn the language server process
        let mut process = Command::new(command)
            .args(args)
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {}", command, e))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| "Failed to open stdin".to_string())?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| "Failed to open stdout".to_string())?;

        let mut client = Self {
            process,
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            request_id: AtomicU64::new(1),
            workspace_root: workspace.to_path_buf(),
            pending_responses: Mutex::new(HashMap::new()),
        };

        // Initialize the server
        client.initialize().await?;

        Ok(client)
    }

    /// Initialize the language server
    async fn initialize(&mut self) -> Result<(), String> {
        let workspace_uri = path_to_uri(&self.workspace_root)?;

        #[allow(deprecated)]
        let params = InitializeParams {
            root_uri: Some(workspace_uri.parse().map_err(|e| format!("Invalid URI: {}", e))?),
            capabilities: lsp_types::ClientCapabilities::default(),
            ..Default::default()
        };

        let _result: InitializeResult = self
            .send_request::<lsp_types::request::Initialize>(params)
            .await?;

        // Send initialized notification
        self.send_notification("initialized", json!({})).await?;

        debug!("LSP server initialized");
        Ok(())
    }

    /// Send a request and wait for response
    async fn send_request<R: lsp_types::request::Request>(
        &self,
        params: R::Params,
    ) -> Result<R::Result, String>
    where
        R::Params: Serialize,
        R::Result: for<'de> Deserialize<'de>,
    {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": R::METHOD,
            "params": params
        });

        self.send_message(&request).await?;
        let response = self.read_response(id).await?;

        if let Some(error) = response.get("error") {
            return Err(format!("LSP error: {:?}", error));
        }

        serde_json::from_value(response["result"].clone())
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Send a notification (no response expected)
    async fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        self.send_message(&notification).await
    }

    /// Send a JSON-RPC message
    async fn send_message(&self, message: &Value) -> Result<(), String> {
        let content = serde_json::to_string(message)
            .map_err(|e| format!("Failed to serialize message: {}", e))?;

        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(header.as_bytes())
            .map_err(|e| format!("Failed to write header: {}", e))?;
        stdin
            .write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write content: {}", e))?;
        stdin
            .flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;

        debug!("Sent LSP message: {}", message);
        Ok(())
    }

    /// Read a response for a specific request ID
    async fn read_response(&self, expected_id: u64) -> Result<Value, String> {
        let mut stdout = self.stdout.lock().await;

        loop {
            // Read headers
            let mut header_line = String::new();
            let mut content_length: Option<usize> = None;

            loop {
                header_line.clear();
                stdout
                    .read_line(&mut header_line)
                    .map_err(|e| format!("Failed to read header: {}", e))?;

                let line = header_line.trim();
                if line.is_empty() {
                    break;
                }

                if let Some(len_str) = line.strip_prefix("Content-Length: ") {
                    content_length = Some(
                        len_str
                            .parse()
                            .map_err(|e| format!("Invalid content length: {}", e))?,
                    );
                }
            }

            let content_length =
                content_length.ok_or_else(|| "Missing Content-Length header".to_string())?;

            // Read content
            let mut content = vec![0u8; content_length];
            std::io::Read::read_exact(&mut *stdout, &mut content)
                .map_err(|e| format!("Failed to read content: {}", e))?;

            let response: Value = serde_json::from_slice(&content)
                .map_err(|e| format!("Failed to parse response: {}", e))?;

            debug!("Received LSP message: {}", response);

            // Check if this is our response
            if let Some(id) = response.get("id").and_then(|v| v.as_u64()) {
                if id == expected_id {
                    return Ok(response);
                }
                // Store for later
                self.pending_responses.lock().await.insert(id, response);
            }
            // Otherwise it's a notification or server request - ignore for now
        }
    }

    /// Go to definition
    pub async fn go_to_definition(
        &self,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Value, String> {
        let uri = path_to_uri(file_path)?;

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri.parse().map_err(|e| format!("{}", e))?),
                position: Position::new(line, character),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result: Option<GotoDefinitionResponse> =
            self.send_request::<GotoDefinition>(params).await?;

        Ok(self.format_definition_response(result))
    }

    /// Find all references
    pub async fn find_references(
        &self,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Value, String> {
        let uri = path_to_uri(file_path)?;

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri.parse().map_err(|e| format!("{}", e))?),
                position: Position::new(line, character),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        };

        let result: Option<Vec<Location>> = self.send_request::<References>(params).await?;

        Ok(self.format_locations(result.unwrap_or_default()))
    }

    /// Get hover information
    pub async fn hover(
        &self,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Value, String> {
        let uri = path_to_uri(file_path)?;

        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri.parse().map_err(|e| format!("{}", e))?),
                position: Position::new(line, character),
            },
            work_done_progress_params: Default::default(),
        };

        let result: Option<Hover> = self.send_request::<HoverRequest>(params).await?;

        Ok(self.format_hover(result))
    }

    /// Get document symbols
    pub async fn document_symbols(&self, file_path: &Path) -> Result<Value, String> {
        let uri = path_to_uri(file_path)?;

        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(uri.parse().map_err(|e| format!("{}", e))?),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result: Option<DocumentSymbolResponse> =
            self.send_request::<DocumentSymbolRequest>(params).await?;

        Ok(self.format_document_symbols(result))
    }

    /// Search workspace symbols
    pub async fn workspace_symbols(&self, query: &str) -> Result<Value, String> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result: Option<lsp_types::WorkspaceSymbolResponse> =
            self.send_request::<WorkspaceSymbolRequest>(params).await?;

        Ok(json!({
            "query": query,
            "symbols": result.map(|r| match r {
                lsp_types::WorkspaceSymbolResponse::Flat(symbols) => {
                    symbols.into_iter().map(|s| json!({
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                        "location": self.format_location(&s.location),
                    })).collect::<Vec<_>>()
                }
                lsp_types::WorkspaceSymbolResponse::Nested(symbols) => {
                    symbols.into_iter().map(|s| json!({
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                    })).collect::<Vec<_>>()
                }
            }).unwrap_or_default()
        }))
    }

    /// Go to implementation
    pub async fn go_to_implementation(
        &self,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Value, String> {
        let uri = path_to_uri(file_path)?;

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri.parse().map_err(|e| format!("{}", e))?),
                position: Position::new(line, character),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result: Option<GotoDefinitionResponse> =
            self.send_request::<GotoImplementation>(params).await?;

        Ok(self.format_definition_response(result))
    }

    // Formatting helpers

    fn format_definition_response(&self, result: Option<GotoDefinitionResponse>) -> Value {
        match result {
            None => json!({ "definitions": [] }),
            Some(GotoDefinitionResponse::Scalar(loc)) => {
                json!({ "definitions": [self.format_location(&loc)] })
            }
            Some(GotoDefinitionResponse::Array(locs)) => {
                json!({ "definitions": self.format_locations(locs) })
            }
            Some(GotoDefinitionResponse::Link(links)) => {
                json!({
                    "definitions": links.into_iter().map(|l| json!({
                        "file": uri_to_path(l.target_uri.as_str()),
                        "line": l.target_range.start.line + 1,
                        "character": l.target_range.start.character + 1,
                    })).collect::<Vec<_>>()
                })
            }
        }
    }

    fn format_locations(&self, locations: Vec<Location>) -> Value {
        json!(locations
            .into_iter()
            .map(|l| self.format_location(&l))
            .collect::<Vec<_>>())
    }

    fn format_location(&self, location: &Location) -> Value {
        json!({
            "file": uri_to_path(location.uri.as_str()),
            "line": location.range.start.line + 1,
            "character": location.range.start.character + 1,
            "end_line": location.range.end.line + 1,
            "end_character": location.range.end.character + 1,
        })
    }

    fn format_hover(&self, hover: Option<Hover>) -> Value {
        match hover {
            None => json!({ "content": null }),
            Some(h) => {
                let content = match h.contents {
                    lsp_types::HoverContents::Scalar(marked) => match marked {
                        lsp_types::MarkedString::String(s) => s,
                        lsp_types::MarkedString::LanguageString(ls) => {
                            format!("```{}\n{}\n```", ls.language, ls.value)
                        }
                    },
                    lsp_types::HoverContents::Array(arr) => arr
                        .into_iter()
                        .map(|m| match m {
                            lsp_types::MarkedString::String(s) => s,
                            lsp_types::MarkedString::LanguageString(ls) => {
                                format!("```{}\n{}\n```", ls.language, ls.value)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                    lsp_types::HoverContents::Markup(markup) => markup.value,
                };

                json!({ "content": content })
            }
        }
    }

    fn format_document_symbols(&self, result: Option<DocumentSymbolResponse>) -> Value {
        match result {
            None => json!({ "symbols": [] }),
            Some(DocumentSymbolResponse::Flat(symbols)) => {
                json!({
                    "symbols": symbols.into_iter().map(|s| json!({
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                        "line": s.location.range.start.line + 1,
                        "character": s.location.range.start.character + 1,
                    })).collect::<Vec<_>>()
                })
            }
            Some(DocumentSymbolResponse::Nested(symbols)) => {
                json!({
                    "symbols": self.format_nested_symbols(symbols)
                })
            }
        }
    }

    fn format_nested_symbols(&self, symbols: Vec<lsp_types::DocumentSymbol>) -> Vec<Value> {
        symbols
            .into_iter()
            .map(|s| {
                let mut obj = json!({
                    "name": s.name,
                    "kind": format!("{:?}", s.kind),
                    "line": s.range.start.line + 1,
                    "character": s.range.start.character + 1,
                });
                if let Some(children) = s.children {
                    obj["children"] = json!(self.format_nested_symbols(children));
                }
                obj
            })
            .collect()
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Try to gracefully shutdown the server
        if let Ok(mut stdin) = self.stdin.try_lock() {
            let shutdown = json!({
                "jsonrpc": "2.0",
                "id": 999999,
                "method": "shutdown",
                "params": null
            });
            let content = serde_json::to_string(&shutdown).unwrap_or_default();
            let header = format!("Content-Length: {}\r\n\r\n", content.len());
            let _ = stdin.write_all(header.as_bytes());
            let _ = stdin.write_all(content.as_bytes());
            let _ = stdin.flush();
        }

        // Kill the process if still running
        let _ = self.process.kill();
    }
}
