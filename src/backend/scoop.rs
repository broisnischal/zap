use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{
    bootstrap::{ensure_tool, BootstrapTarget},
    InstallResult, Package, PackageExtra, PackageManager,
};

/// Scoop backend for Windows
pub struct ScoopBackend;

impl ScoopBackend {
    pub fn new() -> Result<Self> {
        ensure_tool(BootstrapTarget::Scoop)?;
        Ok(Self)
    }

    fn run_scoop(args: &[&str]) -> Result<String> {
        let output = Command::new("scoop")
            .args(args)
            .output()
            .with_context(|| format!("Failed to run scoop {:?}", args))?;

        if !output.status.success() {
            anyhow::bail!("scoop command failed (status: {:?})", output.status.code());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn run_scoop_json(args: &[&str]) -> Result<Value> {
        let mut extended = args.to_vec();
        extended.push("--json");
        let stdout = Self::run_scoop(&extended)?;
        let value: Value =
            serde_json::from_str(&stdout).context("Failed to parse scoop JSON output")?;
        Ok(value)
    }

    fn parse_packages(value: &Value) -> Vec<Package> {
        fn parse_entry(entry: &Value) -> Option<Package> {
            let name = entry
                .get("Name")
                .or_else(|| entry.get("name"))
                .and_then(|v| v.as_str())
                .or_else(|| entry.get("app").and_then(|v| v.as_str()))?
                .to_string();

            let version = entry
                .get("Version")
                .or_else(|| entry.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let description = entry
                .get("Description")
                .or_else(|| entry.get("description"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let bucket = entry
                .get("Bucket")
                .or_else(|| entry.get("bucket"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut pkg = Package {
                name,
                version,
                description,
                popularity: 0.0,
                installed: false,
                maintainer: None,
                url: None,
                extra: PackageExtra::default(),
            };

            if let Some(bucket_name) = bucket {
                pkg.extra.categories = vec![bucket_name];
            }

            Some(pkg)
        }

        if let Some(arr) = value.as_array() {
            return arr.iter().filter_map(parse_entry).collect();
        }

        if let Some(arr) = value
            .get("results")
            .or_else(|| value.get("Results"))
            .and_then(|v| v.as_array())
        {
            return arr.iter().filter_map(parse_entry).collect();
        }

        vec![]
    }

    fn parse_list(output: &str) -> Vec<(String, String)> {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[async_trait]
impl PackageManager for ScoopBackend {
    fn name(&self) -> &str {
        "Scoop"
    }

    fn id(&self) -> &str {
        "scoop"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let json = Self::run_scoop_json(&["search", query])?;
        let mut packages = Self::parse_packages(&json);

        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        packages.truncate(50);
        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];
        for pkg in packages {
            let json = Self::run_scoop_json(&["info", pkg])?;
            let mut parsed = Self::parse_packages(&json);
            if let Some(mut pkg_info) = parsed.pop() {
                pkg_info.installed = self.is_installed(&pkg_info.name)?;
                results.push(pkg_info);
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
            println!("--> Installing {} with scoop...", pkg.name);
            let status = Command::new("scoop")
                .args(["install", &pkg.name])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run scoop install")?;

            let success = status.success();
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("scoop install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Self::run_scoop(&["list", package])?;
        Ok(Self::parse_list(&output)
            .into_iter()
            .any(|(name, _)| name == package))
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Self::run_scoop(&["list"])?;
        Ok(Self::parse_list(&output))
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        let output = Self::run_scoop(&["status"])?;
        // `scoop status` lists outdated apps, but parsing is complex; fallback to empty for now
        let mut packages = vec![];
        for line in output.lines() {
            if !line.contains("->") {
                continue;
            }
            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() >= 1 {
                packages.push(Package {
                    name: parts[0].to_string(),
                    version: String::new(),
                    description: None,
                    popularity: 0.0,
                    installed: true,
                    maintainer: None,
                    url: None,
                    extra: PackageExtra::default(),
                });
            }
        }
        Ok(packages)
    }
}
