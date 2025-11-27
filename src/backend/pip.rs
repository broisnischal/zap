use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::{Command, Stdio};

use super::{
    bootstrap::{ensure_tool, BootstrapTarget},
    InstallResult, Package, PackageExtra, PackageManager,
};

/// pip package manager backend for Python packages
pub struct PipBackend {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct PyPISearchResult {
    info: PyPIInfo,
    releases: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PyPIInfo {
    name: String,
    version: String,
    summary: Option<String>,
    author: Option<String>,
    home_page: Option<String>,
    project_url: Option<String>,
    license: Option<String>,
}

impl PipBackend {
    pub fn new() -> Result<Self> {
        ensure_tool(BootstrapTarget::Python)?;

        if !command_exists("pip") && !command_exists("pip3") {
            install_pip_with_python()
                .context("pip is not available and installation via python failed")?;
        }

        if !command_exists("pip") && !command_exists("pip3") {
            anyhow::bail!("pip is not available on this system");
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    fn get_pip_cmd() -> &'static str {
        if command_exists("pip3") {
            "pip3"
        } else {
            "pip"
        }
    }

    async fn search_pypi(&self, query: &str) -> Result<Vec<Package>> {
        // PyPI's search API was deprecated, so we use the JSON API for specific packages
        // For search, we'll try the package directly or use pip search fallback
        let url = format!("https://pypi.org/pypi/{}/json", query);

        if let Ok(response) = self.client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(result) = response.json::<PyPISearchResult>().await {
                    return Ok(vec![Package {
                        name: result.info.name,
                        version: result.info.version,
                        description: result.info.summary,
                        popularity: 0.0,
                        installed: false,
                        maintainer: result.info.author,
                        url: result.info.home_page.or(result.info.project_url),
                        extra: PackageExtra {
                            license: result.info.license.map(|l| vec![l]).unwrap_or_default(),
                            ..Default::default()
                        },
                    }]);
                }
            }
        }

        // Fallback: search using pip index
        // Note: pip search is disabled on pypi.org, but we can try partial matches
        Ok(vec![])
    }

    fn parse_pip_list(&self, output: &str) -> Vec<(String, String)> {
        let mut packages = vec![];

        for line in output.lines() {
            // Skip header
            if line.starts_with("Package") || line.starts_with("-") || line.is_empty() {
                continue;
            }

            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                packages.push((parts[0].to_string(), parts[1].to_string()));
            }
        }

        packages
    }
}

#[async_trait]
impl PackageManager for PipBackend {
    fn name(&self) -> &str {
        "pip (Python)"
    }

    fn id(&self) -> &str {
        "pip"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let mut packages = self.search_pypi(query).await?;

        // Check which are installed
        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            let url = format!("https://pypi.org/pypi/{}/json", pkg_name);

            if let Ok(response) = self.client.get(&url).send().await {
                if response.status().is_success() {
                    if let Ok(result) = response.json::<PyPISearchResult>().await {
                        let mut pkg = Package {
                            name: result.info.name,
                            version: result.info.version,
                            description: result.info.summary,
                            popularity: 0.0,
                            installed: false,
                            maintainer: result.info.author,
                            url: result.info.home_page.or(result.info.project_url),
                            extra: PackageExtra {
                                license: result.info.license.map(|l| vec![l]).unwrap_or_default(),
                                ..Default::default()
                            },
                        };
                        pkg.installed = self.is_installed(&pkg.name)?;
                        results.push(pkg);
                    }
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

        println!("--> Installing packages with pip...");

        let pip_cmd = Self::get_pip_cmd();
        let status = Command::new(pip_cmd)
            .args(["install", "--user"])
            .args(&pkg_names)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run pip install")?;

        let success = status.success();
        for pkg in packages {
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("pip install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let pip_cmd = Self::get_pip_cmd();
        let output = Command::new(pip_cmd)
            .args(["show", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let pip_cmd = Self::get_pip_cmd();
        let output = Command::new(pip_cmd)
            .args(["list", "--format=columns"])
            .output()?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(self.parse_pip_list(&stdout))
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        let pip_cmd = Self::get_pip_cmd();
        let output = Command::new(pip_cmd)
            .args(["list", "--outdated", "--format=columns"])
            .output()
            .context("Failed to check for updates")?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut updates = vec![];

        for line in stdout.lines() {
            if line.starts_with("Package") || line.starts_with("-") || line.is_empty() {
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
        let pip_cmd = Self::get_pip_cmd();

        if packages.is_empty() {
            // Update all outdated packages
            println!("--> Updating all pip packages...");

            let output = Command::new(pip_cmd)
                .args(["list", "--outdated", "--format=freeze"])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let pkgs: Vec<&str> = stdout
                .lines()
                .filter_map(|l| l.split("==").next())
                .collect();

            if !pkgs.is_empty() {
                let status = Command::new(pip_cmd)
                    .args(["install", "--user", "--upgrade"])
                    .args(&pkgs)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .context("Failed to run pip upgrade")?;

                results.push(InstallResult {
                    package: "all".to_string(),
                    success: status.success(),
                    message: if status.success() {
                        None
                    } else {
                        Some("pip upgrade failed".to_string())
                    },
                });
            }
        } else {
            let pkg_names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();

            let status = Command::new(pip_cmd)
                .args(["install", "--user", "--upgrade"])
                .args(&pkg_names)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run pip upgrade")?;

            let success = status.success();
            for pkg in packages {
                results.push(InstallResult {
                    package: pkg.name.clone(),
                    success,
                    message: if success {
                        None
                    } else {
                        Some("pip upgrade failed".to_string())
                    },
                });
            }
        }

        Ok(results)
    }
}

fn command_exists(cmd: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        Command::new("where")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

fn detect_python_command() -> Option<&'static str> {
    for cmd in ["python3", "python", "py"] {
        if command_exists(cmd) {
            return Some(cmd);
        }
    }
    None
}

fn install_pip_with_python() -> Result<()> {
    let python_cmd = detect_python_command()
        .ok_or_else(|| anyhow!("Python executable not found to install pip"))?;

    println!("--> Attempting to install pip via `{}`...", python_cmd);

    let status = Command::new(python_cmd)
        .args(["-m", "ensurepip", "--upgrade"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to run python ensurepip")?;

    if !status.success() {
        anyhow::bail!("python ensurepip failed with status {:?}", status.code());
    }

    Ok(())
}
