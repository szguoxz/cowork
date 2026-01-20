//! Session Manager for multi-session orchestration
//!
//! Manages multiple concurrent agent sessions, routing inputs and collecting outputs.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use super::agent_loop::AgentLoop;
use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use crate::error::Result;

/// Factory function type for creating session configs
pub type ConfigFactory = Arc<dyn Fn() -> SessionConfig + Send + Sync>;

/// Type alias for the output receiver to simplify complex type
type OutputReceiver = Arc<tokio::sync::Mutex<Option<mpsc::Receiver<(SessionId, SessionOutput)>>>>;

/// Manages multiple concurrent agent sessions
pub struct SessionManager {
    /// Map of session ID to input sender
    sessions: Arc<RwLock<HashMap<SessionId, mpsc::Sender<SessionInput>>>>,
    /// Channel for all session outputs (session_id, output)
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    /// Receiver for outputs (given to consumer via take_output_receiver)
    output_rx: OutputReceiver,
    /// Factory for creating session configs
    config_factory: ConfigFactory,
    /// Channel buffer size for session input/output
    channel_buffer_size: usize,
}

impl SessionManager {
    /// Create a new session manager with the given config factory
    ///
    /// The config factory is called each time a new session is created,
    /// allowing customization of session parameters.
    pub fn new<F>(config_factory: F) -> Self
    where
        F: Fn() -> SessionConfig + Send + Sync + 'static,
    {
        Self::with_buffer_size(config_factory, 256)
    }

    /// Create a new session manager with custom buffer size
    pub fn with_buffer_size<F>(config_factory: F, buffer_size: usize) -> Self
    where
        F: Fn() -> SessionConfig + Send + Sync + 'static,
    {
        let (output_tx, output_rx) = mpsc::channel(buffer_size);

        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            output_tx,
            output_rx: Arc::new(tokio::sync::Mutex::new(Some(output_rx))),
            config_factory: Arc::new(config_factory),
            channel_buffer_size: buffer_size,
        }
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
        let (input_tx, input_rx) = mpsc::channel(self.channel_buffer_size);

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

    /// Take the output receiver
    ///
    /// This can only be called once - subsequent calls will return None.
    /// The receiver yields (session_id, output) tuples from all sessions.
    pub async fn take_output_receiver(
        &self,
    ) -> Option<mpsc::Receiver<(SessionId, SessionOutput)>> {
        let mut rx_guard = self.output_rx.lock().await;
        rx_guard.take()
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
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        let tx = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).cloned()
        };

        if let Some(tx) = tx {
            // Send stop signal
            if let Err(e) = tx.send(SessionInput::Stop).await {
                warn!("Failed to send stop to session {}: {}", session_id, e);
            }

            // Remove from registry
            {
                let mut sessions = self.sessions.write().await;
                sessions.remove(session_id);
            }

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
        }
    }

    #[tokio::test]
    async fn test_session_manager_creation() {
        let manager = SessionManager::new(test_config);
        assert_eq!(manager.session_count().await, 0);
        assert!(manager.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn test_take_output_receiver() {
        let manager = SessionManager::new(test_config);

        // First take should succeed
        let rx1 = manager.take_output_receiver().await;
        assert!(rx1.is_some());

        // Second take should return None
        let rx2 = manager.take_output_receiver().await;
        assert!(rx2.is_none());
    }
}
