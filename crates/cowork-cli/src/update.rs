//! Self-update functionality for the CLI binary.
//!
//! Uses the `self_update` crate to download new releases from GitHub.

use std::time::Duration;

use console::style;
use self_update::cargo_crate_version;

const REPO_OWNER: &str = "szguoxz";
const REPO_NAME: &str = "cowork";

/// Spawn a background version check on startup.
///
/// Prints a single-line notice to stderr if a newer version is available.
/// Silently ignores any errors (network failures, rate limits, etc.)
/// and times out after 5 seconds so it never blocks the user.
pub fn spawn_startup_check() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(startup_check_inner),
        )
        .await;

        if let Ok(Ok(Some(newer))) = result {
            eprintln!(
                "{} A new version ({}) is available. Run {} to update.",
                style("[update]").dim(),
                style(&newer).cyan(),
                style("cowork update").yellow(),
            );
        }
    })
}

/// Returns `Some(version)` if a newer release exists, `None` otherwise.
fn startup_check_inner() -> Option<String> {
    let current = cargo_crate_version!();
    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .ok()?
        .fetch()
        .ok()?;

    let latest = releases.first()?;
    let latest_version = latest.version.trim_start_matches('v');
    if self_update::version::bump_is_greater(current, latest_version).unwrap_or(false) {
        Some(latest_version.to_string())
    } else {
        None
    }
}

/// Run the update command.
///
/// If `check_only` is true, only check for a newer version without installing.
/// Runs the blocking self_update operations on a dedicated thread to avoid
/// conflicts with the tokio runtime.
pub async fn run_update(check_only: bool) -> anyhow::Result<()> {
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
