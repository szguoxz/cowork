//! Session Manager for multi-session orchestration
//!
//! Manages multiple concurrent agent sessions, routing inputs and collecting outputs.

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::info;

use super::agent_loop::AgentLoop;
use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use crate::error::Result;

/// Type alias for the output receiver
pub type OutputReceiver = mpsc::Receiver<(SessionId, SessionOutput)>;

/// Manages multiple concurrent agent sessions
pub struct SessionManager {
    /// Map of session ID to input sender
    sessions: super::types::SessionRegistry,
    /// Channel for all session outputs (session_id, output)
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    /// Base config used for new sessions (RwLock for updates after onboarding)
    base_config: RwLock<SessionConfig>,
}

impl SessionManager {
    /// Create a new session manager with the given base config
    ///
    /// Returns the manager and an output receiver for consuming session outputs.
    pub fn new(mut config: SessionConfig) -> (Self, OutputReceiver) {
        let (output_tx, output_rx) = mpsc::channel(256);

        let sessions = Arc::new(RwLock::new(HashMap::new()));

        // Share the session registry so subagents can register themselves
        config.session_registry = Some(sessions.clone());

        let manager = Self {
            sessions,
            output_tx,
            base_config: RwLock::new(config),
        };

        (manager, output_rx)
    }

    /// Update the base config for new sessions
    ///
    /// Existing sessions are not affected. New sessions will use the updated config.
    pub fn update_config(&self, mut config: SessionConfig) {
        // Preserve the session registry
        config.session_registry = Some(self.sessions.clone());
        *self.base_config.write() = config;
        info!("Session manager config updated");
    }

    /// Push a message to a session
    ///
    /// If the session doesn't exist, it will be created automatically.
    /// Returns an error if the message couldn't be sent.
    pub async fn push_message(&self, session_id: &str, input: SessionInput) -> Result<()> {
         self.get_or_create_session(session_id).await?
            .send(input)
            .await
            .map_err(|e| crate::error::Error::Agent(format!("Failed to send input: {}", e)))
    }

    /// Create a new session with the given ID using the base config
    async fn get_or_create_session(
        &self,
        session_id: &str
    ) -> Result<mpsc::Sender<SessionInput>> {

        if let Some(tx) = self.sessions.read().get(session_id) {
            return Ok(tx.clone());
        }

        info!("Creating new session: {}", session_id);


        // Create input channel for this session
        let (input_tx, input_rx) = mpsc::channel(256);

        // Create the agent loop with current config
        let config = self.base_config.read().clone();
        let agent_loop = AgentLoop::new(
            session_id.to_string(),
            input_rx,
            self.output_tx.clone(),
            config,
        )
        .await?;

        // Spawn the agent loop
        tokio::spawn(agent_loop.run());

        // Register the session
        self.sessions
            .write()
            .insert(session_id.to_string(), input_tx.clone());

        // Emit ready notification
        let _ = self
            .output_tx
            .send((session_id.to_string(), SessionOutput::ready()))
            .await;

        Ok(input_tx)
    }

    /// Get a clone of the output sender (for testing or special cases)
    pub fn output_sender(&self) -> mpsc::Sender<(SessionId, SessionOutput)> {
        self.output_tx.clone()
    }

    /// List active session IDs
    pub fn list_sessions(&self) -> Vec<SessionId> {
        let sessions = self.sessions.read();
        sessions.keys().cloned().collect()
    }

    /// Check if a session exists
    pub fn has_session(&self, session_id: &str) -> bool {
        let sessions = self.sessions.read();
        sessions.contains_key(session_id)
    }

    /// Stop a specific session
    ///
    /// Simply removes the session from the registry, which drops the input sender.
    /// The agent loop will detect the closed channel and save the session before exiting.
    pub fn stop_session(&self, session_id: &str) -> Result<()> {
        if self.sessions.write().remove(session_id).is_some() {
            info!("Stopped session: {}", session_id);
        }
        Ok(())
    }

    /// Stop all sessions
    pub fn stop_all(&self) -> Result<()> {
        self.sessions.write().clear();
        Ok(())
    }

    /// Get the number of active sessions
    pub fn session_count(&self) -> usize {
        let sessions = self.sessions.read();
        sessions.len()
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
            prompt_config: Default::default(),
            component_registry: None,
            tool_scope: None,
            enable_hooks: None,
            save_session: true,
            session_registry: None,
        }
    }

    #[tokio::test]
    async fn test_session_manager_creation() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        assert_eq!(manager.session_count(), 0);
        assert!(manager.list_sessions().is_empty());
    }

    #[tokio::test]
    async fn test_has_session() {
        let (manager, _output_rx) = SessionManager::new(test_config());

        // Session shouldn't exist yet
        assert!(!manager.has_session("test-session"));
    }

    #[tokio::test]
    async fn test_stop_nonexistent_session() {
        let (manager, _output_rx) = SessionManager::new(test_config());

        // Stopping a non-existent session should be a no-op
        let result = manager.stop_session("nonexistent");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_all_empty() {
        let (manager, _output_rx) = SessionManager::new(test_config());

        // Stopping all when empty should be fine
        let result = manager.stop_all();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_output_sender_clone() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        let _sender = manager.output_sender();
        // Just verify we can get a clone of the sender
    }
}
