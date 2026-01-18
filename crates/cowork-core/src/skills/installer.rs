//! Skill package installer
//!
//! Downloads and installs skill packages from URLs.
//! Skill packages are zip files containing a SKILL.md and optional supporting files.
//!
//! Installation locations:
//! - Global: `~/.claude/skills/<skill-name>/`
//! - Project: `{workspace}/.cowork/skills/<skill-name>/`

use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};

use reqwest::blocking::Client;
use tracing::{debug, info, warn};
use zip::ZipArchive;

use super::loader::{DynamicSkill, SkillLoadError, SkillSource};

/// Location to install skills
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallLocation {
    /// Install globally to ~/.claude/skills/
    Global,
    /// Install to project .cowork/skills/
    Project,
}

impl std::fmt::Display for InstallLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallLocation::Global => write!(f, "global (~/.claude/skills/)"),
            InstallLocation::Project => write!(f, "project (.cowork/skills/)"),
        }
    }
}

/// Result of installing a skill
#[derive(Debug)]
pub struct InstallResult {
    /// Name of the installed skill
    pub name: String,
    /// Where it was installed
    pub location: InstallLocation,
    /// Full path to the skill directory
    pub path: PathBuf,
    /// Description from the skill
    pub description: String,
}

/// Errors that can occur during installation
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("Failed to download skill: {0}")]
    DownloadError(String),

    #[error("Invalid skill package: {0}")]
    InvalidPackage(String),

    #[error("Skill '{0}' already exists at {1}. Use --force to overwrite.")]
    AlreadyExists(String, PathBuf),

    #[error("Failed to extract package: {0}")]
    ExtractError(String),

    #[error("Failed to load skill: {0}")]
    LoadError(#[from] SkillLoadError),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Cannot determine home directory")]
    NoHomeDir,

    #[error("Cannot determine project directory")]
    NoProjectDir,
}

/// Skill package installer
pub struct SkillInstaller {
    /// HTTP client for downloads
    client: Client,
    /// Workspace directory (for project-level installs)
    workspace: PathBuf,
}

impl SkillInstaller {
    /// Create a new installer
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            client: Client::new(),
            workspace,
        }
    }

    /// Get the global skills directory
    pub fn global_skills_dir() -> Result<PathBuf, InstallError> {
        dirs::home_dir()
            .map(|h| h.join(".claude").join("skills"))
            .ok_or(InstallError::NoHomeDir)
    }

    /// Get the project skills directory
    pub fn project_skills_dir(&self) -> PathBuf {
        self.workspace.join(".cowork").join("skills")
    }

    /// Install a skill from a URL
    pub fn install_from_url(
        &self,
        url: &str,
        location: InstallLocation,
        force: bool,
    ) -> Result<InstallResult, InstallError> {
        info!("Downloading skill from {}", url);

        // Download the zip file
        let response = self.client.get(url).send().map_err(|e| {
            InstallError::DownloadError(format!("Failed to fetch {}: {}", url, e))
        })?;

        if !response.status().is_success() {
            return Err(InstallError::DownloadError(format!(
                "HTTP {} when fetching {}",
                response.status(),
                url
            )));
        }

        let bytes = response.bytes().map_err(|e| {
            InstallError::DownloadError(format!("Failed to read response: {}", e))
        })?;

        self.install_from_bytes(&bytes, location, force)
    }

    /// Install a skill from zip bytes
    pub fn install_from_bytes(
        &self,
        bytes: &[u8],
        location: InstallLocation,
        force: bool,
    ) -> Result<InstallResult, InstallError> {
        // Open the zip archive
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| InstallError::InvalidPackage(format!("Not a valid zip file: {}", e)))?;

        // Find SKILL.md in the archive (might be at root or in a subdirectory)
        let skill_md_path = self.find_skill_md(&mut archive)?;
        let prefix = skill_md_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        debug!("Found SKILL.md at: {:?}, prefix: {}", skill_md_path, prefix);

        // Read and parse SKILL.md to get the skill name
        let skill_content = {
            let mut file = archive.by_name(&skill_md_path.to_string_lossy())
                .map_err(|e| InstallError::InvalidPackage(format!("Cannot read SKILL.md: {}", e)))?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            content
        };

        // Parse to get the skill name
        let skill = DynamicSkill::parse(
            &skill_content,
            PathBuf::new(),
            match location {
                InstallLocation::Global => SkillSource::User,
                InstallLocation::Project => SkillSource::Project,
            },
        )?;

        let skill_name = skill.frontmatter.name.clone();
        let skill_description = skill.frontmatter.description.clone();

        // Determine target directory
        let target_dir = match location {
            InstallLocation::Global => Self::global_skills_dir()?.join(&skill_name),
            InstallLocation::Project => self.project_skills_dir().join(&skill_name),
        };

        // Check if already exists
        if target_dir.exists() && !force {
            return Err(InstallError::AlreadyExists(skill_name, target_dir));
        }

        // Remove existing if force
        if target_dir.exists() {
            debug!("Removing existing skill at {}", target_dir.display());
            fs::remove_dir_all(&target_dir)?;
        }

        // Create target directory
        fs::create_dir_all(&target_dir)?;

        // Re-open archive for extraction (need fresh handle)
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| InstallError::InvalidPackage(format!("Not a valid zip file: {}", e)))?;

        // Extract files
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| InstallError::ExtractError(format!("Cannot read archive entry: {}", e)))?;

            let file_path = file.name().to_string();

            // Skip if not under our prefix
            let relative_path = if prefix.is_empty() {
                file_path.clone()
            } else if let Some(stripped) = file_path.strip_prefix(&format!("{}/", prefix)) {
                stripped.to_string()
            } else {
                continue;
            };

            // Skip empty paths and directory markers
            if relative_path.is_empty() || relative_path.ends_with('/') {
                continue;
            }

            let target_path = target_dir.join(&relative_path);

            // Create parent directories
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Extract file
            let mut outfile = fs::File::create(&target_path)?;
            io::copy(&mut file, &mut outfile)?;

            debug!("Extracted: {}", target_path.display());
        }

        info!("Installed skill '{}' to {}", skill_name, target_dir.display());

        Ok(InstallResult {
            name: skill_name,
            location,
            path: target_dir,
            description: skill_description,
        })
    }

    /// Find SKILL.md in the archive
    fn find_skill_md<R: Read + io::Seek>(&self, archive: &mut ZipArchive<R>) -> Result<PathBuf, InstallError> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        for i in 0..archive.len() {
            if let Ok(file) = archive.by_index_raw(i) {
                let name = file.name();
                if name.ends_with("SKILL.md") {
                    candidates.push(PathBuf::from(name));
                }
            }
        }

        if candidates.is_empty() {
            return Err(InstallError::InvalidPackage(
                "No SKILL.md found in package".to_string(),
            ));
        }

        // Prefer the shortest path (most likely at root or single subdirectory)
        candidates.sort_by_key(|p| p.components().count());
        Ok(candidates.remove(0))
    }

    /// Uninstall a skill by name
    pub fn uninstall(&self, name: &str, location: Option<InstallLocation>) -> Result<PathBuf, InstallError> {
        // Try project first if no location specified, then global
        let locations = match location {
            Some(loc) => vec![loc],
            None => vec![InstallLocation::Project, InstallLocation::Global],
        };

        for loc in locations {
            let dir = match loc {
                InstallLocation::Global => Self::global_skills_dir()?.join(name),
                InstallLocation::Project => self.project_skills_dir().join(name),
            };

            if dir.exists() {
                fs::remove_dir_all(&dir)?;
                info!("Uninstalled skill '{}' from {}", name, dir.display());
                return Ok(dir);
            }
        }

        Err(InstallError::InvalidPackage(format!(
            "Skill '{}' not found",
            name
        )))
    }

    /// List installed skills
    pub fn list_installed(&self) -> Vec<InstalledSkill> {
        let mut skills = Vec::new();

        // List global skills
        if let Ok(global_dir) = Self::global_skills_dir() {
            if global_dir.exists() {
                skills.extend(self.list_in_dir(&global_dir, InstallLocation::Global));
            }
        }

        // List project skills
        let project_dir = self.project_skills_dir();
        if project_dir.exists() {
            skills.extend(self.list_in_dir(&project_dir, InstallLocation::Project));
        }

        skills
    }

    /// List skills in a directory
    fn list_in_dir(&self, dir: &Path, location: InstallLocation) -> Vec<InstalledSkill> {
        let mut skills = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    let skill_file = path.join("SKILL.md");
                    if skill_file.exists() {
                        let source = match location {
                            InstallLocation::Global => SkillSource::User,
                            InstallLocation::Project => SkillSource::Project,
                        };

                        match DynamicSkill::load(&path, source) {
                            Ok(skill) => {
                                skills.push(InstalledSkill {
                                    name: skill.frontmatter.name,
                                    description: skill.frontmatter.description,
                                    location,
                                    path: path.clone(),
                                });
                            }
                            Err(e) => {
                                warn!("Failed to load skill at {}: {}", path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        skills
    }
}

/// Information about an installed skill
#[derive(Debug, Clone)]
pub struct InstalledSkill {
    /// Skill name
    pub name: String,
    /// Description
    pub description: String,
    /// Where it's installed
    pub location: InstallLocation,
    /// Path to the skill directory
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    fn create_skill_zip(name: &str, description: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = zip::ZipWriter::new(cursor);

            let options = SimpleFileOptions::default();

            let skill_content = format!(
                r#"---
name: {}
description: {}
---

# {}

Instructions here.
"#,
                name, description, name
            );

            zip.start_file("SKILL.md", options).unwrap();
            zip.write_all(skill_content.as_bytes()).unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn test_install_from_bytes() {
        let workspace = TempDir::new().unwrap();
        let installer = SkillInstaller::new(workspace.path().to_path_buf());

        let zip_bytes = create_skill_zip("test-skill", "A test skill");

        let result = installer
            .install_from_bytes(&zip_bytes, InstallLocation::Project, false)
            .unwrap();

        assert_eq!(result.name, "test-skill");
        assert_eq!(result.location, InstallLocation::Project);
        assert!(result.path.exists());
        assert!(result.path.join("SKILL.md").exists());
    }

    #[test]
    fn test_already_exists() {
        let workspace = TempDir::new().unwrap();
        let installer = SkillInstaller::new(workspace.path().to_path_buf());

        let zip_bytes = create_skill_zip("dup-skill", "First install");

        // First install should succeed
        installer
            .install_from_bytes(&zip_bytes, InstallLocation::Project, false)
            .unwrap();

        // Second install should fail
        let result = installer.install_from_bytes(&zip_bytes, InstallLocation::Project, false);
        assert!(matches!(result, Err(InstallError::AlreadyExists(_, _))));

        // With force should succeed
        let result = installer.install_from_bytes(&zip_bytes, InstallLocation::Project, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_uninstall() {
        let workspace = TempDir::new().unwrap();
        let installer = SkillInstaller::new(workspace.path().to_path_buf());

        let zip_bytes = create_skill_zip("remove-me", "To be removed");

        installer
            .install_from_bytes(&zip_bytes, InstallLocation::Project, false)
            .unwrap();

        let result = installer.uninstall("remove-me", Some(InstallLocation::Project));
        assert!(result.is_ok());

        // Verify it's gone
        let skills = installer.list_installed();
        assert!(!skills.iter().any(|s| s.name == "remove-me"));
    }

    #[test]
    fn test_list_installed() {
        let workspace = TempDir::new().unwrap();
        let installer = SkillInstaller::new(workspace.path().to_path_buf());

        installer
            .install_from_bytes(
                &create_skill_zip("skill-one", "First skill"),
                InstallLocation::Project,
                false,
            )
            .unwrap();

        installer
            .install_from_bytes(
                &create_skill_zip("skill-two", "Second skill"),
                InstallLocation::Project,
                false,
            )
            .unwrap();

        let skills = installer.list_installed();
        assert_eq!(skills.len(), 2);
        assert!(skills.iter().any(|s| s.name == "skill-one"));
        assert!(skills.iter().any(|s| s.name == "skill-two"));
    }
}
