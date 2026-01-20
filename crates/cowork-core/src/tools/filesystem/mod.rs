//! Filesystem tools for file operations

mod delete;
mod edit;
mod glob;
mod grep;
mod list;
mod move_file;
mod path_utils;
mod read;
mod search;
mod write;

// Re-export tools
pub use delete::DeleteFile;
pub use edit::EditFile;
pub use glob::GlobFiles;
pub use grep::GrepFiles;
pub use list::ListDirectory;
pub use move_file::MoveFile;
pub use read::ReadFile;
pub use search::SearchFiles;
pub use write::WriteFile;

// Re-export path utilities for use by other modules
pub use path_utils::{
    normalize_path, path_needs_shell_escape, path_to_display, path_to_glob_pattern, path_to_uri,
    shell_escape_path, shell_escape_str, uri_to_path, validate_path, validate_write_path,
};
