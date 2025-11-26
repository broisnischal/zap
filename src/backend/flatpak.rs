use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Flatpak package manager backend
pub struct FlatpakBackend;

impl FlatpakBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("flatpak") {
            anyhow::bail!("Flatpak is not available on this system");
        }
        Ok(Self)
    }

    fn parse_flatpak_search(&self, output: &str) -> Vec<Package> {
        let mut packages = vec![];

        for line in output.lines() {
            if line.is_empty() {
                continue;
            }

            // Format varies, but common: "Name    Description    Application ID    Version    Remotes"
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let name = parts[0].trim().to_string();
                let description = parts.get(1).map(|s| s.trim().to_string());
                let app_id = parts.get(2).map(|s| s.trim().to_string()).unwrap_or_default();
                let version = parts.get(3).map(|s| s.trim().to_string()).unwrap_or_default();

                // Use app_id as the actual name for flatpak
                let pkg_name = if !app_id.is_empty() { app_id.clone() } else { name.clone() };

                packages.push(Package {
                    name: pkg_name,
                    version,
                    description,
                    popularity: 0.0,
                    installed: false,
                    maintainer: None,
                    url: None,
                    extra: PackageExtra::default(),
                });
            }
        }

        packages
    }

    fn parse_flatpak_info(&self, output: &str) -> Option<Package> {
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut url = None;

        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "ID" | "Ref" => {
                        if name.is_empty() {
                            name = value.to_string();
                            // Clean up ref format: app/com.example.App/x86_64/stable -> com.example.App
                            if name.starts_with("app/") || name.starts_with("runtime/") {
                                if let Some(app_id) = name.split('/').nth(1) {
                                    name = app_id.to_string();
                                }
                            }
                        }
                    }
                    "Version" => version = value.to_string(),
                    "Subject" | "Description" => {
                        if description.is_empty() {
                            description = value.to_string();
                        }
                    }
                    "Homepage" => {
                        if !value.is_empty() {
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
            description: if description.is_empty() { None } else { Some(description) },
            popularity: 0.0,
            installed: false,
            maintainer: None,
            url,
            extra: PackageExtra::default(),
        })
    }
}

#[async_trait]
impl PackageManager for FlatpakBackend {
    fn name(&self) -> &str {
        "Flatpak"
    }

    fn id(&self) -> &str {
        "flatpak"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let output = Command::new("flatpak")
            .args(["search", query])
            .output()
            .context("Failed to run flatpak search")?;

        if !output.status.success() {
            // Flatpak returns non-zero if no results
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = self.parse_flatpak_search(&stdout);

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
            // Try to get info from remote first
            let output = Command::new("flatpak")
                .args(["remote-info", "--system", "flathub", pkg_name])
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if let Some(mut pkg) = self.parse_flatpak_info(&stdout) {
                        pkg.installed = self.is_installed(&pkg.name)?;
                        results.push(pkg);
                        continue;
                    }
                }
            }

            // Fallback: search for the package
            if let Ok(search_results) = self.search(pkg_name).await {
                if let Some(pkg) = search_results.into_iter().find(|p| p.name == *pkg_name || p.name.contains(pkg_name)) {
                    results.push(pkg);
                }
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        for pkg in packages {
            println!("--> Installing {} with flatpak...", pkg.name);

            let status = Command::new("flatpak")
                .args(["install", "-y", "flathub", &pkg.name])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run flatpak install")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() { None } else { Some("flatpak install failed".to_string()) },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("flatpak")
            .args(["info", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("flatpak")
            .args(["list", "--app", "--columns=application,version"])
            .output()?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<_> = line.split('\t').collect();
                if parts.len() >= 2 {
                    Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                } else if parts.len() == 1 {
                    Some((parts[0].trim().to_string(), String::new()))
                } else {
                    None
                }
            })
            .collect())
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        let output = Command::new("flatpak")
            .args(["remote-ls", "--updates"])
            .output()
            .context("Failed to check for updates")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if let Some(app_id) = parts.first() {
                let app_id = app_id.trim();
                if !app_id.is_empty() {
                    if let Ok(info_results) = self.info(&[app_id]).await {
                        if let Some(pkg) = info_results.into_iter().next() {
                            updates.push(pkg);
                        }
                    }
                }
            }
        }

        Ok(updates)
    }

    async fn update(&self, _packages: &[Package]) -> Result<Vec<InstallResult>> {
        println!("--> Updating all flatpak packages...");

        let status = Command::new("flatpak")
            .args(["update", "-y"])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run flatpak update")?;

        Ok(vec![InstallResult {
            package: "all".to_string(),
            success: status.success(),
            message: if status.success() { None } else { Some("flatpak update failed".to_string()) },
        }])
    }
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

