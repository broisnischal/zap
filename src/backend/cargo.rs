use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

const CRATES_IO_API: &str = "https://crates.io/api/v1";

/// Cargo package manager backend for Rust crates
pub struct CargoBackend {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct CratesSearchResponse {
    crates: Vec<CrateInfo>,
}

#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    name: String,
    #[serde(default)]
    max_version: String,
    #[serde(default)]
    newest_version: String,
    description: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    downloads: Option<u64>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
}

impl CargoBackend {
    pub fn new() -> Result<Self> {
        if !command_exists("cargo") {
            anyhow::bail!("cargo is not available on this system");
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0 (package manager)")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    fn get_version(info: &CrateInfo) -> String {
        if !info.max_version.is_empty() {
            info.max_version.clone()
        } else if !info.newest_version.is_empty() {
            info.newest_version.clone()
        } else {
            String::new()
        }
    }
}

#[async_trait]
impl PackageManager for CargoBackend {
    fn name(&self) -> &str {
        "Cargo (Rust)"
    }

    fn id(&self) -> &str {
        "cargo"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let url = format!("{}/crates?q={}&per_page=30", CRATES_IO_API, urlencoded(query));

        let response: CratesSearchResponse = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to search crates.io")?
            .json()
            .await
            .context("Failed to parse crates.io response")?;

        let mut packages: Vec<Package> = response.crates.into_iter().map(|c| {
            let downloads = c.downloads.unwrap_or(0);
            // Normalize downloads to a popularity score (0-100)
            let popularity = (downloads as f64).log10() * 10.0;
            let version = Self::get_version(&c);

            Package {
                name: c.name,
                version,
                description: c.description,
                popularity: popularity.min(100.0).max(0.0),
                installed: false,
                maintainer: None,
                url: c.homepage.or(c.repository),
                extra: PackageExtra::default(),
            }
        }).collect();

        // Check which are installed
        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        // Sort by popularity
        packages.sort_by(|a, b| b.popularity.partial_cmp(&a.popularity).unwrap_or(std::cmp::Ordering::Equal));

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for pkg_name in packages {
            let url = format!("{}/crates/{}", CRATES_IO_API, pkg_name);

            if let Ok(response) = self.client.get(&url).send().await {
                if response.status().is_success() {
                    if let Ok(result) = response.json::<CrateResponse>().await {
                        let c = result.krate;
                        let downloads = c.downloads.unwrap_or(0);
                        let popularity = (downloads as f64).log10() * 10.0;
                        let version = Self::get_version(&c);

                        let mut pkg = Package {
                            name: c.name,
                            version,
                            description: c.description,
                            popularity: popularity.min(100.0).max(0.0),
                            installed: false,
                            maintainer: None,
                            url: c.homepage.or(c.repository),
                            extra: PackageExtra::default(),
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

        for pkg in packages {
            println!("--> Installing {} with cargo...", pkg.name);

            let status = Command::new("cargo")
                .args(["install", &pkg.name])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run cargo install")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() { None } else { Some("cargo install failed".to_string()) },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("cargo")
            .args(["install", "--list"])
            .output()?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Format: "crate_name v1.0.0:"
        Ok(stdout.lines().any(|line| {
            line.split_whitespace().next().map(|n| n == package).unwrap_or(false)
        }))
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("cargo")
            .args(["install", "--list"])
            .output()?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = vec![];

        for line in stdout.lines() {
            // Format: "crate_name v1.0.0:" or "    binary_name"
            if !line.starts_with(' ') && line.contains(' ') {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_string();
                    let version = parts[1].trim_start_matches('v').trim_end_matches(':').to_string();
                    packages.push((name, version));
                }
            }
        }

        Ok(packages)
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        let installed = self.list_installed()?;
        let mut updates = vec![];

        for (name, installed_version) in installed {
            if let Ok(info_results) = self.info(&[&name]).await {
                if let Some(pkg) = info_results.into_iter().next() {
                    if pkg.version != installed_version {
                        updates.push(pkg);
                    }
                }
            }
        }

        Ok(updates)
    }

    async fn update(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        if packages.is_empty() {
            // Update all installed crates
            println!("--> Checking for cargo updates...");
            let updates = self.check_updates().await?;
            
            for pkg in updates {
                println!("--> Updating {}...", pkg.name);
                let status = Command::new("cargo")
                    .args(["install", &pkg.name])
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .context("Failed to run cargo install")?;

                results.push(InstallResult {
                    package: pkg.name,
                    success: status.success(),
                    message: if status.success() { None } else { Some("cargo update failed".to_string()) },
                });
            }
        } else {
            for pkg in packages {
                println!("--> Updating {}...", pkg.name);
                let status = Command::new("cargo")
                    .args(["install", &pkg.name])
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .context("Failed to run cargo install")?;

                results.push(InstallResult {
                    package: pkg.name.clone(),
                    success: status.success(),
                    message: if status.success() { None } else { Some("cargo update failed".to_string()) },
                });
            }
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

fn urlencoded(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

