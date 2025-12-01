use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// Docker Hub backend for Docker images
pub struct DockerhubBackend {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct DockerHubSearchResponse {
    results: Vec<DockerHubResult>,
}

#[derive(Debug, Deserialize)]
struct DockerHubResult {
    name: String,
    description: Option<String>,
    star_count: Option<u32>,
    is_official: Option<bool>,
    is_automated: Option<bool>,
}

impl DockerhubBackend {
    pub fn new() -> Result<Self> {
        // Check if docker is available
        if !command_exists("docker") {
            anyhow::bail!("docker is not available on this system. Install Docker to use this backend.");
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    async fn search_dockerhub(&self, query: &str) -> Result<Vec<Package>> {
        // Docker Hub v2 search API
        let url = "https://hub.docker.com/v2/search/repositories";
        let response = self.client
            .get(url)
            .query(&[("q", query), ("page_size", "25")])
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                if let Ok(data) = response.json::<Value>().await {
                    if let Some(results) = data.get("results").and_then(|v| v.as_array()) {
                        let mut packages = Vec::new();
                        for result in results {
                            if let Some(repo_name) = result.get("repo_name").and_then(|v| v.as_str()) {
                                let full_name = if repo_name.starts_with("library/") {
                                    repo_name.strip_prefix("library/").unwrap_or(repo_name).to_string()
                                } else {
                                    repo_name.to_string()
                                };

                                let full_name_clone = full_name.clone();
                                packages.push(Package {
                                    name: full_name,
                                    version: "latest".to_string(),
                                    description: result.get("short_description")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    popularity: result.get("star_count")
                                        .and_then(|v| v.as_u64())
                                        .map(|s| s as f64)
                                        .unwrap_or(0.0),
                                    installed: false,
                                    maintainer: result.get("repo_name")
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| s.split('/').next())
                                        .map(|s| s.to_string()),
                                    url: Some(format!("https://hub.docker.com/r/{}", full_name_clone)),
                                    extra: PackageExtra::default(),
                                });
                            }
                        }
                        // Sort by popularity (star count) descending
                        packages.sort_by(|a, b| b.popularity.partial_cmp(&a.popularity).unwrap_or(std::cmp::Ordering::Equal));
                        return Ok(packages);
                    }
                }
            }
        }

        Ok(vec![])
    }

    async fn fetch_image_info(&self, image_name: &str) -> Result<Option<Package>> {
        // Try to get image info from Docker Hub
        let url = format!("https://hub.docker.com/v2/repositories/{}/", image_name);

        if let Ok(response) = self.client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(data) = response.json::<Value>().await {
                    return Ok(Some(Package {
                        name: data.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(image_name)
                            .to_string(),
                        version: "latest".to_string(),
                        description: data.get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        popularity: data.get("star_count")
                            .and_then(|v| v.as_u64())
                            .map(|s| s as f64)
                            .unwrap_or(0.0),
                        installed: self.is_installed(image_name)?,
                        maintainer: data.get("namespace")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        url: Some(format!("https://hub.docker.com/r/{}", image_name)),
                        extra: PackageExtra::default(),
                    }));
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for DockerhubBackend {
    fn name(&self) -> &str {
        "Docker Hub"
    }

    fn id(&self) -> &str {
        "dockerhub"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let mut packages = self.search_dockerhub(query).await?;
        
        // Limit to top 10 for auto-suggestions
        if packages.len() > 10 {
            packages.truncate(10);
        }
        
        // Check which are installed (pulled)
        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name).unwrap_or(false);
        }

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = Vec::new();

        for image_name in packages {
            if let Ok(Some(mut pkg)) = self.fetch_image_info(image_name).await {
                pkg.installed = self.is_installed(&pkg.name)?;
                results.push(pkg);
            } else {
                // Fallback: create basic package entry
                results.push(Package {
                    name: image_name.to_string(),
                    version: "latest".to_string(),
                    description: Some(format!("Docker image: {}", image_name)),
                    popularity: 0.0,
                    installed: self.is_installed(image_name)?,
                    maintainer: None,
                    url: Some(format!("https://hub.docker.com/r/{}", image_name)),
                    extra: PackageExtra::default(),
                });
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = Vec::new();

        for pkg in packages {
            println!("--> Pulling Docker image: {}...", pkg.name);

            let status = Command::new("docker")
                .args(["pull", &pkg.name])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run docker pull")?;

            results.push(InstallResult {
                package: pkg.name.clone(),
                success: status.success(),
                message: if status.success() {
                    None
                } else {
                    Some("docker pull failed".to_string())
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, image_name: &str) -> Result<bool> {
        // Check if image exists locally
        let output = Command::new("docker")
            .args(["images", "-q", image_name])
            .output()
            .context("Failed to run docker images")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.trim().is_empty())
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let output = Command::new("docker")
            .args(["images", "--format", "{{.Repository}}:{{.Tag}}"])
            .output()
            .context("Failed to run docker images")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    Some((line.to_string(), "latest".to_string()))
                }
            })
            .collect())
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        // Check for updates by comparing local vs remote tags
        Ok(vec![])
    }
}

impl DockerhubBackend {
    /// Run a Docker container from an image
    pub async fn run_container(&self, image_name: &str, container_name: Option<&str>, args: &[String]) -> Result<bool> {
        let mut docker_args = vec!["run".to_string()];
        
        // Add container name if provided
        if let Some(name) = container_name {
            docker_args.push("--name".to_string());
            docker_args.push(name.to_string());
        }
        
        // Add detach flag for background execution
        docker_args.push("-d".to_string());
        
        // Add image name
        docker_args.push(image_name.to_string());
        
        // Add any additional arguments
        docker_args.extend_from_slice(args);

        let status = Command::new("docker")
            .args(&docker_args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run docker run")?;

        Ok(status.success())
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

