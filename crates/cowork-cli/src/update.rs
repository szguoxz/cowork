//! Self-update functionality for the CLI binary.
//!
//! - **Background check**: downloads eligible updates to a staging directory.
//! - **Startup apply**: replaces the current binary with a staged update on next launch.
//! - **Manual update**: `cowork update` bypasses the `[auto-update]` marker.
//!
//! Self-update is only enabled for official builds from GitHub CI.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use console::style;
use self_update::cargo_crate_version;

use cowork_core::update::{
    clear_staged_update, compute_sha256, has_auto_update_marker, read_staged_update,
    updates_dir, write_staged_update, StagedUpdate,
};

const REPO_OWNER: &str = "szguoxz";
const REPO_NAME: &str = "cowork";

/// True if built by GitHub CI, false for local builds.
const IS_CI_BUILD: bool = option_env!("GITHUB_ACTIONS").is_some();

// ─── Startup Apply ───────────────────────────────────────────────────────────

/// Apply a previously staged update by replacing the current binary.
///
/// Called early in `main()`. Returns `Ok(true)` if the binary was replaced
/// (the user should be informed to restart).
pub fn apply_staged_update() -> anyhow::Result<bool> {
    if !IS_CI_BUILD {
        return Ok(false);
    }

    let staged = match read_staged_update() {
        Some(s) if s.complete => s,
        _ => return Ok(false),
    };

    // Verify the binary exists
    if !staged.binary_path.exists() {
        tracing::warn!(
            "Staged binary missing at {}; clearing metadata",
            staged.binary_path.display()
        );
        clear_staged_update()?;
        return Ok(false);
    }

    // Verify SHA-256
    let actual_hash = compute_sha256(&staged.binary_path)?;
    if actual_hash != staged.sha256 {
        tracing::warn!(
            "Staged binary checksum mismatch (expected {}, got {}); clearing",
            staged.sha256,
            actual_hash
        );
        clear_staged_update()?;
        return Ok(false);
    }

    // Replace the current binary
    match self_replace::self_replace(&staged.binary_path) {
        Ok(()) => {
            eprintln!(
                "{} Updated to v{} (was v{}). Restart to use the new version.",
                style("[update]").green().bold(),
                style(&staged.version).cyan(),
                style(&staged.current_version).dim(),
            );
            clear_staged_update()?;
            Ok(true)
        }
        Err(e) => {
            tracing::warn!("self_replace failed: {}; staged update preserved", e);
            eprintln!(
                "{} Failed to apply staged update: {}. Run {} with appropriate permissions.",
                style("[update]").yellow(),
                e,
                style("cowork update").cyan(),
            );
            Ok(false)
        }
    }
}

// ─── Background Staging ──────────────────────────────────────────────────────

/// Spawn a background task that downloads eligible updates to staging.
///
/// The update will be applied on the next startup via `apply_staged_update()`.
/// Times out after 30 seconds and silently ignores errors.
/// Only runs for CI builds.
pub fn spawn_startup_check() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {
        if !IS_CI_BUILD {
            return;
        }
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::task::spawn_blocking(background_download_inner),
        )
        .await;

        match result {
            Ok(Ok(Some(version))) => {
                eprintln!(
                    "{} v{} downloaded. Will apply on next start.",
                    style("[update]").dim(),
                    style(&version).cyan(),
                );
            }
            Ok(Ok(None)) => {} // No update available or already staged
            Ok(Err(e)) => tracing::debug!("Background update task panicked: {:?}", e),
            Err(_) => tracing::debug!("Background update check timed out"),
        }
    })
}

/// Inner blocking function for background download.
/// Returns `Some(version)` if a new update was staged, `None` otherwise.
fn background_download_inner() -> Option<String> {
    // If a complete staged update already exists, skip
    if let Some(staged) = read_staged_update() {
        if staged.complete {
            return None;
        }
        // Incomplete staging — clear and retry
        let _ = clear_staged_update();
    }

    let current = cargo_crate_version!();

    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .ok()?
        .fetch()
        .ok()?;

    // Find the latest release with the [auto-update] marker that is newer
    let eligible = releases.iter().find(|r| {
        let version = r.version.trim_start_matches('v');
        let is_newer =
            self_update::version::bump_is_greater(current, version).unwrap_or(false);
        let has_marker = has_auto_update_marker(r.body.as_deref());
        is_newer && has_marker
    })?;

    let version = eligible.version.trim_start_matches('v').to_string();
    let target = self_update::get_target();

    // Find the matching asset
    let asset_name = format!("cowork-cli-{}", target);
    let asset = eligible.assets.iter().find(|a| a.name.starts_with(&asset_name))?;

    // Prepare staging directory
    let stage_dir = updates_dir().join(&version);
    fs::create_dir_all(&stage_dir).ok()?;
    let binary_path = stage_dir.join(binary_name());

    // Write incomplete metadata first (atomic safety)
    let staged = StagedUpdate {
        version: version.clone(),
        current_version: current.to_string(),
        target: target.to_string(),
        downloaded_at: String::new(),
        binary_path: binary_path.clone(),
        sha256: String::new(),
        complete: false,
    };
    write_staged_update(&staged).ok()?;

    // Download the asset
    let archive_path = stage_dir.join(&asset.name);
    download_asset(&asset.download_url, &archive_path).ok()?;

    // Extract the binary from the archive
    extract_binary(&archive_path, &binary_path).ok()?;

    // Set executable permission on unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&binary_path, fs::Permissions::from_mode(0o755));
    }

    // Clean up the archive
    let _ = fs::remove_file(&archive_path);

    // Compute SHA-256 and finalize metadata
    let sha256 = compute_sha256(&binary_path).ok()?;
    let now = chrono::Utc::now().to_rfc3339();

    let staged = StagedUpdate {
        version: version.clone(),
        current_version: current.to_string(),
        target: target.to_string(),
        downloaded_at: now,
        binary_path,
        sha256,
        complete: true,
    };
    write_staged_update(&staged).ok()?;

    Some(version)
}

// ─── Manual Update Command ───────────────────────────────────────────────────

/// Run the update command.
///
/// If `check_only` is true, only check for a newer version without installing.
/// Manual update bypasses the `[auto-update]` marker and works on any release.
/// Only available for CI builds.
pub async fn run_update(check_only: bool) -> anyhow::Result<()> {
    if !IS_CI_BUILD {
        println!(
            "{} Self-update is only available for official releases.",
            style("[update]").yellow()
        );
        println!("This binary was built locally. Please update via your package manager or rebuild from source.");
        return Ok(());
    }

    // Clear any staged update to avoid conflicts
    let _ = clear_staged_update();

    let current = cargo_crate_version!();
    println!(
        "{} current version: {}",
        style("Cowork CLI").bold(),
        style(current).cyan()
    );

    let current = current.to_string();
    tokio::task::spawn_blocking(move || {
        if check_only {
            check_for_update(&current)
        } else {
            perform_update(&current)
        }
    })
    .await??;

    Ok(())
}

/// Check whether a newer release exists on GitHub.
fn check_for_update(current: &str) -> anyhow::Result<()> {
    println!("Checking for updates...");

    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()?
        .fetch()?;

    if let Some(latest) = releases.first() {
        let latest_version = latest.version.trim_start_matches('v');
        if self_update::version::bump_is_greater(current, latest_version)? {
            println!(
                "{} {} is available (you have {})",
                style("Update available:").green().bold(),
                style(latest_version).cyan(),
                style(current).dim()
            );
            println!(
                "Run {} to install.",
                style("cowork update").yellow()
            );
        } else {
            println!("{}", style("Already up to date.").green());
        }
    } else {
        println!("{}", style("No releases found.").yellow());
    }

    Ok(())
}

/// Download and install the latest release, replacing the current binary.
fn perform_update(current: &str) -> anyhow::Result<()> {
    println!("Looking for updates...");

    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("cowork")
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(current)
        .build()?
        .update()?;

    match status {
        self_update::Status::UpToDate(v) => {
            println!(
                "{} (version {})",
                style("Already up to date.").green(),
                v
            );
        }
        self_update::Status::Updated(v) => {
            println!(
                "{} to version {}",
                style("Successfully updated").green().bold(),
                style(v).cyan()
            );
        }
    }

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Download a file from a URL to a local path.
fn download_asset(url: &str, dest: &PathBuf) -> anyhow::Result<()> {
    let response = reqwest::blocking::get(url)?;
    let bytes = response.bytes()?;
    fs::write(dest, &bytes)?;
    Ok(())
}

/// Extract the `cowork` binary from a tar.gz or zip archive.
fn extract_binary(archive_path: &PathBuf, binary_path: &PathBuf) -> anyhow::Result<()> {
    let name = archive_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_from_tar_gz(archive_path, binary_path)
    } else if name.ends_with(".zip") {
        extract_from_zip(archive_path, binary_path)
    } else {
        // Assume it's a raw binary
        fs::copy(archive_path, binary_path)?;
        Ok(())
    }
}

fn extract_from_tar_gz(archive_path: &PathBuf, binary_path: &PathBuf) -> anyhow::Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let bin = binary_name();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if file_name == bin {
            let mut out = fs::File::create(binary_path)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }

    anyhow::bail!("Binary '{}' not found in archive", bin)
}

fn extract_from_zip(archive_path: &PathBuf, binary_path: &PathBuf) -> anyhow::Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let bin = binary_name();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let file_name = std::path::Path::new(&name)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if file_name == bin {
            let mut out = fs::File::create(binary_path)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }

    anyhow::bail!("Binary '{}' not found in archive", bin)
}

/// Returns the binary name for the current platform.
fn binary_name() -> &'static str {
    if cfg!(windows) {
        "cowork.exe"
    } else {
        "cowork"
    }
}
