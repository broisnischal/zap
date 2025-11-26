use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

const AUR_RPC_URL: &str = "https://aur.archlinux.org/rpc/v5";
const MAX_RESULTS: usize = 30;

#[derive(Debug, Deserialize)]
struct AurApiPackage {
    #[serde(rename = "ID")]
    id: u64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Description")]
    description: Option<String>,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "NumVotes")]
    num_votes: u32,
    #[serde(rename = "Popularity")]
    popularity: f64,
    #[serde(rename = "Maintainer")]
    maintainer: Option<String>,
    #[serde(rename = "URL")]
    url: Option<String>,
    #[serde(rename = "URLPath")]
    url_path: Option<String>,
    #[serde(rename = "OutOfDate")]
    out_of_date: Option<u64>,
    #[serde(rename = "Depends", default)]
    depends: Vec<String>,
    #[serde(rename = "License", default)]
    license: Vec<String>,
}

impl From<AurApiPackage> for Package {
    fn from(aur: AurApiPackage) -> Self {
        Package {
            name: aur.name,
            version: aur.version,
            description: aur.description,
            popularity: aur.popularity,
            installed: false,
            maintainer: aur.maintainer,
            url: aur.url,
            extra: PackageExtra {
                aur_id: Some(aur.id),
                aur_votes: Some(aur.num_votes),
                aur_url_path: aur.url_path,
                out_of_date: aur.out_of_date,
                depends: aur.depends,
                license: aur.license,
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct AurResponse {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    response_type: String,
    #[allow(dead_code)]
    resultcount: u32,
    results: Vec<AurApiPackage>,
    #[serde(default)]
    error: Option<String>,
}

/// AUR (Arch User Repository) package manager backend
pub struct AurBackend {
    client: reqwest::Client,
    build_dir: PathBuf,
}

impl AurBackend {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .gzip(true)
            .build()
            .context("Failed to create HTTP client")?;

        let build_dir = get_build_dir()?;
        std::fs::create_dir_all(&build_dir)?;

        Ok(Self { client, build_dir })
    }

    /// Download PKGBUILD snapshot for a package
    pub async fn download_snapshot(&self, package: &Package) -> Result<Vec<u8>> {
        let url_path = package.extra.aur_url_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Package {} has no AUR URL path", package.name))?;
        
        let url = format!("https://aur.archlinux.org{}", url_path);
        
        let bytes = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to download snapshot")?
            .bytes()
            .await
            .context("Failed to read snapshot bytes")?;

        Ok(bytes.to_vec())
    }

    fn extract_snapshot(&self, pkg_name: &str, data: &[u8]) -> Result<PathBuf> {
        let pkg_dir = self.build_dir.join(pkg_name);
        
        if pkg_dir.exists() {
            std::fs::remove_dir_all(&pkg_dir)?;
        }

        let tar_gz = flate2::read::GzDecoder::new(data.as_ref());
        let mut archive = tar::Archive::new(tar_gz);
        archive.unpack(&self.build_dir)?;

        Ok(pkg_dir)
    }

    fn build_and_install(&self, pkg_dir: &PathBuf, pkg_name: &str) -> Result<()> {
        let pkgbuild = pkg_dir.join("PKGBUILD");
        if !pkgbuild.exists() {
            anyhow::bail!("PKGBUILD not found in {}", pkg_dir.display());
        }

        let status = Command::new("makepkg")
            .current_dir(pkg_dir)
            .arg("-si")
            .arg("--needed")
            .arg("--noconfirm")
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run makepkg")?;

        if !status.success() {
            anyhow::bail!("makepkg failed for {}", pkg_name);
        }

        Ok(())
    }
}

#[async_trait]
impl PackageManager for AurBackend {
    fn name(&self) -> &str {
        "AUR (Arch User Repository)"
    }

    fn id(&self) -> &str {
        "aur"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        // Try name-only search first
        let url = format!("{}/search/{}?by=name", AUR_RPC_URL, urlencoded(query));
        
        let response: AurResponse = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send search request")?
            .json()
            .await
            .context("Failed to parse search response")?;

        if response.error.is_some() || response.results.is_empty() {
            // Fall back to name-desc search
            let url = format!("{}/search/{}?by=name-desc", AUR_RPC_URL, urlencoded(query));
            
            let response: AurResponse = self.client
                .get(&url)
                .send()
                .await
                .context("Failed to send search request")?
                .json()
                .await
                .context("Failed to parse search response")?;

            if let Some(error) = response.error {
                if error.contains("Too many") {
                    return Ok(vec![]);
                }
                anyhow::bail!("AUR API error: {}", error);
            }

            let mut results: Vec<Package> = response.results.into_iter().map(Package::from).collect();
            results.sort_by(|a, b| b.popularity.partial_cmp(&a.popularity).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(MAX_RESULTS);
            return Ok(results);
        }

        let mut results: Vec<Package> = response.results.into_iter().map(Package::from).collect();
        results.sort_by(|a, b| b.popularity.partial_cmp(&a.popularity).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(MAX_RESULTS);
        Ok(results)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        if packages.is_empty() {
            return Ok(vec![]);
        }

        let args: String = packages
            .iter()
            .map(|p| format!("arg[]={}", urlencoded(p)))
            .collect::<Vec<_>>()
            .join("&");

        let url = format!("{}/info?{}", AUR_RPC_URL, args);

        let response: AurResponse = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send info request")?
            .json()
            .await
            .context("Failed to parse info response")?;

        if let Some(error) = response.error {
            anyhow::bail!("AUR API error: {}", error);
        }

        Ok(response.results.into_iter().map(Package::from).collect())
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        for package in packages {
            let result = async {
                if self.is_installed(&package.name)? {
                    return Ok(());
                }

                println!("--> Downloading {}...", package.name);
                let snapshot = self.download_snapshot(package).await?;

                println!("--> Extracting {}...", package.name);
                let pkg_dir = self.extract_snapshot(&package.name, &snapshot)?;

                println!("--> Building and installing {}...", package.name);
                self.build_and_install(&pkg_dir, &package.name)?;

                println!("--> {} installed successfully!", package.name);
                Ok(())
            }.await;

            results.push(InstallResult {
                package: package.name.clone(),
                success: result.is_ok(),
                message: result.err().map(|e: anyhow::Error| e.to_string()),
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let output = Command::new("pacman")
            .args(["-Q", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        Ok(output.success())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("pacman")
            .args(["-Qm"])  // Foreign packages (AUR)
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
        let installed = self.list_installed()?;
        if installed.is_empty() {
            return Ok(vec![]);
        }

        let names: Vec<&str> = installed.iter().map(|(n, _)| n.as_str()).collect();
        let aur_packages = self.info(&names).await?;

        let mut updates = vec![];
        for pkg in aur_packages {
            if let Some((_, installed_ver)) = installed.iter().find(|(n, _)| *n == pkg.name) {
                if installed_ver != &pkg.version {
                    updates.push(pkg);
                }
            }
        }

        Ok(updates)
    }
}

fn get_build_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "zap", "zap")
        .ok_or_else(|| anyhow::anyhow!("Could not determine build directory"))?;
    Ok(dirs.cache_dir().join("builds"))
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

