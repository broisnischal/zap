use anyhow::{Context, Result};
use async_trait::async_trait;
use colored::Colorize;
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Zsh plugin manager backend
pub struct ZshBackend {
    client: reqwest::Client,
    plugins_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct GitHubRepo {
    name: String,
    full_name: String,
    description: Option<String>,
    stargazers_count: u32,
    html_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubSearchResponse {
    items: Vec<GitHubRepo>,
}

impl ZshBackend {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        // Default zsh plugins directory
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("Could not determine home directory")?;
        
        let plugins_dir = PathBuf::from(home).join(".zsh").join("plugins");
        
        // Create plugins directory if it doesn't exist
        std::fs::create_dir_all(&plugins_dir)
            .context("Failed to create zsh plugins directory")?;

        Ok(Self { client, plugins_dir })
    }

    async fn search_github(&self, query: &str) -> Result<Vec<Package>> {
        // Search GitHub for zsh plugins
        let url = format!(
            "https://api.github.com/search/repositories?q={}+language:shell+topic:zsh-plugin&sort=stars&order=desc",
            query
        );

        if let Ok(response) = self.client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(data) = response.json::<GitHubSearchResponse>().await {
                    return Ok(data.items.into_iter().map(|repo| Package {
                        name: repo.full_name,
                        version: "latest".to_string(),
                        description: repo.description,
                        popularity: repo.stargazers_count as f64,
                        installed: false,
                        maintainer: Some(repo.name),
                        url: Some(repo.html_url),
                        extra: PackageExtra::default(),
                    }).collect());
                }
            }
        }

        Ok(vec![])
    }

    fn get_plugin_path(&self, plugin_name: &str) -> PathBuf {
        // Extract repo name from "owner/repo" format
        let repo_name = plugin_name.split('/').last().unwrap_or(plugin_name);
        self.plugins_dir.join(repo_name)
    }

    /// Verify if a GitHub repository exists
    pub async fn verify_repo_exists(&self, repo: &str) -> Result<bool> {
        let url = format!("https://api.github.com/repos/{}", repo);
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }

    /// Get package info for a specific repository
    pub async fn get_repo_info(&self, repo: &str) -> Result<Option<Package>> {
        let url = format!("https://api.github.com/repos/{}", repo);
        let response = self.client.get(&url).send().await?;
        
        if response.status().is_success() {
            if let Ok(repo_data) = response.json::<GitHubRepo>().await {
                let installed = self.is_installed(repo).unwrap_or(false);
                return Ok(Some(Package {
                    name: repo_data.full_name,
                    version: "latest".to_string(),
                    description: repo_data.description,
                    popularity: repo_data.stargazers_count as f64,
                    installed,
                    maintainer: Some(repo_data.name),
                    url: Some(repo_data.html_url),
                    extra: PackageExtra::default(),
                }));
            }
        }
        
        Ok(None)
    }
}

#[async_trait]
impl PackageManager for ZshBackend {
    fn name(&self) -> &str {
        "Zsh Plugins"
    }

    fn id(&self) -> &str {
        "zsh"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let mut packages = self.search_github(query).await?;
        
        // Check which are installed
        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = Vec::new();

        for pkg_name in packages {
            // Try to get info directly from GitHub API
            if let Ok(Some(mut pkg)) = self.get_repo_info(pkg_name).await {
                pkg.installed = self.is_installed(&pkg.name)?;
                results.push(pkg);
            }
            // If direct lookup fails, try search as fallback
            else if let Ok(packages) = self.search_github(pkg_name).await {
                if let Some(mut pkg) = packages.into_iter().find(|p| p.name == *pkg_name) {
                    pkg.installed = self.is_installed(&pkg.name)?;
                    results.push(pkg);
                }
            }
            // Only return package if repo exists - don't create dummy entries
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = Vec::new();

        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(packages.len() as u64);
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Installing plugins... ({pos}/{len})")
                .unwrap()
        );

        for pkg in packages {
            print!("  {} Installing {}...\r", "⏳".yellow(), pkg.name);
            std::io::stdout().flush().ok();

            let plugin_path = self.get_plugin_path(&pkg.name);
            
            // Check if already installed
            if plugin_path.exists() {
                results.push(InstallResult {
                    package: pkg.name.clone(),
                    success: true,
                    message: Some("Already installed".to_string()),
                });
                pb.inc(1);
                continue;
            }

            // Verify repository exists before attempting to clone
            if !self.verify_repo_exists(&pkg.name).await.unwrap_or(false) {
                results.push(InstallResult {
                    package: pkg.name.clone(),
                    success: false,
                    message: Some("Repository not found on GitHub".to_string()),
                });
                pb.inc(1);
                continue;
            }

            // Clone the repository (suppress output for cleaner UI)
            let repo_url = format!("https://github.com/{}.git", pkg.name);
            let status = Command::new("git")
                .args(["clone", "--depth", "1", &repo_url])
                .arg(&plugin_path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .context("Failed to run git clone")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() {
                    Some(format!("Installed to {:?}", plugin_path))
                } else {
                    Some("git clone failed".to_string())
                },
            });
            
            pb.inc(1);
        }

        pb.finish_and_clear();
        println!(); // Clear the last status line

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let plugin_path = self.get_plugin_path(package);
        Ok(plugin_path.exists() && plugin_path.is_dir())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let mut plugins = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.plugins_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    plugins.push((name, "installed".to_string()));
                }
            }
        }

        Ok(plugins)
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        // Check for updates by pulling latest from git
        Ok(vec![])
    }
}

