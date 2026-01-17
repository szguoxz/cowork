//! Filesystem tools for file operations

mod delete;
mod list;
mod move_file;
mod read;
mod search;
mod write;

pub use delete::DeleteFile;
pub use list::ListDirectory;
pub use move_file::MoveFile;
pub use read::ReadFile;
pub use search::SearchFiles;
pub use write::WriteFile;

use std::path::{Path, PathBuf};

use crate::error::ToolError;

/// Validate that a path is within the workspace boundary
pub fn validate_path(path: &Path, workspace: &Path) -> Result<PathBuf, ToolError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| ToolError::ResourceNotFound(path.display().to_string()))?;

    let workspace_canonical = workspace
        .canonicalize()
        .map_err(|e| ToolError::Io(e))?;

    if canonical.starts_with(&workspace_canonical) {
        Ok(canonical)
    } else {
        Err(ToolError::PermissionDenied(format!(
            "Path {} is outside workspace {}",
            path.display(),
            workspace.display()
        )))
    }
}
