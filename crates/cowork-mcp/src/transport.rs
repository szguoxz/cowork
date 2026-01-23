//! MCP Transport layer implementations

use serde_json::Value;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

/// Transport trait for MCP communication
#[allow(async_fn_in_trait)]
pub trait Transport: Send + Sync {
    async fn send(&mut self, message: Value) -> io::Result<()>;
    async fn receive(&mut self) -> io::Result<Option<Value>>;
    async fn close(&mut self) -> io::Result<()>;
}

/// Stdio transport for subprocess communication
pub struct StdioTransport {
    child: Child,
    reader: Option<BufReader<tokio::process::ChildStdout>>,
}

impl StdioTransport {
    pub async fn spawn(command: &str, args: &[&str]) -> io::Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;

        let stdout = child.stdout.take().ok_or_else(|| {
            io::Error::other("Failed to capture stdout")
        })?;

        Ok(Self {
            child,
            reader: Some(BufReader::new(stdout)),
        })
    }
}

impl Transport for StdioTransport {
    async fn send(&mut self, message: Value) -> io::Result<()> {
        let stdin = self.child.stdin.as_mut().ok_or_else(|| {
            io::Error::other("Stdin not available")
        })?;

        let json = serde_json::to_string(&message)?;
        stdin.write_all(json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        Ok(())
    }

    async fn receive(&mut self) -> io::Result<Option<Value>> {
        let reader = self.reader.as_mut().ok_or_else(|| {
            io::Error::other("Reader not available")
        })?;

        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;

        if n == 0 {
            return Ok(None);
        }

        let value: Value = serde_json::from_str(&line)?;
        Ok(Some(value))
    }

    async fn close(&mut self) -> io::Result<()> {
        self.child.kill().await?;
        Ok(())
    }
}

/// SSE transport for HTTP-based communication
pub struct SseTransport {
    base_url: String,
    client: reqwest::Client,
}

impl SseTransport {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }
}

impl Transport for SseTransport {
    async fn send(&mut self, message: Value) -> io::Result<()> {
        self.client
            .post(&self.base_url)
            .json(&message)
            .send()
            .await
            .map_err(io::Error::other)?;

        Ok(())
    }

    async fn receive(&mut self) -> io::Result<Option<Value>> {
        // SSE receive would need event stream handling
        // Placeholder for now
        Ok(None)
    }

    async fn close(&mut self) -> io::Result<()> {
        Ok(())
    }
}
