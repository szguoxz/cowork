//! Shared update types and helpers for CLI and Tauri app self-update.
//!
//! Provides staging metadata, SHA-256 verification, and the `[auto-update]` marker check.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Marker string in a GitHub release body that enables auto-update.
pub const AUTO_UPDATE_MARKER: &str = "[auto-update]";

/// Metadata for a staged update waiting to be applied on next startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedUpdate {
    /// Version of the staged update.
    pub version: String,
    /// Version that was current when the update was downloaded.
    pub current_version: String,
    /// Target triple the binary was built for.
    pub target: String,
    /// ISO 8601 timestamp of when the download completed.
    pub downloaded_at: String,
    /// Path to the downloaded binary.
    pub binary_path: PathBuf,
    /// SHA-256 hex digest of the downloaded binary.
    pub sha256: String,
    /// Whether the download completed successfully.
    pub complete: bool,
}

/// Returns the base directory for update staging: `<data_dir>/cowork/updates/`.
pub fn updates_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cowork")
        .join("updates")
}

/// Returns the path to the staged update metadata file.
pub fn staged_metadata_path() -> PathBuf {
    updates_dir().join("staged.json")
}

/// Check whether a release body contains the `[auto-update]` marker.
pub fn has_auto_update_marker(body: Option<&str>) -> bool {
    body.is_some_and(|b| b.contains(AUTO_UPDATE_MARKER))
}

/// Read the staged update metadata from disk, returning `None` if missing or unparseable.
pub fn read_staged_update() -> Option<StagedUpdate> {
    let path = staged_metadata_path();
    let data = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Atomically write staged update metadata to disk.
///
/// Writes to a temporary file first, then renames to avoid corruption from concurrent access.
pub fn write_staged_update(staged: &StagedUpdate) -> anyhow::Result<()> {
    let path = staged_metadata_path();
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir)?;

    let data = serde_json::to_string_pretty(staged)?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &data)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Remove staged update metadata and its associated binary.
pub fn clear_staged_update() -> anyhow::Result<()> {
    let path = staged_metadata_path();
    if path.exists() {
        // Try to read and remove the binary too
        if let Some(staged) = read_staged_update() {
            let _ = fs::remove_file(&staged.binary_path);
            // Remove the version directory if empty
            if let Some(parent) = staged.binary_path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Compute the SHA-256 hex digest of a file.
pub fn compute_sha256(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_has_auto_update_marker() {
        assert!(has_auto_update_marker(Some("Release notes\n[auto-update]\nMore info")));
        assert!(has_auto_update_marker(Some("[auto-update]")));
        assert!(!has_auto_update_marker(Some("Just a regular release")));
        assert!(!has_auto_update_marker(None));
    }

    #[test]
    fn test_compute_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.bin");
        {
            let mut f = fs::File::create(&file_path).unwrap();
            f.write_all(b"hello world").unwrap();
        }
        let hash = compute_sha256(&file_path).unwrap();
        // Known SHA-256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_staged_update_serialization() {
        let staged = StagedUpdate {
            version: "1.2.3".to_string(),
            current_version: "1.2.0".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            downloaded_at: "2025-01-01T00:00:00Z".to_string(),
            binary_path: PathBuf::from("/tmp/cowork"),
            sha256: "abc123".to_string(),
            complete: true,
        };
        let json = serde_json::to_string(&staged).unwrap();
        let deserialized: StagedUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.version, "1.2.3");
        assert_eq!(deserialized.complete, true);
    }

    #[test]
    fn test_updates_dir() {
        let dir = updates_dir();
        assert!(dir.ends_with("cowork/updates") || dir.ends_with("cowork\\updates"));
    }
}
