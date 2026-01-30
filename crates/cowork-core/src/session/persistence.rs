//! Session persistence - save and load session state
//!
//! Handles saving agent sessions to disk and loading them back.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::Result;
use crate::provider::ChatMessage;

/// Saved session state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub id: String,
    pub name: String,
    /// Messages stored using genai's ChatMessage directly
    pub messages: Vec<ChatMessage>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Get the sessions directory path
pub fn get_sessions_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .map(|p| p.join("cowork"))
        .unwrap_or_else(|| PathBuf::from(".cowork"));
    Ok(base.join("sessions"))
}

/// Load a saved session by ID
pub fn load_session(session_id: &str) -> Result<Option<SavedSession>> {
    let path = get_sessions_dir()?.join(format!("{}.json", session_id));
    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read_to_string(&path)?;
    let saved: SavedSession = serde_json::from_str(&json)?;
    Ok(Some(saved))
}

/// List all saved sessions
pub fn list_saved_sessions() -> Result<Vec<SavedSession>> {
    let sessions_dir = get_sessions_dir()?;
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<SavedSession>(&json) {
                    Ok(session) => sessions.push(session),
                    Err(e) => warn!("Failed to parse session {:?}: {}", path, e),
                },
                Err(e) => warn!("Failed to read session {:?}: {}", path, e),
            }
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}
