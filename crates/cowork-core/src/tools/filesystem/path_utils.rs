//! Cross-platform path utilities for consistent path handling.
//!
//! This module provides helpers for:
//! - Converting paths to glob patterns (forward slashes)
//! - Displaying paths consistently across platforms
//! - Escaping paths for shell commands
//! - Converting paths to/from file:// URIs
//! - Normalizing and validating paths

use std::path::{Component, Path, PathBuf};

use crate::error::ToolError;

// ============================================================================
// PATH DISPLAY & FORMATTING
// ============================================================================

/// Convert a path to a display string with consistent forward slash separators.
///
/// This ensures paths are displayed consistently across platforms in:
/// - JSON responses
/// - Error messages
/// - Log output
pub fn path_to_display(path: &Path) -> String {
    // Always use forward slashes for consistent cross-platform output
    path.to_string_lossy().replace('\\', "/")
}

/// Convert a path to a string suitable for glob patterns.
///
/// The `glob` crate expects forward slashes on all platforms.
pub fn path_to_glob_pattern(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

// ============================================================================
// SHELL ESCAPING
// ============================================================================

/// Escape a path for safe use in shell commands.
///
/// On Windows (cmd.exe):
/// - Wraps path in double quotes
/// - Escapes internal double quotes
///
/// On Unix (sh/bash):
/// - Wraps path in single quotes
/// - Escapes internal single quotes using '\''
pub fn shell_escape_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();

    #[cfg(windows)]
    {
        // Windows cmd.exe: use double quotes, escape internal double quotes
        let escaped = path_str.replace('"', r#""""#);
        format!("\"{}\"", escaped)
    }

    #[cfg(not(windows))]
    {
        // Unix sh: use single quotes, escape internal single quotes
        // 'path' -> 'path'\''s' for paths containing single quotes
        if path_str.contains('\'') {
            let escaped = path_str.replace('\'', "'\\''");
            format!("'{}'", escaped)
        } else {
            format!("'{}'", path_str)
        }
    }
}

/// Escape a string for safe use in shell commands.
///
/// Same as `shell_escape_path` but works with string slices.
pub fn shell_escape_str(s: &str) -> String {
    #[cfg(windows)]
    {
        let escaped = s.replace('"', r#""""#);
        format!("\"{}\"", escaped)
    }

    #[cfg(not(windows))]
    {
        if s.contains('\'') {
            let escaped = s.replace('\'', "'\\''");
            format!("'{}'", escaped)
        } else {
            format!("'{}'", s)
        }
    }
}

/// Check if a path needs escaping for shell use.
///
/// Returns true if the path contains characters that could cause issues:
/// - Spaces
/// - Shell metacharacters
/// - Quotes
pub fn path_needs_shell_escape(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let special_chars = [
        ' ', '$', '`', '!', '&', '|', ';', '<', '>', '(', ')', '{', '}', '[', ']', '*', '?', '~',
        '#', '\'', '"', '\\',
    ];
    path_str.chars().any(|c| special_chars.contains(&c))
}

// ============================================================================
// URI CONVERSION (for LSP)
// ============================================================================

/// Convert a path to a file:// URI with proper encoding.
///
/// Follows RFC 8089 for file URI scheme:
/// - Unix: file:///path/to/file
/// - Windows: file:///C:/path/to/file
///
/// Special characters are percent-encoded.
pub fn path_to_uri(path: &Path) -> Result<String, ToolError> {
    // Ensure absolute path
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        dunce::canonicalize(path).map_err(|e| {
            ToolError::InvalidParams(format!(
                "Cannot resolve path '{}': {}",
                path.display(),
                e
            ))
        })?
    };

    // Convert to forward slashes and encode special characters
    let path_str = path_to_display(&abs_path);
    let encoded = percent_encode_path(&path_str);

    #[cfg(windows)]
    {
        // Windows: file:///C:/path (three slashes, then drive letter)
        Ok(format!("file:///{}", encoded))
    }

    #[cfg(not(windows))]
    {
        // Unix: file:///path (path already starts with /)
        Ok(format!("file://{}", encoded))
    }
}

/// Extract a path from a file:// URI.
pub fn uri_to_path(uri: &str) -> Result<PathBuf, ToolError> {
    if !uri.starts_with("file://") {
        return Err(ToolError::InvalidParams(format!(
            "Invalid file URI (must start with file://): {}",
            uri
        )));
    }

    // Remove file:// prefix
    let path_part = &uri[7..];

    // Handle Windows file:///C:/path vs Unix file:///path
    #[cfg(windows)]
    let path_str = {
        // On Windows, skip the leading / before drive letter
        let p = path_part.strip_prefix('/').unwrap_or(path_part);
        percent_decode_path(p)
    };

    #[cfg(not(windows))]
    let path_str = percent_decode_path(path_part);

    Ok(PathBuf::from(path_str))
}

/// Percent-encode special characters in a path for URI use.
fn percent_encode_path(path: &str) -> String {
    let mut result = String::with_capacity(path.len() * 2);

    for c in path.chars() {
        match c {
            // Characters that must be encoded in URIs
            ' ' => result.push_str("%20"),
            '#' => result.push_str("%23"),
            '%' => result.push_str("%25"),
            '?' => result.push_str("%3F"),
            '[' => result.push_str("%5B"),
            ']' => result.push_str("%5D"),
            // Keep these as-is (valid in file URIs)
            '/' | ':' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' => {
                result.push(c)
            }
            // Alphanumeric and safe characters
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' => {
                result.push(c)
            }
            // Encode everything else
            c => {
                for byte in c.to_string().bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }

    result
}

/// Percent-decode a path from a URI.
fn percent_decode_path(encoded: &str) -> String {
    let mut result = String::with_capacity(encoded.len());
    let mut chars = encoded.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Try to decode %XX
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2
                && let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            // Invalid encoding, keep as-is
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }

    result
}

// ============================================================================
// PATH NORMALIZATION & VALIDATION
// ============================================================================

/// Normalize a path by resolving `.` and `..` components without filesystem access.
///
/// This is used for:
/// - Security validation of paths that don't exist yet
/// - Path traversal prevention
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(p) => components.push(Component::Prefix(p)),
            Component::RootDir => {
                // Keep only prefix (if any) when we see root
                components.retain(|c| matches!(c, Component::Prefix(_)));
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
                        Component::Normal(_) | Component::CurDir => {
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

/// Validate that a path is within the workspace boundary.
///
/// Uses `dunce::canonicalize()` to avoid UNC path prefix issues on Windows.
pub fn validate_path(path: &Path, workspace: &Path) -> Result<PathBuf, ToolError> {
    // Use dunce::canonicalize which avoids \\?\ prefix on Windows
    let canonical = dunce::canonicalize(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::ResourceNotFound(format!(
                "Path not found: {} (working directory: {})",
                path.display(),
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "unknown".to_string())
            ))
        } else {
            ToolError::Io(e)
        }
    })?;

    let workspace_canonical = dunce::canonicalize(workspace).map_err(|e| {
        ToolError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "Cannot resolve workspace path '{}': {}",
                workspace.display(),
                e
            ),
        ))
    })?;

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

/// Validate a path for writing (path may not exist yet).
///
/// Uses `normalize_path()` instead of `canonicalize()` since the file
/// may not exist yet.
pub fn validate_write_path(path: &Path, workspace: &Path) -> Result<PathBuf, ToolError> {
    let normalized = normalize_path(path);
    let workspace_normalized = normalize_path(workspace);

    // For write validation, we need to ensure the workspace is absolute
    // and the normalized path starts with it
    let workspace_abs = if workspace_normalized.is_absolute() {
        workspace_normalized
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&workspace_normalized))
            .unwrap_or(workspace_normalized)
    };

    let path_abs = if normalized.is_absolute() {
        normalized
    } else {
        workspace_abs.join(&normalized)
    };

    let path_normalized = normalize_path(&path_abs);
    let workspace_normalized = normalize_path(&workspace_abs);

    if path_normalized.starts_with(&workspace_normalized) {
        Ok(path_normalized)
    } else {
        Err(ToolError::PermissionDenied(format!(
            "Path {} is outside workspace {}",
            path.display(),
            workspace.display()
        )))
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_display() {
        // Forward slashes should stay as-is
        assert_eq!(path_to_display(Path::new("src/main.rs")), "src/main.rs");

        // Test with mixed separators (simulating Windows input)
        let path_str = "src\\main.rs";
        assert_eq!(
            path_to_display(Path::new(path_str)),
            path_str.replace('\\', "/")
        );
    }

    #[test]
    fn test_path_to_glob_pattern() {
        assert_eq!(
            path_to_glob_pattern(Path::new("src/lib.rs")),
            "src/lib.rs"
        );
    }

    #[test]
    fn test_shell_escape_str() {
        let escaped = shell_escape_str("simple");
        // Should be quoted
        assert!(escaped.contains("simple"));

        let with_space = shell_escape_str("has space");
        assert!(with_space.contains("has space"));
    }

    #[test]
    fn test_percent_encode_path() {
        assert_eq!(percent_encode_path("/path/to/file"), "/path/to/file");
        assert_eq!(
            percent_encode_path("/path/with space"),
            "/path/with%20space"
        );
        assert_eq!(percent_encode_path("/path#anchor"), "/path%23anchor");
        assert_eq!(percent_encode_path("/100%done"), "/100%25done");
    }

    #[test]
    fn test_percent_decode_path() {
        assert_eq!(percent_decode_path("/path/to/file"), "/path/to/file");
        assert_eq!(
            percent_decode_path("/path/with%20space"),
            "/path/with space"
        );
        assert_eq!(percent_decode_path("/path%23anchor"), "/path#anchor");
    }

    #[test]
    fn test_normalize_path_dots() {
        assert_eq!(
            normalize_path(Path::new("./src/lib.rs")),
            PathBuf::from("src/lib.rs")
        );

        assert_eq!(
            normalize_path(Path::new("src/../lib/mod.rs")),
            PathBuf::from("lib/mod.rs")
        );

        assert_eq!(
            normalize_path(Path::new("src/./lib/../mod.rs")),
            PathBuf::from("src/mod.rs")
        );
    }

    #[test]
    fn test_normalize_path_absolute() {
        #[cfg(not(windows))]
        {
            assert_eq!(
                normalize_path(Path::new("/home/user/../admin/./file.txt")),
                PathBuf::from("/home/admin/file.txt")
            );

            // Can't go above root
            assert_eq!(
                normalize_path(Path::new("/../../etc/passwd")),
                PathBuf::from("/etc/passwd")
            );
        }
    }

    #[test]
    fn test_normalize_path_relative_parent() {
        // Relative path with leading ..
        assert_eq!(
            normalize_path(Path::new("../sibling/file.txt")),
            PathBuf::from("../sibling/file.txt")
        );

        assert_eq!(
            normalize_path(Path::new("../../file.txt")),
            PathBuf::from("../../file.txt")
        );
    }

    #[test]
    fn test_path_needs_shell_escape() {
        assert!(!path_needs_shell_escape(Path::new("/simple/path")));
        assert!(!path_needs_shell_escape(Path::new("simple.txt")));
        assert!(path_needs_shell_escape(Path::new("/path with space")));
        assert!(path_needs_shell_escape(Path::new("/path$var")));
        assert!(path_needs_shell_escape(Path::new("/path;cmd")));
        assert!(path_needs_shell_escape(Path::new("file'name")));
    }

    #[test]
    fn test_uri_roundtrip() {
        // Test with a simple absolute path
        #[cfg(not(windows))]
        {
            let original = PathBuf::from("/tmp/test/file.txt");
            // Create the file for canonicalization to work
            let _ = std::fs::create_dir_all("/tmp/test");
            let _ = std::fs::write(&original, "test");

            if let Ok(uri) = path_to_uri(&original) {
                assert!(uri.starts_with("file://"));
                if let Ok(back) = uri_to_path(&uri) {
                    assert_eq!(back, original);
                }
            }

            let _ = std::fs::remove_file(&original);
        }
    }

    #[test]
    fn test_uri_with_special_chars() {
        let encoded = percent_encode_path("/path/with space/and#hash");
        assert_eq!(encoded, "/path/with%20space/and%23hash");

        let decoded = percent_decode_path(&encoded);
        assert_eq!(decoded, "/path/with space/and#hash");
    }
}
