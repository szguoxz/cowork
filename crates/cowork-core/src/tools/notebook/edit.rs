//! NotebookEdit tool - Edit Jupyter notebook cells
//!
//! Allows editing, inserting, and deleting cells in .ipynb files.


use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// Cell types in Jupyter notebooks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CellType {
    Code,
    Markdown,
    Raw,
}

/// Edit modes for notebook cells
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    Replace,
    Insert,
    Delete,
}

/// Tool for editing Jupyter notebook cells
pub struct NotebookEdit {
    workspace: PathBuf,
}

impl NotebookEdit {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}


impl Tool for NotebookEdit {
    fn name(&self) -> &str {
        "NotebookEdit"
    }

    fn description(&self) -> &str {
        "Completely replaces the contents of a specific cell in a Jupyter notebook (.ipynb file) with new source.\n\n\
         Jupyter notebooks are interactive documents that combine code, text, and visualizations, commonly used for data analysis and scientific computing.\n\
         - The notebook_path parameter must be an absolute path, not a relative path\n\
         - Use edit_mode=insert to add a new cell at the specified position\n\
         - Use edit_mode=delete to delete the cell at the specified position"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "Absolute path to the Jupyter notebook file"
                },
                "cell_id": {
                    "type": "string",
                    "description": "ID of the cell to edit. For insert, new cell is added after this cell."
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source code or markdown for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "description": "Type of cell: 'code' or 'markdown'. Required for insert.",
                    "enum": ["code", "markdown"]
                },
                "edit_mode": {
                    "type": "string",
                    "description": "Edit mode: 'replace', 'insert', or 'delete'",
                    "enum": ["replace", "insert", "delete"],
                    "default": "replace"
                }
            },
            "required": ["notebook_path", "new_source"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let notebook_path = params["notebook_path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("notebook_path is required".into()))?;

        let path = if notebook_path.starts_with('/') {
            PathBuf::from(notebook_path)
        } else {
            self.workspace.join(notebook_path)
        };

        if !path.exists() {
            return Err(ToolError::ResourceNotFound(format!(
                "Notebook not found: {}",
                path.display()
            )));
        }

        if path.extension().map(|e| e != "ipynb").unwrap_or(true) {
            return Err(ToolError::InvalidParams(
                "File must be a Jupyter notebook (.ipynb)".into(),
            ));
        }

        let new_source = params["new_source"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("new_source is required".into()))?;

        let edit_mode = match params["edit_mode"].as_str() {
            Some("insert") => EditMode::Insert,
            Some("delete") => EditMode::Delete,
            _ => EditMode::Replace,
        };

        let cell_type = match params["cell_type"].as_str() {
            Some("markdown") => CellType::Markdown,
            Some("raw") => CellType::Raw,
            _ => CellType::Code,
        };

        let cell_id = params["cell_id"].as_str();

        // Read notebook
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read notebook: {}", e)))?;

        let mut notebook: Value = serde_json::from_str(&content)
            .map_err(|e| ToolError::ExecutionFailed(format!("Invalid notebook format: {}", e)))?;

        // Get cells array
        let cells = notebook
            .get_mut("cells")
            .and_then(|c| c.as_array_mut())
            .ok_or_else(|| ToolError::ExecutionFailed("Invalid notebook: no cells array".into()))?;

        match edit_mode {
            EditMode::Replace => {
                let cell_id = cell_id
                    .ok_or_else(|| ToolError::InvalidParams("cell_id required for replace".into()))?;

                let cell = find_cell_by_id_mut(cells, cell_id).ok_or_else(|| {
                    ToolError::ResourceNotFound(format!("Cell not found: {}", cell_id))
                })?;

                // Update source
                let source_lines: Vec<&str> = new_source.lines().collect();
                cell["source"] = json!(source_lines
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        if i < source_lines.len() - 1 {
                            format!("{}\n", line)
                        } else {
                            line.to_string()
                        }
                    })
                    .collect::<Vec<_>>());

                // Update cell type if specified
                cell["cell_type"] = json!(match cell_type {
                    CellType::Code => "code",
                    CellType::Markdown => "markdown",
                    CellType::Raw => "raw",
                });
            }
            EditMode::Insert => {
                let new_cell_id = uuid::Uuid::new_v4().to_string();
                let source_lines: Vec<String> = new_source
                    .lines()
                    .enumerate()
                    .map(|(i, line)| {
                        if i < new_source.lines().count() - 1 {
                            format!("{}\n", line)
                        } else {
                            line.to_string()
                        }
                    })
                    .collect();

                let new_cell = match cell_type {
                    CellType::Code => json!({
                        "cell_type": "code",
                        "execution_count": null,
                        "id": new_cell_id,
                        "metadata": {},
                        "outputs": [],
                        "source": source_lines
                    }),
                    CellType::Markdown => json!({
                        "cell_type": "markdown",
                        "id": new_cell_id,
                        "metadata": {},
                        "source": source_lines
                    }),
                    CellType::Raw => json!({
                        "cell_type": "raw",
                        "id": new_cell_id,
                        "metadata": {},
                        "source": source_lines
                    }),
                };

                // Find insertion point
                let insert_idx = if let Some(id) = cell_id {
                    cells
                        .iter()
                        .position(|c| c.get("id").and_then(|i| i.as_str()) == Some(id))
                        .map(|i| i + 1)
                        .unwrap_or(cells.len())
                } else {
                    0
                };

                cells.insert(insert_idx, new_cell);
            }
            EditMode::Delete => {
                let cell_id = cell_id
                    .ok_or_else(|| ToolError::InvalidParams("cell_id required for delete".into()))?;

                let idx = cells
                    .iter()
                    .position(|c| c.get("id").and_then(|i| i.as_str()) == Some(cell_id))
                    .ok_or_else(|| {
                        ToolError::ResourceNotFound(format!("Cell not found: {}", cell_id))
                    })?;

                cells.remove(idx);
            }
        }

        // Write back
        let output = serde_json::to_string_pretty(&notebook)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to serialize: {}", e)))?;

        tokio::fs::write(&path, output)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write: {}", e)))?;

        Ok(ToolOutput::success(json!({
            "success": true,
            "path": path.display().to_string(),
            "operation": match edit_mode {
                EditMode::Replace => "replaced",
                EditMode::Insert => "inserted",
                EditMode::Delete => "deleted",
            }
        })))
            })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::Medium
    }
}

fn find_cell_by_id_mut<'a>(cells: &'a mut [Value], id: &str) -> Option<&'a mut Value> {
    cells
        .iter_mut()
        .find(|c| c.get("id").and_then(|i| i.as_str()) == Some(id))
}
