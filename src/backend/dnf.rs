use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// DNF package manager backend for Fedora/RHEL
pub struct DnfBackend;

impl DnfBackend {
    pub fn new() -> Result<Self> {
        // Verify dnf is available
        if !command_exists("dnf") {
            anyhow::bail!("DNF is not available on this system");
        }
        Ok(Self)
    }

    fn parse_dnf_search(&self, output: &str) -> Vec<Package> {
        let mut packages = vec![];
        let mut current_name = String::new();
        let mut current_summary = String::new();

        for line in output.lines() {
            // Skip headers and separators
            if line.starts_with("===") || line.starts_with("Last metadata") || line.is_empty() {
                continue;
            }

            // Package name lines don't start with whitespace
            if !line.starts_with(' ') && !line.starts_with('\t') {
                // Save previous package
                if !current_name.is_empty() {
                    let (name, version) = self.parse_package_name(&current_name);
                    packages.push(Package {
                        name,
                        version,
                        description: if current_summary.is_empty() { None } else { Some(current_summary.clone()) },
                        popularity: 0.0,
                        installed: false,
                        maintainer: None,
                        url: None,
                        extra: PackageExtra::default(),
                    });
                }

                // Parse new package name (format: "name.arch : summary" or "name-version.arch : summary")
                if let Some((pkg_part, summary)) = line.split_once(':') {
                    current_name = pkg_part.trim().to_string();
                    current_summary = summary.trim().to_string();
                } else {
                    current_name = line.trim().to_string();
                    current_summary.clear();
                }
            } else {
                // Continuation of summary
                current_summary.push(' ');
                current_summary.push_str(line.trim());
            }
        }

        // Don't forget the last package
        if !current_name.is_empty() {
            let (name, version) = self.parse_package_name(&current_name);
            packages.push(Package {
                name,
                version,
                description: if current_summary.is_empty() { None } else { Some(current_summary) },
                popularity: 0.0,
                installed: false,
                maintainer: None,
                url: None,
                extra: PackageExtra::default(),
            });
        }

        packages
    }

    fn parse_package_name(&self, full_name: &str) -> (String, String) {
        // Remove architecture suffix (e.g., .x86_64, .noarch)
        let name = full_name
            .trim_end_matches(".x86_64")
            .trim_end_matches(".i686")
            .trim_end_matches(".noarch")
            .trim_end_matches(".aarch64");

        // Try to extract version if present in name (name-version format)
        // This is a simplification; real version parsing is more complex
        (name.to_string(), "".to_string())
    }

    fn get_package_version(&self, package: &str) -> Result<String> {
        let output = Command::new("dnf")
            .args(["info", package])
            .output()
            .context("Failed to run dnf info")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        
        for line in stdout.lines() {
            if line.starts_with("Version") {
                if let Some((_, version)) = line.split_once(':') {
                    return Ok(version.trim().to_string());
                }
            }
        }

        Ok("".to_string())
    }

    fn parse_dnf_info(&self, output: &str) -> Option<Package> {
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut url = None;
        let mut license = vec![];
        let mut in_description = false;

        for line in output.lines() {
            if in_description {
                if line.starts_with("             :") || (line.starts_with(' ') && !line.contains(':')) {
                    description.push(' ');
                    description.push_str(line.trim().trim_start_matches(':').trim());
                    continue;
                } else {
                    in_description = false;
                }
            }

            if line.starts_with("Name") {
                if let Some((_, v)) = line.split_once(':') {
                    name = v.trim().to_string();
                }
            } else if line.starts_with("Version") {
                if let Some((_, v)) = line.split_once(':') {
                    version = v.trim().to_string();
                }
            } else if line.starts_with("Summary") || line.starts_with("Description") {
                if let Some((_, v)) = line.split_once(':') {
                    description = v.trim().to_string();
                    in_description = true;
                }
            } else if line.starts_with("URL") {
                if let Some((_, v)) = line.split_once(':') {
                    let u = v.trim();
                    if !u.is_empty() && u != "None" {
                        url = Some(format!(":{}", u)); // Re-add the : that was split
                        // Actually fix the URL parsing
                        if let Some((_, full_url)) = line.split_once(": ") {
                            url = Some(full_url.trim().to_string());
                        }
                    }
                }
            } else if line.starts_with("License") {
                if let Some((_, v)) = line.split_once(':') {
                    license = vec![v.trim().to_string()];
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
                license,
                ..Default::default()
            },
        })
    }
}

#[async_trait]
impl PackageManager for DnfBackend {
    fn name(&self) -> &str {
        "DNF (Fedora/RHEL)"
    }

    fn id(&self) -> &str {
        "dnf"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let output = Command::new("dnf")
            .args(["search", query])
            .output()
            .context("Failed to run dnf search")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = self.parse_dnf_search(&stdout);

        // Get versions for packages
        for pkg in &mut packages {
            if let Ok(version) = self.get_package_version(&pkg.name) {
                pkg.version = version;
            }
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        // Limit results
        packages.truncate(30);

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            let output = Command::new("dnf")
                .args(["info", pkg_name])
                .output()
                .context("Failed to run dnf info")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(mut pkg) = self.parse_dnf_info(&stdout) {
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

        println!("--> Installing packages with dnf...");

        let mut cmd = Command::new("sudo");
        cmd.arg("dnf")
            .arg("install")
            .arg("-y")
            .args(&pkg_names)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let status = cmd.status().context("Failed to run dnf install")?;

        let success = status.success();
        for pkg in packages {
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success { None } else { Some("dnf install failed".to_string()) },
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
        println!("--> Checking for updates...");

        let output = Command::new("dnf")
            .args(["check-update"])
            .output()
            .context("Failed to check for updates")?;

        // dnf check-update returns exit code 100 if updates are available
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        for line in stdout.lines() {
            // Skip empty lines and headers
            if line.is_empty() || line.starts_with("Last metadata") || line.contains("packages can be upgraded") {
                continue;
            }

            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0].split('.').next().unwrap_or(parts[0]);
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

