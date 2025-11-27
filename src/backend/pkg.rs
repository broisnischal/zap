use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::{Command, Stdio};

use super::sudo;
use super::{InstallResult, Package, PackageExtra, PackageManager};

/// pkg package manager backend for FreeBSD
pub struct PkgBackend;

impl PkgBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("pkg") {
            anyhow::bail!("pkg is not available on this system");
        }
        Ok(Self)
    }

    fn parse_pkg_search(&self, output: &str) -> Vec<Package> {
        let mut packages = vec![];

        for line in output.lines() {
            if line.is_empty() {
                continue;
            }

            // Format: "name-version    Description here"
            // or with -o: "origin: name-version Description"
            let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
            if parts.len() >= 1 {
                let name_version = parts[0].trim();
                let description = parts.get(1).map(|s| s.trim().to_string());

                // Split name-version (version is usually after last hyphen with numbers)
                if let Some(idx) = name_version.rfind('-') {
                    let name = &name_version[..idx];
                    let version = &name_version[idx + 1..];

                    packages.push(Package {
                        name: name.to_string(),
                        version: version.to_string(),
                        description,
                        popularity: 0.0,
                        installed: self.is_installed(name).unwrap_or(false),
                        maintainer: None,
                        url: None,
                        extra: PackageExtra::default(),
                    });
                } else {
                    packages.push(Package {
                        name: name_version.to_string(),
                        version: String::new(),
                        description,
                        popularity: 0.0,
                        installed: false,
                        maintainer: None,
                        url: None,
                        extra: PackageExtra::default(),
                    });
                }
            }
        }

        packages
    }

    fn parse_pkg_info(&self, output: &str) -> Option<Package> {
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut url = None;
        let mut maintainer = None;
        let mut license = vec![];

        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "Name" => name = value.to_string(),
                    "Version" => version = value.to_string(),
                    "Comment" => description = value.to_string(),
                    "WWW" => {
                        if !value.is_empty() {
                            url = Some(value.to_string());
                        }
                    }
                    "Maintainer" => {
                        if !value.is_empty() {
                            maintainer = Some(value.to_string());
                        }
                    }
                    "Licenses" => {
                        license = value.split(',').map(|s| s.trim().to_string()).collect();
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
                Some(description)
            },
            popularity: 0.0,
            installed: false,
            maintainer,
            url,
            extra: PackageExtra {
                license,
                ..Default::default()
            },
        })
    }
}

#[async_trait]
impl PackageManager for PkgBackend {
    fn name(&self) -> &str {
        "pkg (FreeBSD)"
    }

    fn id(&self) -> &str {
        "pkg"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let output = Command::new("pkg")
            .args(["search", query])
            .output()
            .context("Failed to run pkg search")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = self.parse_pkg_search(&stdout);
        packages.truncate(30);

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            // Try remote first
            let output = Command::new("pkg")
                .args(["rquery", "%n\n%v\n%c\n%w\n%m\n%L", pkg_name])
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = stdout.lines().collect();
                    if lines.len() >= 4 {
                        let pkg = Package {
                            name: lines[0].to_string(),
                            version: lines[1].to_string(),
                            description: Some(lines[2].to_string()),
                            popularity: 0.0,
                            installed: self.is_installed(lines[0])?,
                            maintainer: lines.get(4).map(|s| s.to_string()),
                            url: lines.get(3).map(|s| s.to_string()),
                            extra: PackageExtra {
                                license: lines
                                    .get(5)
                                    .map(|s| vec![s.to_string()])
                                    .unwrap_or_default(),
                                ..Default::default()
                            },
                        };
                        results.push(pkg);
                        continue;
                    }
                }
            }

            // Fallback to search
            let output = Command::new("pkg")
                .args(["search", "-Q", "comment", pkg_name])
                .output()
                .context("Failed to run pkg search")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parsed = self.parse_pkg_search(&stdout);
                if let Some(pkg) = parsed.into_iter().find(|p| p.name == *pkg_name) {
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

        println!("--> Installing packages with pkg...");

        let mut args = vec!["pkg", "install", "-y"];
        args.extend(pkg_names.iter().copied());

        let status = sudo::run_sudo(&args).context("Failed to run pkg install")?;

        let success = status.success();
        for pkg in packages {
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("pkg install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("pkg")
            .args(["info", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("pkg").args(["query", "%n %v"]).output()?;

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
        println!("--> Updating package database...");
        let _ = sudo::run_sudo_output(&["pkg", "update"]);

        let output = Command::new("pkg")
            .args(["upgrade", "-n"])
            .output()
            .context("Failed to check for updates")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        // Parse upgrade output to find packages
        for line in stdout.lines() {
            // Look for lines like: "package: old_ver -> new_ver"
            if line.contains("->") {
                if let Some(name) = line.split(':').next() {
                    let name = name.trim();
                    if let Ok(info_results) = self.info(&[name]).await {
                        if let Some(pkg) = info_results.into_iter().next() {
                            updates.push(pkg);
                        }
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
