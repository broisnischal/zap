use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::{Command, Stdio};

use super::sudo;
use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Zypper package manager backend for openSUSE
pub struct ZypperBackend;

impl ZypperBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("zypper") {
            anyhow::bail!("zypper is not available on this system");
        }
        Ok(Self)
    }

    fn parse_zypper_search(&self, output: &str) -> Vec<Package> {
        let mut packages = vec![];

        for line in output.lines() {
            // Skip header lines and separators
            if line.starts_with("S ")
                || line.starts_with("--")
                || line.is_empty()
                || line.starts_with("Loading")
            {
                continue;
            }

            // Format: "S | Name | Summary | Type"
            // Or: "i | package-name | Description here | package"
            let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if parts.len() >= 3 {
                let status = parts[0];
                let name = parts[1].to_string();
                let description = parts[2].to_string();
                let installed = status.contains('i');

                if !name.is_empty() && !name.starts_with("S ") {
                    let version = self.get_package_version(&name).unwrap_or_default();
                    packages.push(Package {
                        name,
                        version,
                        description: if description.is_empty() {
                            None
                        } else {
                            Some(description)
                        },
                        popularity: 0.0,
                        installed,
                        maintainer: None,
                        url: None,
                        extra: PackageExtra::default(),
                    });
                }
            }
        }

        packages
    }

    fn get_package_version(&self, package: &str) -> Result<String> {
        let output = Command::new("zypper")
            .args(["info", package])
            .output()
            .context("Failed to run zypper info")?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if line.starts_with("Version") {
                if let Some((_, version)) = line.split_once(':') {
                    return Ok(version.trim().to_string());
                }
            }
        }

        Ok(String::new())
    }

    fn parse_zypper_info(&self, output: &str) -> Option<Package> {
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut url = None;
        let mut in_description = false;

        for line in output.lines() {
            if in_description {
                if line.starts_with("  ") || line.starts_with("\t") {
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
                    "Name" => name = value.to_string(),
                    "Version" => version = value.to_string(),
                    "Summary" => description = value.to_string(),
                    "Description" => {
                        description = value.to_string();
                        in_description = true;
                    }
                    "URL" => {
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
            description: if description.is_empty() {
                None
            } else {
                Some(description.trim().to_string())
            },
            popularity: 0.0,
            installed: false,
            maintainer: None,
            url,
            extra: PackageExtra::default(),
        })
    }
}

#[async_trait]
impl PackageManager for ZypperBackend {
    fn name(&self) -> &str {
        "zypper (openSUSE)"
    }

    fn id(&self) -> &str {
        "zypper"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let output = Command::new("zypper")
            .args(["search", query])
            .output()
            .context("Failed to run zypper search")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = self.parse_zypper_search(&stdout);
        packages.truncate(30);

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            let output = Command::new("zypper")
                .args(["info", pkg_name])
                .output()
                .context("Failed to run zypper info")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(mut pkg) = self.parse_zypper_info(&stdout) {
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

        println!("--> Installing packages with zypper...");

        let mut args = vec!["zypper", "install", "-y"];
        args.extend(pkg_names.iter().copied());

        let status = sudo::run_sudo(&args).context("Failed to run zypper install")?;

        let success = status.success();
        for pkg in packages {
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("zypper install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("rpm")
            .args(["-q", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("rpm")
            .args(["-qa", "--queryformat", "%{NAME} %{VERSION}\n"])
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
        println!("--> Refreshing repositories...");
        let _ = sudo::run_sudo_output(&["zypper", "refresh"]);

        let output = Command::new("zypper")
            .args(["list-updates"])
            .output()
            .context("Failed to check for updates")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        for line in stdout.lines() {
            // Skip headers
            if line.starts_with("S ") || line.starts_with("--") || line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if parts.len() >= 2 {
                let name = parts[1];
                if let Ok(info_results) = self.info(&[name]).await {
                    if let Some(pkg) = info_results.into_iter().next() {
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
