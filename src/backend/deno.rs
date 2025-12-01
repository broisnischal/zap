use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Deno package manager backend
pub struct DenoBackend {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct DenoRegistryResponse {
    items: Vec<DenoRegistryItem>,
}

#[derive(Debug, Deserialize)]
struct DenoRegistryItem {
    name: String,
    description: Option<String>,
    star_count: Option<u32>,
    version: Option<String>,
}

impl DenoBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("deno") {
            anyhow::bail!("deno is not available on this system. Install Deno to use this backend.");
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    async fn search_registry(&self, query: &str) -> Result<Vec<Package>> {
        let url = format!("https://api.deno.land/x/{}", query);
        
        // Try to fetch package info directly
        if let Ok(response) = self.client.get(&url).send().await {
            if response.status().is_success() {
                // Package exists, try to get version info
                return Ok(vec![Package {
                    name: query.to_string(),
                    version: "latest".to_string(),
                    description: Some(format!("Deno module: {}", query)),
                    popularity: 0.0,
                    installed: false,
                    maintainer: None,
                    url: Some(format!("https://deno.land/x/{}", query)),
                    extra: PackageExtra::default(),
                }]);
            }
        }

        // Fallback: search via deno.land API
        let search_url = format!("https://api.deno.land/x?query={}", query);
        if let Ok(response) = self.client.get(&search_url).send().await {
            if response.status().is_success() {
                if let Ok(data) = response.json::<Value>().await {
                    if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
                        let mut packages = Vec::new();
                        for item in items {
                            if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                                packages.push(Package {
                                    name: name.to_string(),
                                    version: item.get("version")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("latest")
                                        .to_string(),
                                    description: item.get("description")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    popularity: item.get("star_count")
                                        .and_then(|v| v.as_u64())
                                        .map(|s| s as f64)
                                        .unwrap_or(0.0),
                                    installed: false,
                                    maintainer: None,
                                    url: Some(format!("https://deno.land/x/{}", name)),
                                    extra: PackageExtra::default(),
                                });
                            }
                        }
                        return Ok(packages);
                    }
                }
            }
        }

        Ok(vec![])
    }
}

#[async_trait]
impl PackageManager for DenoBackend {
    fn name(&self) -> &str {
        "Deno"
    }

    fn id(&self) -> &str {
        "deno"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        self.search_registry(query).await
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = Vec::new();

        for pkg in packages {
            if let Ok(mut packages) = self.search_registry(pkg).await {
                if let Some(mut pkg) = packages.pop() {
                    pkg.installed = self.is_installed(&pkg.name)?;
                    results.push(pkg);
                }
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = Vec::new();

        for pkg in packages {
            println!("--> Installing {} via deno...", pkg.name);

            // Deno installs via import_map or direct import
            // For now, we'll just cache the module
            let status = Command::new("deno")
                .args(["cache", &format!("https://deno.land/x/{}", pkg.name)])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run deno cache")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() {
                    None
                } else {
                    Some("deno cache failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        // Check if deno has cached this module
        let output = Command::new("deno")
            .args(["info", &format!("https://deno.land/x/{}", package)])
            .output()
            .context("Failed to run deno info")?;

        Ok(output.status.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        // Deno doesn't have a direct way to list cached modules
        // We'd need to check the cache directory
        Ok(vec![])
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        // Deno always fetches latest, so updates are automatic
        Ok(vec![])
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

