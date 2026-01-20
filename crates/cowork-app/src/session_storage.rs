//! Session storage for persisting chat sessions to JSON files
//!
//! Saves sessions to: ~/.local/share/cowork/sessions/{date}_{id}.json

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use cowork_core::ChatMessage;

/// Serializable session data (without the provider)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: String,
    pub title: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub system_prompt: String,
    pub provider_type: String,
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Session file metadata (for listing without loading full content)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub title: Option<String>,
    pub message_count: usize,
    pub provider_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub file_path: PathBuf,
    pub file_size: u64,
}

/// Session storage manager
pub struct SessionStorage {
    sessions_dir: PathBuf,
}

impl SessionStorage {
    /// Create a new session storage manager
    pub fn new() -> Self {
        let sessions_dir = Self::default_sessions_dir();
        Self { sessions_dir }
    }

    /// Create with a custom sessions directory
    pub fn with_dir(sessions_dir: PathBuf) -> Self {
        Self { sessions_dir }
    }

    /// Get the default sessions directory
    pub fn default_sessions_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(".cowork"))
            .join("cowork")
            .join("sessions")
    }

    /// Get the sessions directory
    pub fn sessions_dir(&self) -> &PathBuf {
        &self.sessions_dir
    }

    /// Ensure the sessions directory exists
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.sessions_dir)
    }

    /// Generate a filename for a session
    fn session_filename(&self, id: &str, created_at: DateTime<Utc>) -> PathBuf {
        let date = created_at.format("%Y-%m-%d");
        let short_id = &id[..8.min(id.len())];
        self.sessions_dir.join(format!("{}_{}.json", date, short_id))
    }

    /// Save a session to disk
    pub fn save(&self, session: &SessionData) -> std::io::Result<PathBuf> {
        self.ensure_dir()?;

        let path = self.session_filename(&session.id, session.created_at);
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load a session from disk by ID
    pub fn load(&self, id: &str) -> std::io::Result<SessionData> {
        // Find the file that matches this ID
        let entries = std::fs::read_dir(&self.sessions_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                // Check if filename contains the ID
                if let Some(filename) = path.file_stem().and_then(|f| f.to_str()) {
                    if filename.contains(&id[..8.min(id.len())]) {
                        return self.load_from_path(&path);
                    }
                }
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Session {} not found", id),
        ))
    }

    /// Load a session from a specific file path
    pub fn load_from_path(&self, path: &PathBuf) -> std::io::Result<SessionData> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// List all saved sessions (metadata only)
    pub fn list(&self) -> std::io::Result<Vec<SessionMetadata>> {
        self.ensure_dir()?;

        let mut sessions = Vec::new();
        let entries = std::fs::read_dir(&self.sessions_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(metadata) = self.load_metadata(&path) {
                    sessions.push(metadata);
                }
            }
        }

        // Sort by updated_at descending (most recent first)
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(sessions)
    }

    /// Load just the metadata from a session file (faster than loading full content)
    fn load_metadata(&self, path: &PathBuf) -> std::io::Result<SessionMetadata> {
        let json = std::fs::read_to_string(path)?;
        let session: SessionData = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let file_size = std::fs::metadata(path)?.len();

        Ok(SessionMetadata {
            id: session.id,
            title: session.title,
            message_count: session.messages.len(),
            provider_type: session.provider_type,
            created_at: session.created_at,
            updated_at: session.updated_at,
            file_path: path.clone(),
            file_size,
        })
    }

    /// Delete a session by ID
    pub fn delete(&self, id: &str) -> std::io::Result<()> {
        let entries = std::fs::read_dir(&self.sessions_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Some(filename) = path.file_stem().and_then(|f| f.to_str()) {
                    if filename.contains(&id[..8.min(id.len())]) {
                        return std::fs::remove_file(&path);
                    }
                }
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Session {} not found", id),
        ))
    }

    /// Delete a session by file path
    pub fn delete_by_path(&self, path: &PathBuf) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    /// Delete sessions older than the specified number of days
    pub fn delete_older_than(&self, days: i64) -> std::io::Result<Vec<String>> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let mut deleted = Vec::new();

        let sessions = self.list()?;
        for session in sessions {
            if session.updated_at < cutoff
                && self.delete_by_path(&session.file_path).is_ok()
            {
                deleted.push(session.id);
            }
        }

        Ok(deleted)
    }

    /// Get total size of all session files
    pub fn total_size(&self) -> std::io::Result<u64> {
        let sessions = self.list()?;
        Ok(sessions.iter().map(|s| s.file_size).sum())
    }

    /// Delete all sessions
    pub fn delete_all(&self) -> std::io::Result<usize> {
        let sessions = self.list()?;
        let count = sessions.len();

        for session in sessions {
            let _ = self.delete_by_path(&session.file_path);
        }

        Ok(count)
    }
}

impl Default for SessionStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a title from the first user message
pub fn generate_title(messages: &[ChatMessage]) -> Option<String> {
    messages
        .iter()
        .find(|m| m.role == "user")
        .map(|m| {
            let content = m.content.trim();
            if content.len() > 50 {
                format!("{}...", &content[..47])
            } else {
                content.to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let storage = SessionStorage::with_dir(dir.path().to_path_buf());

        let session = SessionData {
            id: "test-session-123".to_string(),
            title: Some("Test Session".to_string()),
            messages: vec![ChatMessage {
                id: "msg-1".to_string(),
                role: "user".to_string(),
                content: "Hello".to_string(),
                tool_calls: vec![],
                timestamp: Utc::now(),
            }],
            system_prompt: "Test prompt".to_string(),
            provider_type: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Save
        let path = storage.save(&session).unwrap();
        assert!(path.exists());

        // Load
        let loaded = storage.load("test-session-123").unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.messages.len(), 1);
    }

    #[test]
    fn test_list_sessions() {
        let dir = tempdir().unwrap();
        let storage = SessionStorage::with_dir(dir.path().to_path_buf());

        // Create a few sessions with unique IDs (must differ in first 8 chars)
        for i in 0..3 {
            let session = SessionData {
                id: format!("test{:04}-session", i),
                title: Some(format!("Session {}", i)),
                messages: vec![],
                system_prompt: "Test".to_string(),
                provider_type: "anthropic".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            storage.save(&session).unwrap();
        }

        let list = storage.list().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_delete_session() {
        let dir = tempdir().unwrap();
        let storage = SessionStorage::with_dir(dir.path().to_path_buf());

        let session = SessionData {
            id: "delete-me-123".to_string(),
            title: None,
            messages: vec![],
            system_prompt: "Test".to_string(),
            provider_type: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        storage.save(&session).unwrap();
        assert_eq!(storage.list().unwrap().len(), 1);

        storage.delete("delete-me-123").unwrap();
        assert_eq!(storage.list().unwrap().len(), 0);
    }
}
