//! Sandbox policy management

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::{FilesystemPolicy, NetworkPolicy, ResourceLimits, SandboxConfig};

/// Preset security levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// Maximum restrictions, minimal access
    Paranoid,
    /// Strict restrictions, read-only filesystem
    Strict,
    /// Standard restrictions for most use cases
    Standard,
    /// Relaxed restrictions for trusted code
    Relaxed,
    /// Minimal restrictions (dangerous)
    Permissive,
}

impl SecurityLevel {
    /// Create a sandbox config for this security level
    pub fn to_config(&self, root: PathBuf) -> SandboxConfig {
        match self {
            Self::Paranoid => paranoid_config(root),
            Self::Strict => strict_config(root),
            Self::Standard => standard_config(root),
            Self::Relaxed => relaxed_config(root),
            Self::Permissive => permissive_config(root),
        }
    }
}

fn paranoid_config(root: PathBuf) -> SandboxConfig {
    SandboxConfig {
        root: root.clone(),
        network: NetworkPolicy::deny_all(),
        filesystem: FilesystemPolicy {
            read_paths: [root].into_iter().collect(),
            write_paths: HashSet::new(),
            exec_paths: HashSet::new(),
            blocked_paths: default_blocked_paths(),
        },
        limits: ResourceLimits {
            max_memory: 128 * 1024 * 1024, // 128 MB
            max_cpu_time: 10,
            max_processes: 1,
            max_fds: 20,
            max_file_size: 10 * 1024 * 1024, // 10 MB
        },
    }
}

fn strict_config(root: PathBuf) -> SandboxConfig {
    let mut read_paths: HashSet<PathBuf> = [root.clone()].into_iter().collect();
    let mut exec_paths: HashSet<PathBuf> = HashSet::new();

    #[cfg(not(windows))]
    {
        read_paths.insert(PathBuf::from("/usr"));
        exec_paths.insert(PathBuf::from("/usr/bin"));
        exec_paths.insert(PathBuf::from("/bin"));
    }

    #[cfg(windows)]
    {
        read_paths.insert(PathBuf::from("C:\\Windows\\System32"));
        exec_paths.insert(PathBuf::from("C:\\Windows\\System32"));
    }

    SandboxConfig {
        root: root.clone(),
        network: NetworkPolicy::deny_all(),
        filesystem: FilesystemPolicy {
            read_paths,
            write_paths: [root].into_iter().collect(),
            exec_paths,
            blocked_paths: default_blocked_paths(),
        },
        limits: ResourceLimits {
            max_memory: 256 * 1024 * 1024, // 256 MB
            max_cpu_time: 30,
            max_processes: 5,
            max_fds: 50,
            max_file_size: 50 * 1024 * 1024, // 50 MB
        },
    }
}

fn standard_config(root: PathBuf) -> SandboxConfig {
    let mut read_paths: HashSet<PathBuf> = [root.clone()].into_iter().collect();
    let mut exec_paths: HashSet<PathBuf> = HashSet::new();

    #[cfg(not(windows))]
    {
        read_paths.insert(PathBuf::from("/usr"));
        read_paths.insert(PathBuf::from("/lib"));
        exec_paths.insert(PathBuf::from("/usr/bin"));
        exec_paths.insert(PathBuf::from("/bin"));
        exec_paths.insert(PathBuf::from("/usr/local/bin"));
    }

    #[cfg(windows)]
    {
        read_paths.insert(PathBuf::from("C:\\Windows\\System32"));
        read_paths.insert(PathBuf::from("C:\\Program Files"));
        exec_paths.insert(PathBuf::from("C:\\Windows\\System32"));
        exec_paths.insert(PathBuf::from("C:\\Windows"));
    }

    SandboxConfig {
        root: root.clone(),
        network: NetworkPolicy {
            enabled: true,
            allowed_hosts: HashSet::new(),
            blocked_hosts: ["localhost", "127.0.0.1", "0.0.0.0"]
                .into_iter()
                .map(String::from)
                .collect(),
        },
        filesystem: FilesystemPolicy {
            read_paths,
            write_paths: [root].into_iter().collect(),
            exec_paths,
            blocked_paths: default_blocked_paths(),
        },
        limits: ResourceLimits::default(),
    }
}

fn relaxed_config(root: PathBuf) -> SandboxConfig {
    SandboxConfig {
        root: root.clone(),
        network: NetworkPolicy::allow_all(),
        filesystem: FilesystemPolicy {
            read_paths: HashSet::new(), // Allow reading anywhere not blocked
            write_paths: [root].into_iter().collect(),
            exec_paths: HashSet::new(), // Allow executing anywhere not blocked
            blocked_paths: default_blocked_paths(),
        },
        limits: ResourceLimits {
            max_memory: 1024 * 1024 * 1024, // 1 GB
            max_cpu_time: 300,
            max_processes: 50,
            max_fds: 500,
            max_file_size: 500 * 1024 * 1024, // 500 MB
        },
    }
}

fn permissive_config(root: PathBuf) -> SandboxConfig {
    SandboxConfig {
        root,
        network: NetworkPolicy::allow_all(),
        filesystem: FilesystemPolicy {
            read_paths: HashSet::new(),
            write_paths: HashSet::new(),
            exec_paths: HashSet::new(),
            blocked_paths: minimal_blocked_paths(),
        },
        limits: ResourceLimits {
            max_memory: 4 * 1024 * 1024 * 1024, // 4 GB
            max_cpu_time: 3600,                 // 1 hour
            max_processes: 100,
            max_fds: 1000,
            max_file_size: 1024 * 1024 * 1024, // 1 GB
        },
    }
}

fn default_blocked_paths() -> HashSet<PathBuf> {
    let mut paths = HashSet::new();

    #[cfg(not(windows))]
    {
        for path in [
            "/etc/passwd",
            "/etc/shadow",
            "/etc/sudoers",
            "/root",
            "/home",
            "/var/log",
            "/proc",
            "/sys",
            "/dev",
        ] {
            paths.insert(PathBuf::from(path));
        }
    }

    #[cfg(windows)]
    {
        for path in [
            "C:\\Windows\\System32\\config",
            "C:\\Windows\\System32\\drivers\\etc",
            "C:\\Users",
            "C:\\ProgramData",
            "C:\\Windows\\Logs",
        ] {
            paths.insert(PathBuf::from(path));
        }
    }

    paths
}

fn minimal_blocked_paths() -> HashSet<PathBuf> {
    let mut paths = HashSet::new();

    #[cfg(not(windows))]
    {
        paths.insert(PathBuf::from("/etc/shadow"));
        paths.insert(PathBuf::from("/etc/sudoers"));
    }

    #[cfg(windows)]
    {
        paths.insert(PathBuf::from("C:\\Windows\\System32\\config\\SAM"));
        paths.insert(PathBuf::from("C:\\Windows\\System32\\config\\SECURITY"));
    }

    paths
}

/// Policy builder for custom configurations
pub struct PolicyBuilder {
    root: PathBuf,
    base: SecurityLevel,
    network_overrides: Option<NetworkPolicy>,
    filesystem_overrides: Option<FilesystemPolicy>,
    limit_overrides: Option<ResourceLimits>,
}

impl PolicyBuilder {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            base: SecurityLevel::Standard,
            network_overrides: None,
            filesystem_overrides: None,
            limit_overrides: None,
        }
    }

    pub fn base_level(mut self, level: SecurityLevel) -> Self {
        self.base = level;
        self
    }

    pub fn network(mut self, policy: NetworkPolicy) -> Self {
        self.network_overrides = Some(policy);
        self
    }

    pub fn filesystem(mut self, policy: FilesystemPolicy) -> Self {
        self.filesystem_overrides = Some(policy);
        self
    }

    pub fn limits(mut self, limits: ResourceLimits) -> Self {
        self.limit_overrides = Some(limits);
        self
    }

    pub fn build(self) -> SandboxConfig {
        let mut config = self.base.to_config(self.root);

        if let Some(network) = self.network_overrides {
            config.network = network;
        }

        if let Some(filesystem) = self.filesystem_overrides {
            config.filesystem = filesystem;
        }

        if let Some(limits) = self.limit_overrides {
            config.limits = limits;
        }

        config
    }
}
