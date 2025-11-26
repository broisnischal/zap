use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::{Command, Stdio};

use super::sudo;
use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Pacman package manager backend for Arch Linux (official repos only)
pub struct PacmanBackend;

impl PacmanBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("pacman") {
            anyhow::bail!("pacman is not available on this system");
        }
        Ok(Self)
    }

    fn parse_pacman_search(&self, output: &str) -> Vec<Package> {
        let mut packages = vec![];
        let mut current_name = String::new();
        let mut current_version = String::new();
        let mut current_desc = String::new();
        let mut current_repo = String::new();

        for line in output.lines() {
            if line.starts_with("    ") {
                // Description line (continuation)
                current_desc.push_str(line.trim());
            } else if !line.is_empty() {
                // Save previous package
                if !current_name.is_empty() {
                    packages.push(Package {
                        name: current_name.clone(),
                        version: current_version.clone(),
                        description: if current_desc.is_empty() { None } else { Some(current_desc.clone()) },
                        popularity: 0.0,
                        installed: self.is_installed(&current_name).unwrap_or(false),
                        maintainer: None,
                        url: None,
                        extra: PackageExtra {
                            apt_section: Some(current_repo.clone()), // Reuse apt_section for repo
                            ..Default::default()
                        },
                    });
                }

                // Parse: repo/name version [installed]
                // Example: "extra/vim 9.0.1234-1"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(repo_name) = parts.first() {
                    if let Some((repo, name)) = repo_name.split_once('/') {
                        current_repo = repo.to_string();
                        current_name = name.to_string();
                        current_version = parts.get(1).unwrap_or(&"").to_string();
                        current_desc.clear();
                    }
                }
            }
        }

        // Don't forget the last package
        if !current_name.is_empty() {
            packages.push(Package {
                name: current_name.clone(),
                version: current_version.clone(),
                description: if current_desc.is_empty() { None } else { Some(current_desc) },
                popularity: 0.0,
                installed: self.is_installed(&current_name).unwrap_or(false),
                maintainer: None,
                url: None,
                extra: PackageExtra {
                    apt_section: Some(current_repo),
                    ..Default::default()
                },
            });
        }

        packages
    }

    fn parse_pacman_info(&self, output: &str) -> Option<Package> {
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut url = None;
        let mut depends = vec![];
        let mut license = vec![];

        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "Name" => name = value.to_string(),
                    "Version" => version = value.to_string(),
                    "Description" => description = value.to_string(),
                    "URL" => {
                        if !value.is_empty() && value != "None" {
                            url = Some(value.to_string());
                        }
                    }
                    "Licenses" => {
                        license = value.split_whitespace().map(|s| s.to_string()).collect();
                    }
                    "Depends On" => {
                        if value != "None" {
                            depends = value.split_whitespace()
                                .map(|s| s.split('>').next().unwrap_or(s))
                                .map(|s| s.split('<').next().unwrap_or(s))
                                .map(|s| s.split('=').next().unwrap_or(s))
                                .map(|s| s.to_string())
                                .collect();
                        }
                    }
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return None;
        }

        Some(Package {
            name,
            version,
            description: if description.is_empty() { None } else { Some(description) },
            popularity: 0.0,
            installed: false,
            maintainer: None,
            url,
            extra: PackageExtra {
                depends,
                license,
                ..Default::default()
            },
        })
    }
}

#[async_trait]
impl PackageManager for PacmanBackend {
    fn name(&self) -> &str {
        "pacman (Arch Linux)"
    }

    fn id(&self) -> &str {
        "pacman"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let output = Command::new("pacman")
            .args(["-Ss", query])
            .output()
            .context("Failed to run pacman -Ss")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = self.parse_pacman_search(&stdout);
        packages.truncate(30);

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            let output = Command::new("pacman")
                .args(["-Si", pkg_name])
                .output()
                .context("Failed to run pacman -Si")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(mut pkg) = self.parse_pacman_info(&stdout) {
                    pkg.installed = self.is_installed(&pkg.name)?;
                    results.push(pkg);
                }
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];
        let pkg_names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();

        if pkg_names.is_empty() {
            return Ok(results);
        }

        println!("--> Installing packages with pacman...");

        let mut args = vec!["pacman", "-S", "--noconfirm", "--needed"];
        args.extend(pkg_names.iter().copied());

        let status = sudo::run_sudo(&args).context("Failed to run pacman -S")?;

        let success = status.success();
        for pkg in packages {
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success { None } else { Some("pacman install failed".to_string()) },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("pacman")
            .args(["-Q", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("pacman")
            .args(["-Qn"]) // Native packages only (not AUR)
            .output()?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect())
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        println!("--> Syncing package databases...");
        let _ = sudo::run_sudo_output(&["pacman", "-Sy"]);

        let output = Command::new("pacman")
            .args(["-Qu"])
            .output()
            .context("Failed to check for updates")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        for line in stdout.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            if let Some(name) = parts.first() {
                if let Ok(mut info_results) = self.info(&[name]).await {
                    if let Some(pkg) = info_results.pop() {
                        updates.push(pkg);
                    }
                }
            }
        }

        Ok(updates)
    }
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

