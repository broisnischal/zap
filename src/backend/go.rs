use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Go package manager backend (go install)
pub struct GoBackend {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct PkgGoDevResponse {
    results: Vec<PkgGoDevResult>,
}

#[derive(Debug, Deserialize)]
struct PkgGoDevResult {
    #[serde(rename = "Path")]
    path: String,
    #[serde(rename = "Version")]
    version: Option<String>,
    #[serde(rename = "Synopsis")]
    synopsis: Option<String>,
}

impl GoBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("go") {
            anyhow::bail!("go is not available on this system");
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    async fn search_pkg_go_dev(&self, query: &str) -> Result<Vec<Package>> {
        // pkg.go.dev doesn't have a public API, so we'll try to get package info directly
        // For now, we'll assume the query is a module path or search for it

        // Try to get module info directly
        let url = format!("https://proxy.golang.org/{}/@latest", query);

        if let Ok(response) = self.client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(text) = response.text().await {
                    // Parse module info (JSON format)
                    if let Ok(info) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(version) = info.get("Version").and_then(|v| v.as_str()) {
                            return Ok(vec![Package {
                                name: query.to_string(),
                                version: version.to_string(),
                                description: None,
                                popularity: 0.0,
                                installed: self.is_installed(query).unwrap_or(false),
                                maintainer: None,
                                url: Some(format!("https://pkg.go.dev/{}", query)),
                                extra: PackageExtra::default(),
                            }]);
                        }
                    }
                }
            }
        }

        // If direct lookup fails, return empty (no public search API)
        Ok(vec![])
    }

    fn find_installed_binaries() -> Vec<(String, String)> {
        let mut binaries = vec![];

        // Check GOBIN first, then GOPATH/bin
        if let Ok(gobin) = std::env::var("GOBIN") {
            if let Ok(entries) = std::fs::read_dir(&gobin) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    binaries.push((name, "installed".to_string()));
                }
            }
        }

        // Check GOPATH/bin
        if let Ok(gopath) = std::env::var("GOPATH") {
            let bin_path = std::path::Path::new(&gopath).join("bin");
            if let Ok(entries) = std::fs::read_dir(&bin_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !binaries.iter().any(|(n, _)| n == &name) {
                        binaries.push((name, "installed".to_string()));
                    }
                }
            }
        }

        // Fallback: check ~/go/bin
        if let Ok(home) = std::env::var("HOME") {
            let bin_path = std::path::Path::new(&home).join("go").join("bin");
            if let Ok(entries) = std::fs::read_dir(&bin_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !binaries.iter().any(|(n, _)| n == &name) {
                        binaries.push((name, "installed".to_string()));
                    }
                }
            }
        }

        binaries
    }
}

#[async_trait]
impl PackageManager for GoBackend {
    fn name(&self) -> &str {
        "Go (go install)"
    }

    fn id(&self) -> &str {
        "go"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        // Try to find the package directly
        self.search_pkg_go_dev(query).await
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_path in packages {
            // Try to get module info from Go proxy
            let url = format!("https://proxy.golang.org/{}/@latest", pkg_path);

            if let Ok(response) = self.client.get(&url).send().await {
                if response.status().is_success() {
                    if let Ok(text) = response.text().await {
                        if let Ok(info) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(version) = info.get("Version").and_then(|v| v.as_str()) {
                                let pkg = Package {
                                    name: pkg_path.to_string(),
                                    version: version.to_string(),
                                    description: None,
                                    popularity: 0.0,
                                    installed: self.is_installed(pkg_path)?,
                                    maintainer: None,
                                    url: Some(format!("https://pkg.go.dev/{}", pkg_path)),
                                    extra: PackageExtra::default(),
                                };
                                results.push(pkg);
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        for pkg in packages {
            println!("--> Installing {} with go install...", pkg.name);

            // go install requires @version suffix for modules
            let pkg_spec = if pkg.name.contains('@') {
                pkg.name.clone()
            } else if !pkg.version.is_empty() {
                format!("{}@{}", pkg.name, pkg.version)
            } else {
                format!("{}@latest", pkg.name)
            };

            let status = Command::new("go")
                .args(["install", &pkg_spec])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run go install")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() {
                    None
                } else {
                    Some("go install failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        // Extract binary name from package path
        // e.g., github.com/user/tool -> tool
        let binary_name = package.rsplit('/').next().unwrap_or(package);

        // Check if binary exists in GOBIN or GOPATH/bin
        let binaries = Self::find_installed_binaries();
        Ok(binaries.iter().any(|(name, _)| name == binary_name))
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        Ok(Self::find_installed_binaries())
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        // Go doesn't have a built-in way to check for updates
        // Would need to track installed versions separately
        Ok(vec![])
    }

    async fn update(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        // For Go, updating is just reinstalling with @latest
        let mut results = vec![];

        for pkg in packages {
            println!("--> Updating {} with go install...", pkg.name);

            let pkg_spec = format!("{}@latest", pkg.name.split('@').next().unwrap_or(&pkg.name));

            let status = Command::new("go")
                .args(["install", &pkg_spec])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run go install")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() {
                    None
                } else {
                    Some("go install failed".to_string())
                },
            });
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
