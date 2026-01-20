# Cross-Platform Path Handling Design Document

## Overview

This document outlines the design for improving cross-platform path handling in Cowork, specifically addressing Windows compatibility issues while maintaining clean, maintainable code.

## Current State

### Existing Infrastructure

The codebase already has a foundation for path handling in `crates/cowork-core/src/tools/filesystem/mod.rs`:

```rust
// Current helpers (already implemented)
pub fn path_to_glob_pattern(path: &Path) -> String;  // Forward slashes for glob crate
pub fn path_to_display(path: &Path) -> String;       // Consistent display format
pub fn normalize_path(path: &Path) -> PathBuf;       // Resolve . and .. without fs access
pub fn validate_path(path: &Path, workspace: &Path) -> Result<PathBuf, ToolError>;  // Security check
```

### Problems Identified

| Issue | Severity | Location | Description |
|-------|----------|----------|-------------|
| Inconsistent path output | Medium | Multiple files | Some use `path_to_display()`, others use `.display().to_string()` |
| Unquoted shell paths | Critical | `shell/execute.rs:144` | Unix shell redirect path not quoted, breaks on spaces |
| Missing URI encoding | High | `lsp/client.rs:39-57` | Paths with special chars (`%`, `#`, spaces) break LSP URIs |
| No shell escaping | Medium | N/A | No helper for escaping paths in shell commands |

### Files with Path Handling (22 total)

**Filesystem Tools (8 files):**
- `read.rs` - Uses `.display().to_string()` ⚠️
- `write.rs` - Uses `.display().to_string()` ⚠️
- `edit.rs` - Uses `.display().to_string()` ⚠️
- `delete.rs` - Uses `.display().to_string()` ⚠️
- `move_file.rs` - Uses `.display().to_string()` ⚠️
- `glob.rs` - Uses `path_to_display()` ✅
- `grep.rs` - Uses `path_to_display()` ✅
- `list.rs` - Uses `path_to_display()` ✅
- `search.rs` - Uses `.display().to_string()` ⚠️

**Document Tools (2 files):**
- `read_pdf.rs` - Uses `.display().to_string()` ⚠️
- `read_office.rs` - Uses `.display().to_string()` ⚠️

**Shell/Sandbox (4 files):**
- `shell/execute.rs` - Platform-specific code, has bug ❌
- `sandbox/lib.rs` - Platform-specific blocked paths ✅
- `sandbox/process.rs` - Platform-specific env setup ✅
- `sandbox/validation.rs` - Path validation ✅

**LSP (1 file):**
- `lsp/client.rs` - Manual URI construction, missing encoding ❌

**Configuration (3 files):**
- `config.rs` - Uses `dirs` crate ✅
- `skills/installer.rs` - Uses `dirs` crate ✅
- `context/monitor.rs` - File watching ✅

**CLI (1 file):**
- `main.rs` - Uses `dunce::canonicalize()` ✅

---

## Proposed Design

### Architecture Decision: Enhanced Centralized Module

**Decision:** Enhance existing `filesystem/mod.rs` with additional helpers, NOT create a separate Windows module.

**Rationale:**
1. Most operations are identical across platforms, only separators differ
2. Rust's `std::path` already handles most cross-platform concerns
3. `#[cfg]` attributes in one location are cleaner than duplicate modules
4. `dunce` crate already solves the main Windows canonicalization issue

### New Module Structure

```
crates/cowork-core/src/tools/filesystem/
├── mod.rs              # Path utilities (enhanced)
├── path_utils.rs       # NEW: Dedicated path utility module
├── read.rs
├── write.rs
├── edit.rs
├── delete.rs
├── move_file.rs
├── glob.rs
├── grep.rs
├── list.rs
└── search.rs
```

### New Path Utilities Module

Create `crates/cowork-core/src/tools/filesystem/path_utils.rs`:

```rust
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
///
/// # Example
/// ```
/// use std::path::Path;
/// // On Windows: "src\\main.rs" -> "src/main.rs"
/// // On Unix: "src/main.rs" -> "src/main.rs"
/// let display = path_to_display(Path::new("src/main.rs"));
/// ```
pub fn path_to_display(path: &Path) -> String {
    // Always use forward slashes for consistent cross-platform output
    path.to_string_lossy().replace('\\', "/")
}

/// Convert a path to a string suitable for glob patterns.
///
/// The `glob` crate expects forward slashes on all platforms.
///
/// # Example
/// ```
/// let pattern = path_to_glob_pattern(&workspace.join("**/*.rs"));
/// // Always uses forward slashes regardless of platform
/// ```
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
///
/// # Example
/// ```
/// let escaped = shell_escape_path(Path::new("/path/with spaces/file.txt"));
/// // Unix: '/path/with spaces/file.txt'
/// // Windows: "/path/with spaces/file.txt"
/// ```
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

/// Check if a path needs escaping for shell use.
///
/// Returns true if the path contains characters that could cause issues:
/// - Spaces
/// - Shell metacharacters: $ ` ! & | ; < > ( ) { } [ ] * ? ~ #
/// - Quotes: ' "
pub fn path_needs_shell_escape(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let special_chars = [
        ' ', '$', '`', '!', '&', '|', ';', '<', '>', '(', ')',
        '{', '}', '[', ']', '*', '?', '~', '#', '\'', '"', '\\'
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
/// Special characters are percent-encoded:
/// - Space -> %20
/// - # -> %23
/// - % -> %25
/// - ? -> %3F
///
/// # Example
/// ```
/// let uri = path_to_uri(Path::new("/home/user/my file.rs"))?;
/// // "file:///home/user/my%20file.rs"
/// ```
pub fn path_to_uri(path: &Path) -> Result<String, ToolError> {
    // Ensure absolute path
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        dunce::canonicalize(path).map_err(|e| {
            ToolError::InvalidParams(format!("Cannot resolve path '{}': {}", path.display(), e))
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
///
/// # Example
/// ```
/// let path = uri_to_path("file:///home/user/my%20file.rs")?;
/// // PathBuf("/home/user/my file.rs")
/// ```
pub fn uri_to_path(uri: &str) -> Result<PathBuf, ToolError> {
    if !uri.starts_with("file://") {
        return Err(ToolError::InvalidParams(format!(
            "Invalid file URI (must start with file://): {}", uri
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
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
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
///
/// # Example
/// ```
/// let normalized = normalize_path(Path::new("/home/user/../admin/./file.txt"));
/// // PathBuf("/home/admin/file.txt")
/// ```
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
///
/// # Errors
/// - `ResourceNotFound` if the path doesn't exist
/// - `PermissionDenied` if the path is outside the workspace
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
            format!("Cannot resolve workspace path '{}': {}", workspace.display(), e),
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

    if normalized.starts_with(&workspace_normalized) {
        Ok(normalized)
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

        // Backslashes should become forward slashes
        assert_eq!(path_to_display(Path::new("src\\main.rs")), "src/main.rs");
    }

    #[test]
    fn test_shell_escape_path() {
        let simple = shell_escape_path(Path::new("/path/to/file"));
        assert!(simple.starts_with('\'') || simple.starts_with('"'));

        let with_space = shell_escape_path(Path::new("/path/with space/file"));
        assert!(with_space.contains("with space"));
    }

    #[test]
    fn test_percent_encode_path() {
        assert_eq!(percent_encode_path("/path/to/file"), "/path/to/file");
        assert_eq!(percent_encode_path("/path/with space"), "/path/with%20space");
        assert_eq!(percent_encode_path("/path#anchor"), "/path%23anchor");
    }

    #[test]
    fn test_percent_decode_path() {
        assert_eq!(percent_decode_path("/path/to/file"), "/path/to/file");
        assert_eq!(percent_decode_path("/path/with%20space"), "/path/with space");
        assert_eq!(percent_decode_path("/path%23anchor"), "/path#anchor");
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path(Path::new("/home/user/../admin/./file.txt")),
            PathBuf::from("/home/admin/file.txt")
        );

        assert_eq!(
            normalize_path(Path::new("./src/../lib/mod.rs")),
            PathBuf::from("lib/mod.rs")
        );
    }

    #[test]
    fn test_path_needs_shell_escape() {
        assert!(!path_needs_shell_escape(Path::new("/simple/path")));
        assert!(path_needs_shell_escape(Path::new("/path with space")));
        assert!(path_needs_shell_escape(Path::new("/path$var")));
        assert!(path_needs_shell_escape(Path::new("/path;cmd")));
    }
}
```

---

## Implementation Plan

### Phase 1: Critical Fixes (P0)

**1.1 Fix shell path quoting bug**

File: `crates/cowork-core/src/tools/shell/execute.rs`

```rust
// BEFORE (line 144):
#[cfg(not(windows))]
let child = Command::new("sh")
    .arg("-c")
    .arg(format!("{} > {} 2>&1", command, output_file))  // BUG: unquoted path
    .current_dir(&working_dir)
    .spawn()?;

// AFTER:
#[cfg(not(windows))]
let child = Command::new("sh")
    .arg("-c")
    .arg(format!("{} > '{}' 2>&1", command, output_file.replace('\'', "'\\''")))
    .current_dir(&working_dir)
    .spawn()?;
```

Or better, use the new helper:
```rust
use crate::tools::filesystem::shell_escape_path;

let escaped_output = shell_escape_path(Path::new(&output_file));
let child = Command::new("sh")
    .arg("-c")
    .arg(format!("{} > {} 2>&1", command, escaped_output))
    .current_dir(&working_dir)
    .spawn()?;
```

### Phase 2: High Priority (P1)

**2.1 Add URI encoding to LSP client**

File: `crates/cowork-core/src/tools/lsp/client.rs`

```rust
// BEFORE:
fn path_to_uri(path: &Path) -> Result<String, ToolError> {
    let abs_path = dunce::canonicalize(path)?;
    #[cfg(windows)]
    {
        Ok(format!("file:///{}", abs_path.display().to_string().replace('\\', "/")))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("file://{}", abs_path.display()))
    }
}

// AFTER:
use crate::tools::filesystem::path_to_uri;
// Just use the centralized helper which handles encoding
```

### Phase 3: Medium Priority (P2)

**3.1 Standardize JSON path output**

Update these files to use `path_to_display()`:

| File | Change |
|------|--------|
| `read.rs` | `.display().to_string()` → `path_to_display(&path)` |
| `write.rs` | `.display().to_string()` → `path_to_display(&path)` |
| `edit.rs` | `.display().to_string()` → `path_to_display(&path)` |
| `delete.rs` | `.display().to_string()` → `path_to_display(&path)` |
| `move_file.rs` | `.display().to_string()` → `path_to_display(&path)` |
| `search.rs` | `.display().to_string()` → `path_to_display(&path)` |
| `read_pdf.rs` | `.display().to_string()` → `path_to_display(&path)` |
| `read_office.rs` | `.display().to_string()` → `path_to_display(&path)` |

### Phase 4: Low Priority (P3)

**4.1 Add comprehensive tests**

Create `crates/cowork-core/src/tools/filesystem/path_utils_test.rs` with:
- Cross-platform path conversion tests
- Shell escaping tests with edge cases
- URI encoding/decoding round-trip tests
- Path validation boundary tests

**4.2 Documentation**

Add doc comments explaining:
- When to use each helper function
- Platform-specific behavior
- Security considerations

---

## Migration Strategy

### Step 1: Create new path_utils.rs module
- Copy functions from mod.rs
- Add new functions (shell_escape, URI encoding)
- Add comprehensive tests

### Step 2: Update mod.rs to re-export
```rust
mod path_utils;
pub use path_utils::*;
```

### Step 3: Update consumers one by one
- Start with critical fixes (shell/execute.rs)
- Then LSP client
- Then filesystem tools
- Run tests after each change

### Step 4: Remove old duplicate code
- Clean up any inline path handling
- Ensure all path operations go through helpers

---

## Testing Strategy

### Unit Tests
- Test each helper function independently
- Test edge cases: empty paths, paths with special chars, very long paths
- Test platform-specific behavior with `#[cfg(test)]`

### Integration Tests
- Test filesystem tools with paths containing spaces
- Test glob patterns on different directory structures
- Test LSP with files having special characters in names

### Manual Testing Checklist
- [ ] Windows: List directory with spaces in path
- [ ] Windows: Glob pattern with backslashes in workspace
- [ ] Windows: LSP go-to-definition with file containing `#` in name
- [ ] Unix: Shell command with file containing single quote
- [ ] Unix: Grep in directory with `$` in name

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking existing functionality | High | Comprehensive test suite, incremental rollout |
| Platform-specific bugs | Medium | CI testing on Windows, macOS, Linux |
| Performance regression from encoding | Low | Encoding is fast, paths are short |
| Edge cases in shell escaping | Medium | Use established patterns, extensive testing |

---

## Success Criteria

1. All filesystem tools work correctly on Windows with:
   - Paths containing spaces
   - Paths with special characters
   - UNC paths (network shares)
   - Long paths (> 260 chars with `\\?\` prefix)

2. All JSON output uses consistent forward-slash format

3. LSP client works with files having special characters in names

4. Shell commands work with paths containing spaces and quotes

5. No regressions in existing Unix functionality

---

## Appendix: File Change Summary

| File | Changes Required |
|------|------------------|
| `filesystem/mod.rs` | Move functions to path_utils.rs, re-export |
| `filesystem/path_utils.rs` | NEW: Centralized path utilities |
| `filesystem/read.rs` | Use `path_to_display()` |
| `filesystem/write.rs` | Use `path_to_display()` |
| `filesystem/edit.rs` | Use `path_to_display()` |
| `filesystem/delete.rs` | Use `path_to_display()` |
| `filesystem/move_file.rs` | Use `path_to_display()` |
| `filesystem/search.rs` | Use `path_to_display()` |
| `document/read_pdf.rs` | Use `path_to_display()` |
| `document/read_office.rs` | Use `path_to_display()` |
| `shell/execute.rs` | Use `shell_escape_path()` for output file |
| `lsp/client.rs` | Use `path_to_uri()` and `uri_to_path()` |
| `cli/main.rs` | No changes (already using dunce) |
