//! Cowork Sandbox - Secure execution environment
//!
//! This crate provides sandboxing capabilities for running untrusted code
//! and commands in a secure, isolated environment.

pub mod container;
pub mod policy;
pub mod process;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Root directory for the sandbox
    pub root: PathBuf,
    /// Network access policy
    pub network: NetworkPolicy,
    /// Filesystem access policy
    pub filesystem: FilesystemPolicy,
    /// Resource limits
    pub limits: ResourceLimits,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            root: std::env::temp_dir().join("cowork-sandbox"),
            network: NetworkPolicy::default(),
            filesystem: FilesystemPolicy::default(),
            limits: ResourceLimits::default(),
        }
    }
}

/// Network access policy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct NetworkPolicy {
    /// Allow network access
    pub enabled: bool,
    /// Allowed hosts (if empty, all allowed when enabled)
    pub allowed_hosts: HashSet<String>,
    /// Blocked hosts
    pub blocked_hosts: HashSet<String>,
}

impl NetworkPolicy {
    pub fn allow_all() -> Self {
        Self {
            enabled: true,
            allowed_hosts: HashSet::new(),
            blocked_hosts: HashSet::new(),
        }
    }

    pub fn deny_all() -> Self {
        Self::default()
    }
}

/// Filesystem access policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemPolicy {
    /// Readable paths
    pub read_paths: HashSet<PathBuf>,
    /// Writable paths
    pub write_paths: HashSet<PathBuf>,
    /// Executable paths
    pub exec_paths: HashSet<PathBuf>,
    /// Blocked paths (always denied)
    pub blocked_paths: HashSet<PathBuf>,
}

impl Default for FilesystemPolicy {
    fn default() -> Self {
        let mut blocked = HashSet::new();

        // Platform-specific sensitive paths
        #[cfg(not(windows))]
        {
            blocked.insert(PathBuf::from("/etc/passwd"));
            blocked.insert(PathBuf::from("/etc/shadow"));
            blocked.insert(PathBuf::from("/root"));
        }

        #[cfg(windows)]
        {
            blocked.insert(PathBuf::from("C:\\Windows\\System32\\config"));
            blocked.insert(PathBuf::from("C:\\Windows\\System32\\drivers\\etc\\hosts"));
            blocked.insert(PathBuf::from("C:\\Users\\Administrator"));
        }

        Self {
            read_paths: HashSet::new(),
            write_paths: HashSet::new(),
            exec_paths: HashSet::new(),
            blocked_paths: blocked,
        }
    }
}

/// Resource limits for sandboxed processes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory: u64,
    /// Maximum CPU time in seconds
    pub max_cpu_time: u64,
    /// Maximum number of processes
    pub max_processes: u32,
    /// Maximum file descriptors
    pub max_fds: u32,
    /// Maximum file size in bytes
    pub max_file_size: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory: 512 * 1024 * 1024, // 512 MB
            max_cpu_time: 60,               // 60 seconds
            max_processes: 10,
            max_fds: 100,
            max_file_size: 100 * 1024 * 1024, // 100 MB
        }
    }
}

/// Sandbox execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub memory_used: u64,
    pub killed: bool,
    pub kill_reason: Option<String>,
}

/// Sandbox errors
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Failed to create sandbox: {0}")]
    Creation(String),
    #[error("Execution failed: {0}")]
    Execution(String),
    #[error("Policy violation: {0}")]
    PolicyViolation(String),
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// The main sandbox interface
pub struct Sandbox {
    config: SandboxConfig,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Execute a command in the sandbox
    pub async fn execute(
        &self,
        command: &str,
        args: &[&str],
    ) -> Result<SandboxResult, SandboxError> {
        // Validate command against policy
        self.validate_command(command)?;

        // Execute using process sandboxing
        process::execute_sandboxed(&self.config, command, args).await
    }

    fn validate_command(&self, command: &str) -> Result<(), SandboxError> {
        // Check if command path is allowed
        let path = PathBuf::from(command);

        for blocked in &self.config.filesystem.blocked_paths {
            if path.starts_with(blocked) {
                return Err(SandboxError::PolicyViolation(format!(
                    "Command {} is in blocked path",
                    command
                )));
            }
        }

        Ok(())
    }
}
