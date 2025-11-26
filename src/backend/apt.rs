use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// APT package manager backend for Debian/Ubuntu
pub struct AptBackend;

impl AptBackend {
    pub fn new() -> Result<Self> {
        // Verify apt is available
        if !command_exists("apt") && !command_exists("apt-get") {
            anyhow::bail!("APT is not available on this system");
        }
        Ok(Self)
    }

    fn parse_apt_cache_search(&self, output: &str) -> Vec<Package> {
        let mut packages = vec![];
        
        for line in output.lines() {
            // Format: "package-name - Description here"
            if let Some((name, desc)) = line.split_once(" - ") {
                let name = name.trim();
                
                // Get version info
                let version = self.get_package_version(name).unwrap_or_default();
                let installed = self.is_installed(name).unwrap_or(false);
                
                packages.push(Package {
                    name: name.to_string(),
                    version,
                    description: Some(desc.trim().to_string()),
                    popularity: 0.0, // APT doesn't provide popularity
                    installed,
                    maintainer: None,
                    url: None,
                    extra: PackageExtra {
                        apt_section: None,
                        apt_priority: None,
                        ..Default::default()
                    },
                });
            }
        }

        packages
    }

    fn get_package_version(&self, package: &str) -> Result<String> {
        let output = Command::new("apt-cache")
            .args(["policy", package])
            .output()
            .context("Failed to run apt-cache policy")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse "Candidate: x.y.z" line
        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("Candidate:") {
                return Ok(line.replace("Candidate:", "").trim().to_string());
            }
        }

        Ok("unknown".to_string())
    }

    fn parse_apt_show(&self, output: &str) -> Option<Package> {
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut maintainer = None;
        let mut url = None;
        let mut section = None;
        let mut priority = None;
        let mut depends = vec![];
        let mut in_description = false;

        for line in output.lines() {
            if in_description {
                if line.starts_with(' ') {
                    description.push_str(line.trim());
                    description.push('\n');
                } else {
                    in_description = false;
                }
            }

            if line.starts_with("Package:") {
                name = line.replace("Package:", "").trim().to_string();
            } else if line.starts_with("Version:") {
                version = line.replace("Version:", "").trim().to_string();
            } else if line.starts_with("Description:") {
                description = line.replace("Description:", "").trim().to_string();
                in_description = true;
            } else if line.starts_with("Maintainer:") {
                maintainer = Some(line.replace("Maintainer:", "").trim().to_string());
            } else if line.starts_with("Homepage:") {
                url = Some(line.replace("Homepage:", "").trim().to_string());
            } else if line.starts_with("Section:") {
                section = Some(line.replace("Section:", "").trim().to_string());
            } else if line.starts_with("Priority:") {
                priority = Some(line.replace("Priority:", "").trim().to_string());
            } else if line.starts_with("Depends:") {
                let deps_str = line.replace("Depends:", "");
                depends = deps_str
                    .split(',')
                    .map(|d| d.trim().split_whitespace().next().unwrap_or("").to_string())
                    .filter(|d| !d.is_empty())
                    .collect();
            }
        }

        if name.is_empty() {
            return None;
        }

        Some(Package {
            name,
            version,
            description: if description.is_empty() { None } else { Some(description.trim().to_string()) },
            popularity: 0.0,
            installed: false,
            maintainer,
            url,
            extra: PackageExtra {
                apt_section: section,
                apt_priority: priority,
                depends,
                ..Default::default()
            },
        })
    }
}

#[async_trait]
impl PackageManager for AptBackend {
    fn name(&self) -> &str {
        "APT (Debian/Ubuntu)"
    }

    fn id(&self) -> &str {
        "apt"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let output = Command::new("apt-cache")
            .args(["search", query])
            .output()
            .context("Failed to run apt-cache search")?;

        if !output.status.success() {
            anyhow::bail!("apt-cache search failed");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = self.parse_apt_cache_search(&stdout);
        
        // Limit results
        packages.truncate(30);
        
        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            let output = Command::new("apt-cache")
                .args(["show", pkg_name])
                .output()
                .context("Failed to run apt-cache show")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(mut pkg) = self.parse_apt_show(&stdout) {
                    pkg.installed = self.is_installed(&pkg.name)?;
                    results.push(pkg);
                }
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        // Install all packages in one apt command for efficiency
        let pkg_names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        
        if pkg_names.is_empty() {
            return Ok(results);
        }

        println!("--> Installing packages with apt...");
        
        let mut cmd = Command::new("sudo");
        cmd.arg("apt")
            .arg("install")
            .arg("-y")
            .args(&pkg_names)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let status = cmd.status().context("Failed to run apt install")?;

        let success = status.success();
        for pkg in packages {
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success { None } else { Some("apt install failed".to_string()) },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("dpkg")
            .args(["-s", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("dpkg-query")
            .args(["-W", "-f=${Package} ${Version}\n"])
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
        // First update package lists
        println!("--> Updating package lists...");
        let _ = Command::new("sudo")
            .args(["apt", "update"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        // Get upgradable packages
        let output = Command::new("apt")
            .args(["list", "--upgradable"])
            .output()
            .context("Failed to check for updates")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        for line in stdout.lines().skip(1) {  // Skip "Listing..." header
            // Format: "package/release version arch [upgradable from: old_version]"
            if let Some(name) = line.split('/').next() {
                if let Some(info) = self.info(&[name]).await?.into_iter().next() {
                    updates.push(info);
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

