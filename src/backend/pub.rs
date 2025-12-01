use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Pub package manager backend for Dart packages
pub struct PubBackend {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct PubSearchResponse {
    packages: Vec<PubPackage>,
}

#[derive(Debug, Deserialize)]
struct PubPackage {
    name: String,
    latest: Option<PubVersion>,
    description: Option<String>,
    popularity: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct PubVersion {
    version: String,
}

impl PubBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("dart") && !command_exists("pub") {
            anyhow::bail!("dart/pub is not available on this system. Install Dart to use this backend.");
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    async fn search_pub_dev(&self, query: &str) -> Result<Vec<Package>> {
        let url = format!("https://pub.dev/api/search?q={}", query);
        
        if let Ok(response) = self.client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(data) = response.json::<PubSearchResponse>().await {
                    return Ok(data.packages.into_iter().map(|p| {
                        let pkg_name = p.name.clone();
                        Package {
                            name: p.name,
                            version: p.latest
                                .map(|v| v.version)
                                .unwrap_or_else(|| "unknown".to_string()),
                            description: p.description,
                            popularity: p.popularity.unwrap_or(0.0) * 100.0,
                            installed: false,
                            maintainer: None,
                            url: Some(format!("https://pub.dev/packages/{}", pkg_name)),
                            extra: PackageExtra::default(),
                        }
                    }).collect());
                }
            }
        }

        Ok(vec![])
    }

    async fn fetch_package(&self, name: &str) -> Result<Option<Package>> {
        let url = format!("https://pub.dev/api/packages/{}", name);
        
        if let Ok(response) = self.client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(data) = response.json::<Value>().await {
                    let version = data.get("latest")
                        .and_then(|v| v.get("version"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    
                    return Ok(Some(Package {
                        name: data.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(name)
                            .to_string(),
                        version,
                        description: data.get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        popularity: 0.0,
                        installed: false,
                        maintainer: None,
                        url: Some(format!("https://pub.dev/packages/{}", name)),
                        extra: PackageExtra::default(),
                    }));
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for PubBackend {
    fn name(&self) -> &str {
        "pub (Dart)"
    }

    fn id(&self) -> &str {
        "pub"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let mut packages = self.search_pub_dev(query).await?;
        
        // Check which are installed
        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = Vec::new();

        for pkg_name in packages {
            if let Ok(Some(mut pkg)) = self.fetch_package(pkg_name).await {
                pkg.installed = self.is_installed(&pkg.name)?;
                results.push(pkg);
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = Vec::new();
        let pkg_names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();

        if pkg_names.is_empty() {
            return Ok(results);
        }

        println!("--> Installing packages with pub...");

        let pub_cmd = if command_exists("dart") { "dart" } else { "pub" };
        let status = Command::new(pub_cmd)
            .args(["pub", "add"])
            .args(&pkg_names)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run pub add")?;

        let success = status.success();
        for pkg in packages {
            results.push(InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("pub add failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        // Check pubspec.yaml or pub cache
        let pub_cmd = if command_exists("dart") { "dart" } else { "pub" };
        let output = Command::new(pub_cmd)
            .args(["pub", "deps"])
            .output()
            .context("Failed to run pub deps")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains(package))
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        // Would need to parse pubspec.yaml or pub cache
        Ok(vec![])
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        // Would need to check pub outdated
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

