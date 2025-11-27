use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use async_trait::async_trait;

use super::{
    bootstrap::{ensure_tool, BootstrapTarget},
    InstallResult, Package, PackageExtra, PackageManager,
};

/// Chocolatey backend for Windows
pub struct ChocoBackend;

impl ChocoBackend {
    pub fn new() -> Result<Self> {
        ensure_tool(BootstrapTarget::Choco)?;
        Ok(Self)
    }

    fn run_choco(args: &[&str]) -> Result<String> {
        let output = Command::new("choco")
            .args(args)
            .output()
            .with_context(|| format!("Failed to run choco {:?}", args))?;

        if !output.status.success() {
            anyhow::bail!("choco command failed (status: {:?})", output.status.code());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn parse_limitoutput(&self, output: &str) -> Vec<Package> {
        output
            .lines()
            .filter_map(|line| {
                if let Some((name, rest)) = line.split_once('|') {
                    let version = rest.trim().to_string();
                    let pkg = Package {
                        name: name.trim().to_string(),
                        version,
                        description: None,
                        popularity: 0.0,
                        installed: false,
                        maintainer: None,
                        url: None,
                        extra: PackageExtra::default(),
                    };
                    Some(pkg)
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_package_info(&self, package: &str) -> Result<Option<Package>> {
        // Use `choco info` for detailed metadata if available, fallback to search
        let output = Command::new("choco")
            .args(["info", package])
            .output()
            .context("Failed to run choco info")?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut title = None;
        let mut version = String::new();
        let mut description = Vec::new();
        let mut url = None;

        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("Title:") {
                title = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("Version:") {
                version = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("Summary:") {
                description.push(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("Description:") {
                description.push(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("Project URL:") {
                let trimmed = rest.trim();
                if !trimmed.is_empty() && trimmed != "n/a" {
                    url = Some(trimmed.to_string());
                }
            }
        }

        if version.is_empty() {
            // fallback to search for version
            if let Some(pkg) = self
                .parse_limitoutput(&Self::run_choco(&[
                    "search",
                    package,
                    "--exact",
                    "--limitoutput",
                ])?) // safe recursion?
                .into_iter()
                .next()
            {
                version = pkg.version;
            }
        }

        let mut combined_description = if description.is_empty() {
            None
        } else {
            Some(description.join(" "))
        };

        if combined_description.is_none() {
            combined_description = title.clone();
        }

        Ok(Some(Package {
            name: package.to_string(),
            version,
            description: combined_description,
            popularity: 0.0,
            installed: false,
            maintainer: None,
            url,
            extra: PackageExtra::default(),
        }))
    }
}

#[async_trait]
impl PackageManager for ChocoBackend {
    fn name(&self) -> &str {
        "Chocolatey"
    }

    fn id(&self) -> &str {
        "choco"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let stdout = Self::run_choco(&["search", query, "--limitoutput"])?;
        let mut packages = self.parse_limitoutput(&stdout);

        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        packages.truncate(50);
        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];
        for pkg in packages {
            if let Some(mut details) = self.get_package_info(pkg)? {
                details.installed = self.is_installed(pkg)?;
                results.push(details);
            }
        }
        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];
        if packages.is_empty() {
            return Ok(results);
        }

        for pkg in packages {
            println!("--> Installing {} with choco...", pkg.name);
            let status = Command::new("choco")
                .args(["install", &pkg.name, "-y"])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run choco install")?;

            let success = status.success();
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("choco install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let stdout =
            Self::run_choco(&["list", package, "--local-only", "--exact", "--limitoutput"])?;
        Ok(!self.parse_limitoutput(&stdout).is_empty())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let stdout = Self::run_choco(&["list", "--local-only", "--limitoutput"])?;
        Ok(self
            .parse_limitoutput(&stdout)
            .into_iter()
            .map(|pkg| (pkg.name, pkg.version))
            .collect())
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        let stdout = Self::run_choco(&["outdated", "--limitoutput"])?;
        Ok(self.parse_limitoutput(&stdout))
    }
}
