use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::{Command, Stdio};

use super::{InstallResult, Package, PackageExtra, PackageManager};

#[derive(Debug, Deserialize)]
struct BrewSearchResult {
    formulae: Vec<String>,
    casks: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BrewFormula {
    name: String,
    full_name: String,
    desc: Option<String>,
    homepage: Option<String>,
    versions: BrewVersions,
    license: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BrewVersions {
    stable: Option<String>,
    head: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrewCask {
    token: String,
    name: Vec<String>,
    desc: Option<String>,
    homepage: Option<String>,
    version: String,
}

/// Homebrew package manager backend for macOS
pub struct BrewBackend {
    client: reqwest::Client,
}

impl BrewBackend {
    pub fn new() -> Result<Self> {
        // Verify brew is available
        if !command_exists("brew") {
            anyhow::bail!("Homebrew is not installed. Install it from https://brew.sh");
        }

        let client = reqwest::Client::builder()
            .user_agent("zap/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    async fn search_api(&self, query: &str) -> Result<(Vec<String>, Vec<String>)> {
        // Use Homebrew's API for searching
        let url = format!(
            "https://formulae.brew.sh/api/search.json?q={}",
            urlencoded(query)
        );

        let response: BrewSearchResult = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to search Homebrew")?
            .json()
            .await
            .unwrap_or(BrewSearchResult { formulae: vec![], casks: vec![] });

        Ok((response.formulae, response.casks))
    }

    async fn get_formula_info(&self, name: &str) -> Result<Option<Package>> {
        let url = format!("https://formulae.brew.sh/api/formula/{}.json", name);
        
        let response = self.client.get(&url).send().await;
        
        if let Ok(resp) = response {
            if resp.status().is_success() {
                if let Ok(formula) = resp.json::<BrewFormula>().await {
                    return Ok(Some(Package {
                        name: formula.name,
                        version: formula.versions.stable.unwrap_or_else(|| "latest".to_string()),
                        description: formula.desc,
                        popularity: 0.0,
                        installed: false,
                        maintainer: None,
                        url: formula.homepage,
                        extra: PackageExtra {
                            brew_tap: Some(formula.full_name),
                            brew_cask: Some(false),
                            depends: formula.dependencies,
                            license: formula.license.map(|l| vec![l]).unwrap_or_default(),
                            ..Default::default()
                        },
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn get_cask_info(&self, name: &str) -> Result<Option<Package>> {
        let url = format!("https://formulae.brew.sh/api/cask/{}.json", name);
        
        let response = self.client.get(&url).send().await;
        
        if let Ok(resp) = response {
            if resp.status().is_success() {
                if let Ok(cask) = resp.json::<BrewCask>().await {
                    return Ok(Some(Package {
                        name: cask.token.clone(),
                        version: cask.version,
                        description: cask.desc,
                        popularity: 0.0,
                        installed: false,
                        maintainer: None,
                        url: cask.homepage,
                        extra: PackageExtra {
                            brew_tap: Some(format!("cask/{}", cask.token)),
                            brew_cask: Some(true),
                            ..Default::default()
                        },
                    }));
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for BrewBackend {
    fn name(&self) -> &str {
        "Homebrew"
    }

    fn id(&self) -> &str {
        "brew"
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        if query.len() < 2 {
            return Ok(vec![]);
        }

        let (formulae, casks) = self.search_api(query).await?;
        let mut packages = vec![];

        // Get info for formulae (limit to first 15)
        for name in formulae.iter().take(15) {
            if let Ok(Some(pkg)) = self.get_formula_info(name).await {
                packages.push(pkg);
            }
        }

        // Get info for casks (limit to first 15)
        for name in casks.iter().take(15) {
            if let Ok(Some(pkg)) = self.get_cask_info(name).await {
                packages.push(pkg);
            }
        }

        // Mark installed packages
        for pkg in &mut packages {
            pkg.installed = self.is_installed(&pkg.name)?;
        }

        Ok(packages)
    }

    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>> {
        let mut results = vec![];

        for name in packages {
            // Try as formula first
            if let Ok(Some(pkg)) = self.get_formula_info(name).await {
                results.push(pkg);
                continue;
            }

            // Try as cask
            if let Ok(Some(pkg)) = self.get_cask_info(name).await {
                results.push(pkg);
            }
        }

        // Mark installed packages
        for pkg in &mut results {
            pkg.installed = self.is_installed(&pkg.name)?;
        }

        Ok(results)
    }

    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        let mut results = vec![];

        for package in packages {
            let is_cask = package.extra.brew_cask.unwrap_or(false);
            
            println!("--> Installing {}{}...", 
                package.name, 
                if is_cask { " (cask)" } else { "" }
            );

            let mut cmd = Command::new("brew");
            cmd.arg("install");
            
            if is_cask {
                cmd.arg("--cask");
            }
            
            cmd.arg(&package.name)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());

            let status = cmd.status().context("Failed to run brew install")?;

            results.push(InstallResult {
                package: package.name.clone(),
                success: status.success(),
                message: if status.success() { 
                    None 
                } else { 
                    Some("brew install failed".to_string()) 
                },
            });
        }

        Ok(results)
    }

    fn is_installed(&self, package: &str) -> Result<bool> {
        // Check formulae
        let formula_check = Command::new("brew")
            .args(["list", "--formula", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if formula_check.map(|s| s.success()).unwrap_or(false) {
            return Ok(true);
        }

        // Check casks
        let cask_check = Command::new("brew")
            .args(["list", "--cask", package])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        Ok(cask_check.map(|s| s.success()).unwrap_or(false))
    }

    fn list_installed(&self) -> Result<Vec<(String, String)>> {
        let mut installed = vec![];

        // List formulae
        let output = Command::new("brew")
            .args(["list", "--formula", "--versions"])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    installed.push((parts[0].to_string(), parts[1].to_string()));
                }
            }
        }

        // List casks
        let output = Command::new("brew")
            .args(["list", "--cask", "--versions"])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    installed.push((parts[0].to_string(), parts[1].to_string()));
                }
            }
        }

        Ok(installed)
    }

    async fn check_updates(&self) -> Result<Vec<Package>> {
        println!("--> Checking for updates...");
        
        let output = Command::new("brew")
            .args(["outdated", "--json"])
            .output()
            .context("Failed to check for updates")?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse outdated packages
        #[derive(Deserialize)]
        struct OutdatedFormula {
            name: String,
            current_version: String,
        }
        
        let outdated: Vec<OutdatedFormula> = serde_json::from_str(&stdout).unwrap_or_default();
        
        let mut updates = vec![];
        for pkg in outdated {
            if let Ok(Some(mut info)) = self.get_formula_info(&pkg.name).await {
                info.installed = true;
                updates.push(info);
            }
        }

        Ok(updates)
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

