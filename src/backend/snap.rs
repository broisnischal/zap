use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Snap package manager backend
pub struct SnapBackend;

impl SnapBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("snap") {
            anyhow::bail!("Snap is not available on this system");
        }
        Ok(Self)
    }

    fn parse_snap_find(&self, output: &str) -> Vec<Package> {
        let mut packages = vec![];

        for line in output.lines() {
            // Skip header
            if line.starts_with("Name") || line.is_empty() {
                continue;
            }

            // Format: "Name  Version  Publisher  Notes  Summary"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let name = parts[0].to_string();
                let version = parts[1].to_string();
                let maintainer = Some(parts[2].to_string());
                // Summary is everything after Notes
                let summary = parts[4..].join(" ");

                packages.push(Package {
                    name,
                    version,
                    description: Some(summary),
                    popularity: 0.0,
                    installed: false,
                    maintainer,
                    url: None,
                    extra: PackageExtra::default(),
                });
            }
        }

        packages
    }

    fn parse_snap_info(&self, output: &str) -> Option<Package> {
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut maintainer = None;
        let mut url = None;
        let mut in_description = false;

        for line in output.lines() {
            if in_description {
                if line.starts_with("  ") {
                    description.push(' ');
                    description.push_str(line.trim());
                    continue;
                } else if !line.contains(':') {
                    description.push(' ');
                    description.push_str(line.trim());
                    continue;
                } else {
                    in_description = false;
                }
            }

            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "name" => name = value.to_string(),
                    "version" | "installed" => {
                        if version.is_empty() {
                            version = value.split_whitespace().next().unwrap_or(value).to_string();
                        }
                    }
                    "summary" => description = value.to_string(),
                    "description" => {
                        if description.is_empty() || value.len() > description.len() {
                            description = value.to_string();
                            in_description = true;
                        }
                    }
                    "publisher" => maintainer = Some(value.to_string()),
                    "store-url" | "contact" => {
                        if url.is_none() && !value.is_empty() {
                            url = Some(value.to_string());
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
            description: if description.is_empty() {
                None
            } else {
                Some(description.trim().to_string())
            },
            popularity: 0.0,
            installed: false,
            maintainer,
            url,
            extra: PackageExtra::default(),
        })
    }
}

#[async_trait]
impl PackageManager for SnapBackend {
    fn name(&self) -> &str {
        "Snap"
    }

    fn id(&self) -> &str {
        "snap"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let output = Command::new("snap")
            .args(["find", query])
            .output()
            .context("Failed to run snap find")?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = self.parse_snap_find(&stdout);

        // Check which are installed
        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        packages.truncate(30);
        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            let output = Command::new("snap")
                .args(["info", pkg_name])
                .output()
                .context("Failed to run snap info")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(mut pkg) = self.parse_snap_info(&stdout) {
                    pkg.installed = self.is_installed(&pkg.name)?;
                    results.push(pkg);
                }
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        for pkg in packages {
            println!("--> Installing {} with snap...", pkg.name);

            let status = Command::new("sudo")
                .args(["snap", "install", &pkg.name])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run snap install")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() {
                    None
                } else {
                    Some("snap install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("snap")
            .args(["list", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("snap").args(["list"]).output()?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .skip(1) // Skip header
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
        let output = Command::new("snap")
            .args(["refresh", "--list"])
            .output()
            .context("Failed to check for updates")?;

        if !output.status.success() {
            // No updates available returns non-zero
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        for line in stdout.lines() {
            if line.starts_with("Name") || line.is_empty() {
                continue;
            }

            let parts: Vec<_> = line.split_whitespace().collect();
            if let Some(name) = parts.first() {
                if let Ok(info_results) = self.info(&[name]).await {
                    if let Some(pkg) = info_results.into_iter().next() {
                        updates.push(pkg);
                    }
                }
            }
        }

        Ok(updates)
    }

    async fn update(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        if packages.is_empty() {
            // Update all
            println!("--> Updating all snap packages...");
            let status = Command::new("sudo")
                .args(["snap", "refresh"])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run snap refresh")?;

            results.push(InstallResult {
                package: "all".to_string(),
                success: status.success(),
                message: if status.success() {
                    None
                } else {
                    Some("snap refresh failed".to_string())
                },
            });
        } else {
            for pkg in packages {
                println!("--> Updating {}...", pkg.name);
                let status = Command::new("sudo")
                    .args(["snap", "refresh", &pkg.name])
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .context("Failed to run snap refresh")?;

                results.push(InstallResult {
                    package: pkg.name.clone(),
                    success: status.success(),
                    message: if status.success() {
                        None
                    } else {
                        Some("snap refresh failed".to_string())
                    },
                });
            }
        }

        Ok(results)
    }
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
