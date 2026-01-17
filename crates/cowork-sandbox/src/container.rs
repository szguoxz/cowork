//! Container-based sandboxing (Docker/Podman)

use crate::{SandboxConfig, SandboxError, SandboxResult};

/// Container runtime to use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Docker,
    Podman,
}

impl ContainerRuntime {
    /// Get the command name for this runtime
    pub fn command(&self) -> &str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    /// Check if the runtime is available
    pub async fn is_available(&self) -> bool {
        tokio::process::Command::new(self.command())
            .arg("--version")
            .output()
            .await
            .map(|o: std::process::Output| o.status.success())
            .unwrap_or(false)
    }
}

/// Container sandbox using Docker or Podman
pub struct ContainerSandbox {
    runtime: ContainerRuntime,
    image: String,
    config: SandboxConfig,
}

impl ContainerSandbox {
    pub fn new(runtime: ContainerRuntime, image: impl Into<String>, config: SandboxConfig) -> Self {
        Self {
            runtime,
            image: image.into(),
            config,
        }
    }

    /// Execute a command in a container
    pub async fn execute(
        &self,
        command: &str,
        args: &[&str],
    ) -> Result<SandboxResult, SandboxError> {
        let start = std::time::Instant::now();

        let mut cmd = tokio::process::Command::new(self.runtime.command());
        cmd.arg("run")
            .arg("--rm") // Remove container after execution
            .arg("--network")
            .arg(if self.config.network.enabled {
                "bridge"
            } else {
                "none"
            });

        // Set resource limits
        cmd.arg("--memory")
            .arg(format!("{}b", self.config.limits.max_memory));

        cmd.arg("--cpus").arg("1");

        cmd.arg("--pids-limit")
            .arg(self.config.limits.max_processes.to_string());

        // Mount workspace
        cmd.arg("-v")
            .arg(format!(
                "{}:/workspace:rw",
                self.config.root.display()
            ));

        cmd.arg("-w").arg("/workspace");

        // Security options
        cmd.arg("--security-opt").arg("no-new-privileges");

        // Add the image and command
        cmd.arg(&self.image);
        cmd.arg(command);
        cmd.args(args);

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Execute with timeout
        let timeout = std::time::Duration::from_secs(self.config.limits.max_cpu_time);
        let result: Result<Result<std::process::Output, std::io::Error>, _> =
            tokio::time::timeout(timeout, cmd.output()).await;

        match result {
            Ok(Ok(output)) => Ok(SandboxResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                memory_used: 0,
                killed: !output.status.success() && output.status.code().is_none(),
                kill_reason: None,
            }),
            Ok(Err(e)) => Err(SandboxError::Execution(e.to_string())),
            Err(_) => Ok(SandboxResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
                memory_used: 0,
                killed: true,
                kill_reason: Some("Timeout".to_string()),
            }),
        }
    }

    /// Pull the container image if not present
    pub async fn ensure_image(&self) -> Result<(), SandboxError> {
        let output: std::process::Output = tokio::process::Command::new(self.runtime.command())
            .args(["pull", &self.image])
            .output()
            .await
            .map_err(|e: std::io::Error| SandboxError::Execution(e.to_string()))?;

        if !output.status.success() {
            return Err(SandboxError::Execution(format!(
                "Failed to pull image: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }
}
