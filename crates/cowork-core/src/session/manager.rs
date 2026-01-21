//! Session Manager for multi-session orchestration
//!
//! Manages multiple concurrent agent sessions, routing inputs and collecting outputs.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

use super::agent_loop::AgentLoop;
use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use crate::error::Result;

/// Factory function type for creating session configs
pub type ConfigFactory = Arc<dyn Fn() -> SessionConfig + Send + Sync>;

/// Type alias for the output receiver
pub type OutputReceiver = mpsc::Receiver<(SessionId, SessionOutput)>;

/// Manages multiple concurrent agent sessions
pub struct SessionManager {
    /// Map of session ID to input sender
    sessions: Arc<RwLock<HashMap<SessionId, mpsc::Sender<SessionInput>>>>,
    /// Channel for all session outputs (session_id, output)
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    /// Factory for creating session configs
    config_factory: ConfigFactory,
}

impl SessionManager {
    /// Create a new session manager with the given config factory
    ///
    /// Returns the manager and an output receiver for consuming session outputs.
    /// The config factory is called each time a new session is created,
    /// allowing customization of session parameters.
    pub fn new<F>(config_factory: F) -> (Self, OutputReceiver)
    where
        F: Fn() -> SessionConfig + Send + Sync + 'static,
    {
        let (output_tx, output_rx) = mpsc::channel(256);

        let manager = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            output_tx,
            config_factory: Arc::new(config_factory),
        };

        (manager, output_rx)
    }

    /// Push a message to a session
    ///
    /// If the session doesn't exist, it will be created automatically.
    /// Returns an error if the message couldn't be sent.
    pub async fn push_message(&self, session_id: &str, input: SessionInput) -> Result<()> {
        let session_id = session_id.to_string();

        // Check if session exists
        let tx = {
            let sessions = self.sessions.read().await;
            sessions.get(&session_id).cloned()
        };

        let tx = match tx {
            Some(tx) => tx,
            None => {
                // Create new session
                self.create_session(&session_id).await?
            }
        };

        // Send the input
        tx.send(input)
            .await
            .map_err(|e| crate::error::Error::Agent(format!("Failed to send input: {}", e)))?;

        Ok(())
    }

    /// Create a new session with the given ID
    ///
    /// Returns the input sender for the session
    async fn create_session(&self, session_id: &str) -> Result<mpsc::Sender<SessionInput>> {
        let session_id = session_id.to_string();
        info!("Creating new session: {}", session_id);

        // Create input channel for this session
        let (input_tx, input_rx) = mpsc::channel(256);

        // Get config from factory
        let config = (self.config_factory)();

        // Create the agent loop
        let agent_loop = AgentLoop::new(
            session_id.clone(),
            input_rx,
            self.output_tx.clone(),
            config,
        )
        .await?;

        // Spawn the agent loop
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            debug!("Agent loop starting for session: {}", session_id_clone);
            agent_loop.run().await;
            debug!("Agent loop ended for session: {}", session_id_clone);
        });

        // Register the session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), input_tx.clone());
        }

        // Emit ready notification
        let _ = self
            .output_tx
            .send((session_id, SessionOutput::ready()))
            .await;

        Ok(input_tx)
    }

    /// Get a clone of the output sender (for testing or special cases)
    pub fn output_sender(&self) -> mpsc::Sender<(SessionId, SessionOutput)> {
        self.output_tx.clone()
    }

    /// List active session IDs
    pub async fn list_sessions(&self) -> Vec<SessionId> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    /// Check if a session exists
    pub async fn has_session(&self, session_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(session_id)
    }

    /// Stop a specific session
    ///
    /// Simply removes the session from the registry, which drops the input sender.
    /// The agent loop will detect the closed channel and save the session before exiting.
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            info!("Stopped session: {}", session_id);
        }
        Ok(())
    }

    /// Stop all sessions
    pub async fn stop_all(&self) -> Result<()> {
        let session_ids: Vec<String> = {
            let sessions = self.sessions.read().await;
            sessions.keys().cloned().collect()
        };

        for session_id in session_ids {
            self.stop_session(&session_id).await?;
        }

        Ok(())
    }

    /// Get the number of active sessions
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    /// Remove a session from the registry (called when session ends)
    #[allow(dead_code)]
    pub(crate) async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        debug!("Removed session from registry: {}", session_id);
    }
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        // Note: We can't async drop, so sessions will be dropped when their
        // senders are dropped, which will cause the agent loops to exit
        debug!("SessionManager dropping");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionConfig;

    fn test_config() -> SessionConfig {
        SessionConfig {
            workspace_path: std::env::current_dir().unwrap(),
            approval_config: crate::approval::ToolApprovalConfig::trust_all(),
            system_prompt: Some("You are a test assistant.".to_string()),
            provider_type: crate::provider::ProviderType::Anthropic,
            model: None,
            api_key: None,
            web_search_config: None,
        }
    }

    #[tokio::test]
    async fn test_session_manager_creation() {
        let (manager, _output_rx) = SessionManager::new(test_config);
        assert_eq!(manager.session_count().await, 0);
        assert!(manager.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn test_has_session() {
        let (manager, _output_rx) = SessionManager::new(test_config);

        // Session shouldn't exist yet
        assert!(!manager.has_session("test-session").await);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_session() {
        let (manager, _output_rx) = SessionManager::new(test_config);

        // Stopping a non-existent session should be a no-op
        let result = manager.stop_session("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_all_empty() {
        let (manager, _output_rx) = SessionManager::new(test_config);

        // Stopping all when empty should be fine
        let result = manager.stop_all().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_output_sender_clone() {
        let (manager, _output_rx) = SessionManager::new(test_config);
        let _sender = manager.output_sender();
        // Just verify we can get a clone of the sender
    }

    #[tokio::test]
    async fn test_remove_session() {
        let (manager, _output_rx) = SessionManager::new(test_config);

        // Add a session entry directly for testing
        {
            let (tx, _rx) = mpsc::channel(1);
            let mut sessions = manager.sessions.write().await;
            sessions.insert("test-session".to_string(), tx);
        }

        assert!(manager.has_session("test-session").await);

        // Remove it
        manager.remove_session("test-session").await;

        assert!(!manager.has_session("test-session").await);
    }
}
