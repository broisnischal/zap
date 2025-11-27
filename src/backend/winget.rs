use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{
    bootstrap::{ensure_tool, BootstrapTarget},
    InstallResult, Package, PackageExtra, PackageManager,
};

/// Windows `winget` package manager backend
pub struct WingetBackend;

impl WingetBackend {
    pub fn new() -> Result<Self> {
        ensure_tool(BootstrapTarget::Winget)?;
        Ok(Self)
    }

    fn run_winget(args: &[&str]) -> Result<String> {
        let output = Command::new("winget")
            .args(args)
            .output()
            .with_context(|| format!("Failed to run winget {:?}", args))?;

        if !output.status.success() {
            anyhow::bail!("winget command failed (status: {:?})", output.status.code());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn run_winget_json(args: &[&str]) -> Result<Value> {
        let mut extended_args = args.to_vec();
        extended_args.extend([
            "--accept-source-agreements",
            "--accept-package-agreements",
            "--output",
            "json",
        ]);
        let stdout = Self::run_winget(&extended_args)?;
        let value: Value =
            serde_json::from_str(&stdout).context("Failed to parse winget JSON output")?;
        Ok(value)
    }

    fn parse_packages(value: &Value) -> Vec<Package> {
        let mut packages = vec![];

        fn parse_entry(entry: &Value) -> Option<Package> {
            let id = entry
                .get("Id")
                .or_else(|| entry.get("id"))?
                .as_str()?
                .to_string();
            let version = entry
                .get("Version")
                .or_else(|| entry.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let display_name = entry
                .get("Name")
                .or_else(|| entry.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let summary = entry
                .get("ShortDescription")
                .or_else(|| entry.get("Description"))
                .or_else(|| entry.get("summary"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let publisher = entry
                .get("Publisher")
                .or_else(|| entry.get("publisher"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let description = match (display_name, summary.clone()) {
                (Some(name), Some(desc)) => Some(format!("{} - {}", name, desc)),
                (Some(name), None) => Some(name),
                (None, Some(desc)) => Some(desc),
                (None, None) => None,
            };

            let mut pkg = Package {
                name: id.clone(),
                version,
                description,
                popularity: 0.0,
                installed: false,
                maintainer: publisher,
                url: None,
                extra: PackageExtra::default(),
            };

            if let Some(tags) = entry
                .get("Tags")
                .or_else(|| entry.get("tags"))
                .and_then(|v| v.as_array())
            {
                pkg.extra.keywords = tags
                    .iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect();
            }

            Some(pkg)
        }

        if let Some(arr) = value
            .get("Data")
            .or_else(|| value.get("data"))
            .and_then(|v| v.as_array())
        {
            for entry in arr {
                if let Some(pkg) = parse_entry(entry) {
                    packages.push(pkg);
                }
            }
        }

        if let Some(sources) = value
            .get("Sources")
            .or_else(|| value.get("sources"))
            .and_then(|v| v.as_array())
        {
            for source in sources {
                if let Some(pkgs) = source
                    .get("Packages")
                    .or_else(|| source.get("packages"))
                    .and_then(|v| v.as_array())
                {
                    for entry in pkgs {
                        if let Some(pkg) = parse_entry(entry) {
                            packages.push(pkg);
                        }
                    }
                }
            }
        }

        packages
    }
}

#[async_trait]
impl PackageManager for WingetBackend {
    fn name(&self) -> &str {
        "winget"
    }

    fn id(&self) -> &str {
        "winget"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let json = Self::run_winget_json(&["search", "--query", query])?;
        let mut packages = Self::parse_packages(&json);

        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        packages.truncate(40);
        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];
        for pkg in packages {
            let json = Self::run_winget_json(&["show", "--id", pkg, "--exact"])?;
            let mut parsed = Self::parse_packages(&json);
            if let Some(mut info) = parsed.pop() {
                info.installed = self.is_installed(&info.name)?;
                results.push(info);
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
            println!("--> Installing {} with winget...", pkg.name);
            let status = Command::new("winget")
                .args([
                    "install",
                    "--id",
                    &pkg.name,
                    "--exact",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                ])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run winget install")?;

            let success = status.success();
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("winget install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let json = Self::run_winget_json(&["list", "--id", package, "--exact"])?;
        let packages = Self::parse_packages(&json);
        Ok(!packages.is_empty())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let json = Self::run_winget_json(&["list"])?;
        let packages = Self::parse_packages(&json);
        Ok(packages
            .into_iter()
            .map(|pkg| (pkg.name, pkg.version))
            .collect())
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        let json = Self::run_winget_json(&["upgrade"])?;
        Ok(Self::parse_packages(&json))
    }
}
