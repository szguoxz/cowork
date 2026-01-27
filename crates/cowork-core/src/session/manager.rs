//! Session Manager for multi-session orchestration
//!
//! Manages multiple concurrent agent sessions, routing inputs and collecting outputs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::info;

use super::agent_loop::AgentLoop;
use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use crate::error::Result;
use crate::mcp_manager::McpServerManager;
use crate::orchestration::SystemPrompt;
use crate::prompt::TemplateVars;
use crate::ConfigManager;

/// Type alias for the output receiver
pub type OutputReceiver = mpsc::Receiver<(SessionId, SessionOutput)>;

/// Config source for session creation
enum ConfigSource {
    /// Read from disk each time (for Tauri)
    FromDisk,
    /// Use fixed config (for CLI)
    Fixed(SessionConfig),
}

/// Manages multiple concurrent agent sessions
pub struct SessionManager {
    /// Map of session ID to input sender
    sessions: super::types::SessionRegistry,
    /// Channel for all session outputs (session_id, output)
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    /// Workspace path for building session config
    workspace_path: PathBuf,
    /// Config source - from disk or fixed
    config_source: ConfigSource,
}

impl SessionManager {
    /// Create a new session manager for a workspace
    ///
    /// Config is read fresh from disk when each session is created.
    /// Use this for Tauri where settings can change between sessions.
    pub fn new(workspace_path: PathBuf) -> (Self, OutputReceiver) {
        let (output_tx, output_rx) = mpsc::channel(256);
        let sessions = Arc::new(RwLock::new(HashMap::new()));

        let manager = Self {
            sessions,
            output_tx,
            workspace_path,
            config_source: ConfigSource::FromDisk,
        };

        (manager, output_rx)
    }

    /// Create a session manager with a fixed config
    ///
    /// Use this for CLI where config is set at startup and doesn't change.
    pub fn with_config(config: SessionConfig) -> (Self, OutputReceiver) {
        let (output_tx, output_rx) = mpsc::channel(256);
        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let workspace_path = config.workspace_path.clone();

        let manager = Self {
            sessions,
            output_tx,
            workspace_path,
            config_source: ConfigSource::Fixed(config),
        };

        (manager, output_rx)
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

    /// Create a new session with the given ID
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

        // Get config based on source
        let mut config = match &self.config_source {
            ConfigSource::FromDisk => self.build_session_config(),
            ConfigSource::Fixed(c) => c.clone(),
        };
        config.session_registry = Some(self.sessions.clone());

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

    /// Build session config by reading fresh settings from disk
    fn build_session_config(&self) -> SessionConfig {
        let config_manager = ConfigManager::new().unwrap_or_default();
        let config = config_manager.config();

        // Get provider settings
        let default_provider = config.get_default_provider();
        let approval_level: crate::ApprovalLevel = config
            .approval
            .auto_approve_level
            .parse()
            .unwrap_or(crate::ApprovalLevel::Low);

        let mut tool_approval_config = crate::ToolApprovalConfig::default();
        tool_approval_config.set_level(approval_level);

        // Build system prompt with workspace context and git info
        let system_prompt = self.build_system_prompt(default_provider.as_ref().map(|p| p.model.as_str()));

        let mut session_config = SessionConfig::new(self.workspace_path.clone())
            .with_approval_config(tool_approval_config)
            .with_web_search_config(config.web_search.clone())
            .with_system_prompt(system_prompt);

        if let Some(provider_config) = default_provider {
            let provider_type: crate::provider::ProviderType = provider_config
                .provider_type
                .parse()
                .unwrap_or(crate::provider::ProviderType::Anthropic);

            session_config = session_config.with_provider(provider_type);
            session_config = session_config.with_model(&provider_config.model);
            if let Some(api_key) = provider_config.get_api_key() {
                session_config = session_config.with_api_key(api_key);
            }
        }

        // Create MCP server manager from config if servers are configured
        if !config.mcp_servers.is_empty() {
            let mcp_manager = Arc::new(McpServerManager::with_configs(config.mcp_servers.clone()));

            // Start enabled servers
            let results = mcp_manager.start_enabled();
            for (name, result) in results {
                match result {
                    Ok(()) => info!("Started MCP server: {}", name),
                    Err(e) => tracing::warn!("Failed to start MCP server '{}': {}", name, e),
                }
            }

            session_config = session_config.with_mcp_manager(mcp_manager);
        }

        session_config
    }

    /// Build system prompt with workspace context and git info
    fn build_system_prompt(&self, model_info: Option<&str>) -> String {
        let mut vars = TemplateVars::default();
        vars.working_directory = self.workspace_path.display().to_string();
        vars.is_git_repo = self.workspace_path.join(".git").exists();

        // Get git status and branch info if in a repo
        if vars.is_git_repo {
            if let Ok(output) = std::process::Command::new("git")
                .args(["status", "--short", "--branch"])
                .current_dir(&self.workspace_path)
                .output()
            {
                vars.git_status = String::from_utf8_lossy(&output.stdout).to_string();
            }

            if let Ok(output) = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&self.workspace_path)
                .output()
            {
                vars.current_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }

            if let Ok(output) = std::process::Command::new("git")
                .args(["log", "--oneline", "-5"])
                .current_dir(&self.workspace_path)
                .output()
            {
                vars.recent_commits = String::from_utf8_lossy(&output.stdout).to_string();
            }
        }

        if let Some(info) = model_info {
            vars.model_info = info.to_string();
        }

        SystemPrompt::new()
            .with_template_vars(vars)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_workspace() -> PathBuf {
        std::env::current_dir().unwrap()
    }

    #[tokio::test]
    async fn test_session_manager_creation() {
        let (manager, _output_rx) = SessionManager::new(test_workspace());
        assert_eq!(manager.session_count(), 0);
        assert!(manager.list_sessions().is_empty());
    }

    #[tokio::test]
    async fn test_has_session() {
        let (manager, _output_rx) = SessionManager::new(test_workspace());

        // Session shouldn't exist yet
        assert!(!manager.has_session("test-session"));
    }

    #[tokio::test]
    async fn test_stop_nonexistent_session() {
        let (manager, _output_rx) = SessionManager::new(test_workspace());

        // Stopping a non-existent session should be a no-op
        let result = manager.stop_session("nonexistent");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_all_empty() {
        let (manager, _output_rx) = SessionManager::new(test_workspace());

        // Stopping all when empty should be fine
        let result = manager.stop_all();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_output_sender_clone() {
        let (manager, _output_rx) = SessionManager::new(test_workspace());
        let _sender = manager.output_sender();
        // Just verify we can get a clone of the sender
    }
}
