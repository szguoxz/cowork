//! Filesystem tools for file operations

mod delete;
mod edit;
mod glob;
mod grep;
mod list;
mod move_file;
mod read;
mod search;
mod write;

pub use delete::DeleteFile;
pub use edit::EditFile;
pub use glob::GlobFiles;
pub use grep::GrepFiles;
pub use list::ListDirectory;
pub use move_file::MoveFile;
pub use read::ReadFile;
pub use search::SearchFiles;
pub use write::WriteFile;

use std::path::{Component, Path, PathBuf};

use crate::error::ToolError;

/// Normalize a path by resolving `.` and `..` components without filesystem access.
/// This is used for paths that don't exist yet but need to be validated.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(p) => components.push(Component::Prefix(p)),
            Component::RootDir => {
                components.clear();
                components.push(Component::RootDir);
            }
            Component::CurDir => {} // Skip `.`
            Component::ParentDir => {
                // Pop the last component unless we're at root or have no components
                if let Some(last) = components.last() {
                    match last {
                        Component::RootDir | Component::Prefix(_) => {
                            // Can't go above root, ignore the `..`
                        }
                        Component::ParentDir => {
                            // Already have `..`, add another
                            components.push(Component::ParentDir);
                        }
                        Component::Normal(_) => {
                            components.pop();
                        }
                        Component::CurDir => {
                            components.pop();
                        }
                    }
                } else {
                    // No components yet, keep the `..`
                    components.push(Component::ParentDir);
                }
            }
            Component::Normal(c) => components.push(Component::Normal(c)),
        }
    }

    if components.is_empty() {
        PathBuf::from(".")
    } else {
        components.iter().collect()
    }
}

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
