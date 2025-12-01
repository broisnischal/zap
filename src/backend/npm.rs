use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

/// npm package manager backend for JavaScript/TypeScript packages
pub struct NpmBackend {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmSearchObject>,
}

#[derive(Debug, Deserialize)]
struct NpmSearchObject {
    package: NpmSearchPackage,
    score: Option<NpmScore>,
}

#[derive(Debug, Deserialize)]
struct NpmSearchPackage {
    name: String,
    version: String,
    description: Option<String>,
    keywords: Option<Vec<String>>,
    links: Option<NpmLinks>,
    publisher: Option<NpmPublisher>,
}

#[derive(Debug, Deserialize)]
struct NpmLinks {
    npm: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmPublisher {
    username: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmScore {
    detail: Option<NpmScoreDetail>,
}

#[derive(Debug, Deserialize)]
struct NpmScoreDetail {
    popularity: Option<f64>,
}

impl NpmBackend {
    pub fn new() -> Result<Self> {
        if !command_exists(Self::npm_command()) {
            anyhow::bail!(
                "npm is not available on this system. Install Node.js to use this backend."
            );
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    fn npm_command() -> &'static str {
        if cfg!(target_os = "windows") {
            "npm.cmd"
        } else {
            "npm"
        }
    }

    async fn search_registry(&self, query: &str) -> Result<Vec<Package>> {
        let response = self
            .client
            .get("https://registry.npmjs.org/-/v1/search")
            .query(&[("text", query), ("size", "25")])
            .send()
            .await
            .context("Failed to query npm registry")?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let data: NpmSearchResponse = response
            .json()
            .await
            .context("Failed to parse npm search response")?;

        let mut packages: Vec<Package> = data
            .objects
            .into_iter()
            .map(Self::package_from_search)
            .collect();

        let installed = Self::global_dependencies()?
            .into_iter()
            .collect::<HashMap<_, _>>();

        for pkg in &mut packages {
            pkg.installed = installed.contains_key(&pkg.name);
        }

        Ok(packages)
    }

    fn package_from_search(object: NpmSearchObject) -> Package {
        let popularity = object
            .score
            .and_then(|s| s.detail.and_then(|d| d.popularity))
            .unwrap_or(0.0)
            * 100.0;

        let mut extra = PackageExtra::default();
        extra.keywords = object.package.keywords.unwrap_or_default();

        let maintainer = object
            .package
            .publisher
            .and_then(|p| p.username.or(p.email));

        let url = object.package.links.as_ref().and_then(|links| {
            links
                .homepage
                .clone()
                .or(links.npm.clone())
                .or(links.repository.clone())
        });

        Package {
            name: object.package.name,
            version: object.package.version,
            description: object.package.description,
            popularity,
            installed: false,
            maintainer,
            url,
            extra,
        }
    }

    async fn fetch_package(&self, name: &str) -> Result<Option<Package>> {
        let url = format!("https://registry.npmjs.org/{name}");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to query npm package info")?;

        if response.status().as_u16() == 404 {
            return Ok(None);
        }

        let value: Value = response
            .json()
            .await
            .context("Failed to parse npm package info")?;

        let latest = value
            .get("dist-tags")
            .and_then(|tags| tags.get("latest"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if latest.is_empty() {
            return Ok(None);
        }

        let mut extra = PackageExtra::default();
        extra.depends = extract_dependencies(&value, &latest);
        if let Some(keywords) = extract_keywords(&value) {
            extra.keywords = keywords;
        }
        if let Some(license) = value.get("license").and_then(|v| v.as_str()) {
            extra.license = vec![license.to_string()];
        }

        let maintainer = value
            .get("maintainers")
            .and_then(|m| m.as_array())
            .and_then(|arr| arr.first())
            .and_then(|m| m.get("name").or_else(|| m.get("username")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let homepage = value
            .get("homepage")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let repo_url = value
            .get("repository")
            .and_then(|repo| repo.get("url"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let url = homepage
            .or(repo_url)
            .unwrap_or_else(|| format!("https://www.npmjs.com/package/{name}"));

        let description = value
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let installed = self.is_installed(name).unwrap_or(false);

        Ok(Some(Package {
            name: value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(name)
                .to_string(),
            version: latest,
            description,
            popularity: 0.0,
            installed,
            maintainer,
            url: Some(url),
            extra,
        }))
    }

    fn global_dependencies() -> Result<Vec<(String, String)>> {
        let output = Command::new(Self::npm_command())
            .args(["list", "-g", "--depth", "0", "--json"])
            .output()
            .context("Failed to run npm list")?;

        if output.stdout.is_empty() {
            return Ok(vec![]);
        }

        let value: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);

        let mut packages = Vec::new();
        if let Some(deps) = value.get("dependencies").and_then(|v| v.as_object()) {
            for (name, info) in deps {
                if let Some(version) = info.get("version").and_then(|v| v.as_str()) {
                    packages.push((name.clone(), version.to_string()));
                }
            }
        }

        Ok(packages)
    }

    fn run_npm(args: &[&str]) -> Result<std::process::ExitStatus> {
        Command::new(Self::npm_command())
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run npm command")
    }
}

#[async_trait]
impl PackageManager for NpmBackend {
    fn name(&self) -> &str {
        "npm (Node.js)"
    }

    fn id(&self) -> &str {
        "npm"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let packages = self.search_registry(query).await?;
        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = Vec::new();

        for pkg in packages {
            if let Some(info) = self.fetch_package(pkg).await? {
                results.push(info);
            }
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let pkg_names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        if pkg_names.is_empty() {
            return Ok(vec![]);
        }

        // Check if we're in a project directory (has package.json)
        let is_project = std::path::Path::new("package.json").exists();
        
        if is_project {
            println!("--> Installing packages locally (project dependencies)...");
        } else {
            println!("--> Installing packages locally (creating package.json)...");
            // Initialize package.json if it doesn't exist
            let _ = Self::run_npm(&["init", "-y"]);
        }

        // Install locally (without -g flag)
        let mut args = vec!["install", "--save"];
        args.extend(pkg_names.iter().copied());

        let status = Self::run_npm(&args)?;
        let success = status.success();

        Ok(packages
            .iter()
            .map(|pkg| InstallResult {
                package: pkg.name.clone(),
                success,
                message: if success {
                    None
                } else {
                    Some("npm install failed".to_string())
                },
            })
            .collect())
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        let installed = Self::global_dependencies()?;
        Ok(installed.iter().any(|(name, _)| name == package))
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        Self::global_dependencies()
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        let output = Command::new(Self::npm_command())
            .args(["outdated", "-g", "--json"])
            .output()
            .context("Failed to run npm outdated")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() || trimmed == "{}" || trimmed == "null" {
            return Ok(vec![]);
        }

        let value: Value = serde_json::from_str(trimmed).unwrap_or(Value::Null);
        let mut updates = Vec::new();

        if let Some(map) = value.as_object() {
            for name in map.keys() {
                if let Some(mut pkg) = self.fetch_package(name).await? {
                    pkg.extra.out_of_date = Some(1);
                    updates.push(pkg);
                }
            }
        }

        Ok(updates)
    }

    async fn update(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = Vec::new();

        if packages.is_empty() {
            println!("--> Updating all global npm packages...");
            let status = Self::run_npm(&["update", "-g"])?;
            results.push(InstallResult {
                package: "all".to_string(),
                success: status.success(),
                message: if status.success() {
                    None
                } else {
                    Some("npm update failed".to_string())
                },
            });
        } else {
            let pkg_names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
            let mut args = vec!["update", "-g"];
            args.extend(pkg_names.iter().copied());

            let status = Self::run_npm(&args)?;
            let success = status.success();

            for pkg in packages {
                results.push(InstallResult {
                    package: pkg.name.clone(),
                    success,
                    message: if success {
                        None
                    } else {
                        Some("npm update failed".to_string())
                    },
                });
            }
        }

        Ok(results)
    }
}

fn extract_dependencies(value: &Value, version: &str) -> Vec<String> {
    value
        .get("versions")
        .and_then(|versions| versions.get(version))
        .and_then(|ver| ver.get("dependencies"))
        .and_then(|deps| deps.as_object())
        .map(|deps| deps.keys().cloned().collect())
        .unwrap_or_default()
}

fn extract_keywords(value: &Value) -> Option<Vec<String>> {
    if let Some(array) = value.get("keywords").and_then(|v| v.as_array()) {
        let keywords: Vec<String> = array
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if keywords.is_empty() {
            None
        } else {
            Some(keywords)
        }
    } else if let Some(single) = value.get("keywords").and_then(|v| v.as_str()) {
        Some(vec![single.to_string()])
    } else {
        None
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
