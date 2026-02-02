//! Filesystem tools for file operations

mod document;
mod edit;
mod export;
mod glob;
mod grep;
mod path_utils;
mod read;
mod write;

// Re-export tools
pub use edit::EditFile;
pub use export::ExportDocument;
pub use glob::GlobFiles;
pub use grep::GrepFiles;
pub use read::ReadFile;
pub use write::WriteFile;

// Re-export path utilities for use by other modules
pub use path_utils::{
    normalize_path, path_needs_shell_escape, path_to_display, path_to_glob_pattern, path_to_uri,
    percent_decode_path, percent_encode_path, shell_escape_path, shell_escape_str, uri_to_path,
    validate_path, validate_write_path,
};
