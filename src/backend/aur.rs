use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::sudo;
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
        let url_path = package
            .extra
            .aur_url_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Package {} has no AUR URL path", package.name))?;

        let url = format!("https://aur.archlinux.org{}", url_path);

        let bytes = self
            .client
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

    /// Parse PKGBUILD to extract all dependencies
    fn parse_pkgbuild_dependencies(&self, pkg_dir: &PathBuf) -> Result<Vec<String>> {
        let pkgbuild = pkg_dir.join("PKGBUILD");
        if !pkgbuild.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&pkgbuild).context("Failed to read PKGBUILD")?;

        let mut deps = Vec::new();
        let mut in_array = false;
        let mut current_deps = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Check for dependency arrays: depends, makedepends, checkdepends
            if line.starts_with("depends=")
                || line.starts_with("makedepends=")
                || line.starts_with("checkdepends=")
            {
                // Check if it's an array assignment (bash array syntax)
                if line.contains("(") {
                    in_array = true;
                    current_deps.clear();

                    // Extract dependencies from the line
                    if let Some(start) = line.find('(') {
                        let rest = &line[start + 1..];

                        // Check if array closes on same line
                        if let Some(end) = rest.rfind(')') {
                            let deps_str = &rest[..end];
                            // Parse quoted strings
                            let mut current = String::new();
                            let mut in_quotes = false;
                            let mut quote_char = '\0';

                            for ch in deps_str.chars() {
                                match ch {
                                    '\'' | '"' if !in_quotes => {
                                        in_quotes = true;
                                        quote_char = ch;
                                    }
                                    '\'' | '"' if in_quotes && ch == quote_char => {
                                        in_quotes = false;
                                        quote_char = '\0';
                                        if !current.is_empty() {
                                            current_deps.push(current.clone());
                                            current.clear();
                                        }
                                    }
                                    _ if in_quotes => {
                                        current.push(ch);
                                    }
                                    _ if ch.is_whitespace() && !in_quotes => {
                                        if !current.is_empty() {
                                            current_deps.push(current.clone());
                                            current.clear();
                                        }
                                    }
                                    _ if !in_quotes => {
                                        current.push(ch);
                                    }
                                    _ => {}
                                }
                            }

                            if !current.is_empty() {
                                current_deps.push(current);
                            }

                            in_array = false;
                            deps.extend(current_deps.drain(..));
                        } else {
                            // Multi-line array, continue reading
                            let deps_str = rest;
                            let mut current = String::new();
                            let mut in_quotes = false;
                            let mut quote_char = '\0';

                            for ch in deps_str.chars() {
                                match ch {
                                    '\'' | '"' if !in_quotes => {
                                        in_quotes = true;
                                        quote_char = ch;
                                    }
                                    '\'' | '"' if in_quotes && ch == quote_char => {
                                        in_quotes = false;
                                        quote_char = '\0';
                                        if !current.is_empty() {
                                            current_deps.push(current.clone());
                                            current.clear();
                                        }
                                    }
                                    _ if in_quotes => {
                                        current.push(ch);
                                    }
                                    _ if ch.is_whitespace() && !in_quotes => {
                                        if !current.is_empty() {
                                            current_deps.push(current.clone());
                                            current.clear();
                                        }
                                    }
                                    _ if !in_quotes => {
                                        current.push(ch);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                } else {
                    // Single dependency (not an array)
                    if let Some(dep) = line.split('=').nth(1) {
                        let dep = dep.trim().trim_matches('"').trim_matches('\'');
                        if !dep.is_empty() {
                            deps.push(dep.to_string());
                        }
                    }
                }
            } else if in_array {
                // Continue reading multi-line array
                let line = line.trim();
                if line == ")" {
                    in_array = false;
                    deps.extend(current_deps.drain(..));
                } else {
                    // Parse quoted strings from this line
                    let mut current = String::new();
                    let mut in_quotes = false;
                    let mut quote_char = '\0';

                    for ch in line.chars() {
                        match ch {
                            '\'' | '"' if !in_quotes => {
                                in_quotes = true;
                                quote_char = ch;
                            }
                            '\'' | '"' if in_quotes && ch == quote_char => {
                                in_quotes = false;
                                quote_char = '\0';
                                if !current.is_empty() {
                                    current_deps.push(current.clone());
                                    current.clear();
                                }
                            }
                            _ if in_quotes => {
                                current.push(ch);
                            }
                            _ if ch.is_whitespace() && !in_quotes => {
                                if !current.is_empty() {
                                    current_deps.push(current.clone());
                                    current.clear();
                                }
                            }
                            _ if !in_quotes && ch != '(' && ch != ')' => {
                                current.push(ch);
                            }
                            _ => {}
                        }
                    }

                    if !current.is_empty() && in_quotes {
                        // Unclosed quote, might continue on next line
                    } else if !current.is_empty() {
                        current_deps.push(current);
                    }
                }
            }
        }

        // Clean up dependencies (remove version constraints, etc.)
        let cleaned_deps: Vec<String> = deps
            .iter()
            .map(|dep| {
                // Remove version constraints like >=, =, etc.
                // Also handle package names with operators: package>=1.0 -> package
                dep.split_whitespace()
                    .next()
                    .unwrap_or(dep)
                    .split(|c: char| c == '>' || c == '<' || c == '=')
                    .next()
                    .unwrap_or(dep)
                    .to_string()
            })
            .filter(|dep| !dep.is_empty() && dep != "(" && dep != ")")
            .collect();

        Ok(cleaned_deps)
    }

    /// Check if a package exists in the main Arch repositories
    fn is_in_main_repos(&self, package: &str) -> bool {
        // Remove version constraints if present
        let pkg_name = package.split_whitespace().next().unwrap_or(package);

        let output = Command::new("pacman")
            .args(["-Ss", "^", pkg_name, "$"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.lines().any(|line| {
                line.starts_with(&format!("extra/{}", pkg_name))
                    || line.starts_with(&format!("community/{}", pkg_name))
                    || line.starts_with(&format!("core/{}", pkg_name))
                    || line.starts_with(&format!("multilib/{}", pkg_name))
            })
        } else {
            false
        }
    }

    /// Check if a package exists in AUR
    async fn is_in_aur(&self, package: &str) -> bool {
        // Remove version constraints
        let pkg_name = package.split_whitespace().next().unwrap_or(package);

        let url = format!("{}/info/{}", AUR_RPC_URL, urlencoded(pkg_name));
        if let Ok(response) = self.client.get(&url).send().await {
            if let Ok(aur_response) = response.json::<AurResponse>().await {
                return !aur_response.results.is_empty();
            }
        }
        false
    }

    /// Resolve AUR dependencies for packages iteratively (avoids recursion issues)
    async fn resolve_aur_dependencies(&self, package: &Package) -> Result<Vec<Package>> {
        let mut all_deps = Vec::new();
        let mut visited = HashSet::new();
        let mut to_process = vec![package.clone()];

        while let Some(current_pkg) = to_process.pop() {
            let pkg_name = &current_pkg.name;

            // Skip if already processed
            if visited.contains(pkg_name) {
                continue;
            }

            visited.insert(pkg_name.clone());

            // Download and parse PKGBUILD to get dependencies
            let snapshot = match self.download_snapshot(&current_pkg).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "Warning: Could not fetch dependencies for {}: {}",
                        pkg_name, e
                    );
                    continue;
                }
            };

            let pkg_dir = match self.extract_snapshot(pkg_name, &snapshot) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Warning: Could not extract {}: {}", pkg_name, e);
                    continue;
                }
            };

            let deps = match self.parse_pkgbuild_dependencies(&pkg_dir) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!(
                        "Warning: Could not parse dependencies for {}: {}",
                        pkg_name, e
                    );
                    continue;
                }
            };

            // Resolve each dependency
            for dep in deps {
                // Skip if already installed
                if self.is_installed(&dep).unwrap_or(false) {
                    continue;
                }

                // Check if it's in main repos (pacman will handle it)
                if self.is_in_main_repos(&dep) {
                    continue;
                }

                // Check if it's in AUR
                if self.is_in_aur(&dep).await {
                    // Get package info from AUR
                    if let Ok(mut aur_packages) = self.info(&[&dep]).await {
                        if let Some(dep_pkg) = aur_packages.pop() {
                            // Add to list if not already present
                            if !all_deps.iter().any(|p: &Package| p.name == dep_pkg.name) {
                                all_deps.push(dep_pkg.clone());
                                // Add to processing queue
                                to_process.push(dep_pkg);
                            }
                        }
                    }
                }
            }
        }

        Ok(all_deps)
    }

    /// Install a single package with dependency resolution
    async fn install_with_deps(&self, package: &Package) -> Result<()> {
        // Check if already installed
        if self.is_installed(&package.name)? {
            return Ok(());
        }

        // Resolve dependencies
        println!("--> Resolving dependencies for {}...", package.name);
        let deps = match self.resolve_aur_dependencies(package).await {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Warning: Could not fully resolve dependencies: {}", e);
                vec![] // Continue anyway
            }
        };

        // Install dependencies first
        if !deps.is_empty() {
            println!("--> Installing {} dependencies...", deps.len());
            let mut failed_deps = Vec::new();

            for dep in &deps {
                if self.is_installed(&dep.name).unwrap_or(false) {
                    continue;
                }

                println!("  --> Installing dependency: {}...", dep.name);
                match self.install_single_package(dep).await {
                    Ok(()) => {
                        println!("  --> {} installed successfully", dep.name);
                    }
                    Err(e) => {
                        eprintln!("  --> Warning: Failed to install {}: {}", dep.name, e);
                        failed_deps.push((dep.name.clone(), e));
                        // Continue with other dependencies
                    }
                }
            }

            // If some dependencies failed, warn but continue
            if !failed_deps.is_empty() {
                eprintln!(
                    "Warning: {} dependencies failed to install, continuing anyway...",
                    failed_deps.len()
                );
            }
        }

        // Install the main package
        // If it fails, try installing without dependency checks
        match self.install_single_package(package).await {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("Warning: Installation failed: {}", e);
                eprintln!("Attempting fallback installation method...");

                // Fallback: try with makepkg directly, let it handle what it can
                self.fallback_install(package).await
            }
        }
    }

    /// Fallback installation method
    async fn fallback_install(&self, package: &Package) -> Result<()> {
        if self.is_installed(&package.name)? {
            return Ok(());
        }

        println!(
            "--> Attempting fallback installation for {}...",
            package.name
        );
        let snapshot = self.download_snapshot(package).await?;
        let pkg_dir = self.extract_snapshot(&package.name, &snapshot)?;

        // Try building first, then installing manually
        println!("--> Building package...");
        let build_status = Command::new("makepkg")
            .current_dir(&pkg_dir)
            .arg("-s")
            .arg("--needed")
            .arg("--noconfirm")
            .arg("--skipinteg")
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run makepkg")?;

        if !build_status.success() {
            anyhow::bail!("Failed to build {} even with fallback method", package.name);
        }

        // Find and install the built package
        let pkg_file = std::fs::read_dir(&pkg_dir)?
            .filter_map(|e| e.ok())
            .find(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "pkg.tar.zst")
                    .unwrap_or(false)
                    || e.path()
                        .extension()
                        .map(|ext| ext == "pkg.tar.xz")
                        .unwrap_or(false)
            });

        if let Some(pkg_file) = pkg_file {
            println!("--> Installing built package...");
            let pkg_path = pkg_file.path();
            let pkg_path_str = pkg_path.to_string_lossy();
            let status =
                sudo::run_sudo(&["pacman", "-U", "--noconfirm", "--needed", &pkg_path_str])
                    .context("Failed to install package")?;

            if status.success() {
                println!(
                    "--> {} installed successfully (fallback method)",
                    package.name
                );
                Ok(())
            } else {
                anyhow::bail!(
                    "Failed to install {} even with fallback method",
                    package.name
                );
            }
        } else {
            anyhow::bail!("Could not find built package file for {}", package.name);
        }
    }

    /// Install a single package without dependency resolution
    async fn install_single_package(&self, package: &Package) -> Result<()> {
        if self.is_installed(&package.name)? {
            return Ok(());
        }

        println!("--> Downloading {}...", package.name);
        let snapshot = match self.download_snapshot(package).await {
            Ok(s) => s,
            Err(e) => anyhow::bail!("Failed to download {}: {}", package.name, e),
        };

        println!("--> Extracting {}...", package.name);
        let pkg_dir = match self.extract_snapshot(&package.name, &snapshot) {
            Ok(d) => d,
            Err(e) => anyhow::bail!("Failed to extract {}: {}", package.name, e),
        };

        println!("--> Building and installing {}...", package.name);

        // Use makepkg with dependency handling
        // First try to install missing dependencies from repos
        let status = Command::new("makepkg")
            .current_dir(&pkg_dir)
            .arg("-si")
            .arg("--needed")
            .arg("--noconfirm")
            .arg("--skipinteg") // Skip integrity checks for faster builds
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run makepkg")?;

        if !status.success() {
            // If makepkg failed, try building without installing dependencies
            // (we handle AUR deps ourselves)
            println!("--> Retrying build without automatic dependency installation...");
            let status = Command::new("makepkg")
                .current_dir(&pkg_dir)
                .arg("-s")
                .arg("--needed")
                .arg("--noconfirm")
                .arg("--skipinteg")
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run makepkg")?;

            if !status.success() {
                anyhow::bail!("makepkg failed for {}", package.name);
            }

            // Install the built package
            let pkg_file = std::fs::read_dir(&pkg_dir)?
                .filter_map(|e| e.ok())
                .find(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "pkg.tar.zst")
                        .unwrap_or(false)
                        || e.path()
                            .extension()
                            .map(|ext| ext == "pkg.tar.xz")
                            .unwrap_or(false)
                });

            if let Some(pkg_file) = pkg_file {
                let pkg_path = pkg_file.path();
                let pkg_path_str = pkg_path.to_string_lossy();
                let status =
                    sudo::run_sudo(&["pacman", "-U", "--noconfirm", "--needed", &pkg_path_str])
                        .context("Failed to install package")?;

                if !status.success() {
                    anyhow::bail!("Failed to install {}", package.name);
                }
            } else {
                anyhow::bail!("Could not find built package file for {}", package.name);
            }
        }

        println!("--> {} installed successfully!", package.name);
        Ok(())
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

        let response: AurResponse = self
            .client
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

            let response: AurResponse = self
                .client
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

            let mut results: Vec<Package> =
                response.results.into_iter().map(Package::from).collect();
            results.sort_by(|a, b| {
                b.popularity
                    .partial_cmp(&a.popularity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            results.truncate(MAX_RESULTS);
            return Ok(results);
        }

        let mut results: Vec<Package> = response.results.into_iter().map(Package::from).collect();
        results.sort_by(|a, b| {
            b.popularity
                .partial_cmp(&a.popularity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
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

        let response: AurResponse = self
            .client
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
            let result = self.install_with_deps(package).await;

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
            .args(["-Qm"]) // Foreign packages (AUR)
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
