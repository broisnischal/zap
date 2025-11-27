use anyhow::{Context, Result};
use colored::Colorize;
use semver::Version;
use serde::Deserialize;

const RELEASE_API: &str = "https://api.github.com/repos/broisnischal/zap/releases/latest";

#[derive(Debug, Deserialize)]
pub struct ReleaseInfo {
    tag_name: String,
    html_url: String,
    body: Option<String>,
}

pub async fn fetch_newer_release(current: &str) -> Result<Option<ReleaseInfo>> {
    let client = reqwest::Client::builder()
        .user_agent("zap/0.1.0 (self-update)")
        .build()
        .context("Failed to create HTTP client for update check")?;

    let resp = client
        .get(RELEASE_API)
        .send()
        .await
        .context("Failed to query GitHub Releases")?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let release: ReleaseInfo = resp
        .json()
        .await
        .context("Failed to parse release metadata")?;

    if is_newer(&release.tag_name, current) {
        Ok(Some(release))
    } else {
        Ok(None)
    }
}

fn normalize(version: &str) -> Option<Version> {
    let cleaned = version.trim_start_matches('v');
    Version::parse(cleaned).ok()
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (normalize(latest), normalize(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => latest != current,
    }
}

pub fn print_update_message(release: &ReleaseInfo) {
    println!();
    println!("{}", "Update Available".yellow().bold());
    println!("{}", "=".repeat(40).bright_black());
    println!(
        "Current version: {}  |  Latest: {}",
        env!("CARGO_PKG_VERSION").green(),
        release.tag_name.cyan()
    );
    println!("Release notes: {}", release.html_url);
    println!();
    println!("To update zap, run one of the following based on your OS:");
    println!();
    println!(
        "  Linux/macOS: {}",
        "curl -fsSL https://raw.githubusercontent.com/broisnischal/zap/main/install.sh | bash -s latest"
            .bright_black()
    );
    println!(
        "  Windows: download the latest zip from {} and replace zap.exe",
        release.html_url.bright_black()
    );
    println!();
}
