use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;
use std::sync::Arc;

use super::{detect_available_package_managers, Package, PackageManager, InstallResult};

/// Detected package type for a given package name
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageType {
    /// System package (pacman, apt, dnf, etc.)
    System,
    /// npm package
    Npm,
    /// pip package
    Pip,
    /// Cargo package
    Cargo,
    /// Go package
    Go,
    /// Unknown - will try all backends
    Unknown,
}

/// Detect the likely package type from a package name
pub fn detect_package_type(name: &str) -> PackageType {
    // Check for explicit npm scoped packages
    if name.starts_with("@") {
        return PackageType::Npm;
    }

    // Check for npm packages with slashes (but not Go packages)
    if name.contains("/") && !name.starts_with("github.com/") && !name.starts_with("golang.org/") && !name.starts_with("gopkg.in/") && !name.starts_with("deno.land/") {
        // Could be npm scoped package without @
        // But also could be other things, so we'll try npm first
        return PackageType::Npm;
    }

    // Check for Go-style packages
    if name.starts_with("github.com/") || name.starts_with("golang.org/") || name.starts_with("gopkg.in/") {
        return PackageType::Go;
    }

    // Check for Deno packages
    if name.starts_with("deno.land/") {
        return PackageType::Npm; // Use npm type for now, will be handled by deno backend
    }

    // For everything else, assume it could be a system package or language package
    // We'll try system packages first, then language packages
    PackageType::Unknown
}

/// Multi-backend manager that can search across all available package managers
pub struct MultiBackend {
    backends: Vec<(String, Arc<dyn PackageManager>)>,
}

impl MultiBackend {
    /// Create a new multi-backend manager with all available package managers
    pub fn new() -> Result<Self> {
        let available = detect_available_package_managers();
        let mut backends = Vec::new();

        println!("--> Detected {} available package managers: {}", 
                 available.len(), 
                 available.join(", "));

        // Try to create each available backend
        for backend_id in available {
            match create_backend_by_id(backend_id) {
                Ok(backend) => {
                    backends.push((backend_id.to_string(), backend));
                }
                Err(e) => {
                    eprintln!("  --> Warning: Failed to initialize {} backend: {}", backend_id, e);
                }
            }
        }

        if backends.is_empty() {
            anyhow::bail!("No package managers could be initialized");
        }

        println!("--> Initialized {} package managers", backends.len());
        Ok(Self { backends })
    }

    /// Get all backends
    pub fn get_backends(&self) -> &[(String, Arc<dyn PackageManager>)] {
        &self.backends
    }

    /// Get a specific backend by ID
    pub fn get_backend(&self, id: &str) -> Option<&Arc<dyn PackageManager>> {
        self.backends.iter().find(|(bid, _)| bid == id).map(|(_, b)| b)
    }

    /// Search across all backends for a package
    pub async fn search_all(&self, query: &str) -> Result<Vec<(String, Vec<Package>)>> {
        let mut results = Vec::new();

        // Search in parallel across all backends
        let mut futures = Vec::new();
        for (id, backend) in &self.backends {
            let id_clone = id.clone();
            let backend_clone = Arc::clone(backend);
            let query_clone = query.to_string();
            futures.push(async move {
                match backend_clone.search(&query_clone).await {
                    Ok(packages) => Some((id_clone, packages)),
                    Err(_) => None,
                }
            });
        }

        // Wait for all searches to complete
        for future in futures {
            if let Some((id, packages)) = future.await {
                if !packages.is_empty() {
                    results.push((id, packages));
                }
            }
        }

        Ok(results)
    }

    /// Get info for a package across all backends
    pub async fn info_all(&self, package_name: &str) -> Result<Vec<(String, Vec<Package>)>> {
        let mut results = Vec::new();

        // Try to get info from all backends
        let mut futures = Vec::new();
        for (id, backend) in &self.backends {
            let id_clone = id.clone();
            let backend_clone = Arc::clone(backend);
            let name_clone = package_name.to_string();
            futures.push(async move {
                match backend_clone.info(&[&name_clone]).await {
                    Ok(packages) if !packages.is_empty() => Some((id_clone, packages)),
                    _ => None,
                }
            });
        }

        // Wait for all info requests to complete
        for future in futures {
            if let Some((id, packages)) = future.await {
                results.push((id, packages));
            }
        }

        Ok(results)
    }

    /// Install packages, automatically detecting which backend to use for each
    pub async fn install_auto(&self, package_names: Vec<String>) -> Result<Vec<InstallResult>> {
        let mut all_results = Vec::new();
        let mut packages_by_backend: HashMap<String, Vec<Package>> = HashMap::new();

        println!("--> Searching across {} available package managers...", self.backends.len());

        // For each package, try to find it in appropriate backends
        for package_name in package_names {
            let pkg_type = detect_package_type(&package_name);
            let mut found = false;

            // Try backends based on detected type
            let backends_to_try: Vec<&str> = match pkg_type {
                PackageType::Npm => vec!["npm", "deno"], // Try npm first, then deno
                PackageType::Pip => vec!["pip"],
                PackageType::Cargo => vec!["cargo"],
                PackageType::Go => vec!["go"],
                PackageType::System => {
                    // For system packages, try all system backends
                    // Order matters: try native package managers first, then AUR/universal
                    // CRITICAL: pacman must come before AUR to avoid installing main repo packages via AUR
                    let priority_order = ["pacman", "apt", "dnf", "zypper", "pkg", "brew", "winget", "scoop", "choco"];
                    let mut system_backends: Vec<&str> = Vec::new();
                    
                    // Add backends in priority order
                    for priority_id in &priority_order {
                        if self.backends.iter().any(|(id, _)| id == *priority_id) {
                            system_backends.push(priority_id);
                        }
                    }
                    
                    // Add any other system backends not in priority list
                    for (id, _) in &self.backends {
                        if matches!(
                            id.as_str(),
                            "pacman" | "apt" | "dnf" | "zypper" | "pkg" | "brew"
                                | "winget" | "scoop" | "choco"
                        ) && !system_backends.contains(&id.as_str()) {
                            system_backends.push(id.as_str());
                        }
                    }
                    
                    // Then add AUR and universal package managers (AUR should be last for system packages)
                    let mut aur_universal: Vec<&str> = self
                        .backends
                        .iter()
                        .filter(|(id, _)| {
                            matches!(id.as_str(), "flatpak" | "snap")
                        })
                        .map(|(id, _)| id.as_str())
                        .collect();
                    
                    // Add AUR last (after flatpak/snap)
                    if self.backends.iter().any(|(id, _)| id == "aur") {
                        aur_universal.push("aur");
                    }
                    
                    system_backends.append(&mut aur_universal);
                    system_backends
                }
                PackageType::Unknown => {
                    // For unknown packages, try system backends first, then language backends
                    // CRITICAL: pacman must come before AUR
                    let priority_order = ["pacman", "apt", "dnf", "zypper", "pkg", "brew", "winget", "scoop", "choco", "flatpak", "snap", "aur"];
                    let mut backends: Vec<&str> = Vec::new();
                    
                    // Add backends in priority order
                    for priority_id in &priority_order {
                        if self.backends.iter().any(|(id, _)| id == *priority_id) {
                            backends.push(priority_id);
                        }
                    }
                    
                    // Add any other system backends not in priority list
                    for (id, _) in &self.backends {
                        if matches!(
                            id.as_str(),
                            "pacman" | "apt" | "dnf" | "zypper" | "pkg" | "brew"
                                | "winget" | "scoop" | "choco" | "aur" | "flatpak" | "snap"
                        ) && !backends.contains(&id.as_str()) {
                            backends.push(id.as_str());
                        }
                    }
                    
                    // Then add language package managers
                    let mut lang_backends: Vec<&str> = self
                        .backends
                        .iter()
                        .filter(|(id, _)| {
                            matches!(id.as_str(), "npm" | "pip" | "cargo" | "go" | "deno" | "pub")
                        })
                        .map(|(id, _)| id.as_str())
                        .collect();
                    
                    backends.append(&mut lang_backends);
                    backends
                }
            };

            // Try each backend in order
            for backend_id in backends_to_try {
                if let Some(backend) = self.get_backend(backend_id) {
                    match backend.info(&[&package_name]).await {
                        Ok(mut packages) if !packages.is_empty() => {
                            // For AUR backend, filter out packages without URL paths (they're not installable via AUR)
                            if backend_id == "aur" {
                                packages.retain(|pkg| {
                                    let has_url_path = pkg.extra.aur_url_path.is_some();
                                    if !has_url_path {
                                        // This package is in AUR database but not installable via AUR
                                        // (likely in main repos), so skip it and try next backend
                                    }
                                    has_url_path
                                });
                                
                                // If all packages were filtered out, continue to next backend
                                if packages.is_empty() {
                                    continue;
                                }
                            }
                            
                            // Found it! Add to the appropriate backend's list
                            println!("  --> Found '{}' in {}", package_name, backend_id);
                            packages_by_backend
                                .entry(backend_id.to_string())
                                .or_insert_with(Vec::new)
                                .append(&mut packages);
                            found = true;
                            break; // Found in this backend, no need to try others
                        }
                        Ok(_) => {
                            // Package not found in this backend, continue silently
                        }
                        Err(e) => {
                            // Error searching, but continue trying other backends
                            // Only show error if it's not a "not found" type error
                            if !e.to_string().contains("not found") && !e.to_string().contains("No package") {
                                eprintln!("  --> Warning: {} error: {}", backend_id, e);
                            }
                        }
                    }
                }
            }

            // If not found in preferred backends, try all remaining backends
            if !found {
                for (backend_id, backend) in &self.backends {
                    // Skip if we already tried this backend
                    let already_tried = match pkg_type {
                        PackageType::Npm => backend_id == "npm",
                        PackageType::Pip => backend_id == "pip",
                        PackageType::Cargo => backend_id == "cargo",
                        PackageType::Go => backend_id == "go",
                        PackageType::System => {
                            matches!(
                                backend_id.as_str(),
                                "pacman" | "aur" | "apt" | "dnf" | "zypper" | "pkg" | "brew"
                                    | "winget" | "scoop" | "choco" | "flatpak" | "snap"
                            )
                        }
                        PackageType::Unknown => false, // Already tried all
                    };

                    if !already_tried {
                        if let Ok(mut packages) = backend.info(&[&package_name]).await {
                            if !packages.is_empty() {
                                packages_by_backend
                                    .entry(backend_id.clone())
                                    .or_insert_with(Vec::new)
                                    .append(&mut packages);
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }

            if !found {
                all_results.push(InstallResult {
                    package: package_name.clone(),
                    success: false,
                    message: Some(format!("Package '{}' not found in any backend", package_name)),
                });
            }
        }

        // Install packages grouped by backend
        for (backend_id, packages) in packages_by_backend {
            if let Some(backend) = self.get_backend(&backend_id) {
                println!();
                println!("--> Installing {} packages via {}:", 
                         packages.len(), 
                         backend.name().cyan().bold());
                for pkg in &packages {
                    println!("  {} {} {}", 
                             "•".green(), 
                             pkg.name.cyan().bold(), 
                             pkg.version.green());
                }
                
                match backend.install(&packages).await {
                    Ok(mut results) => {
                        // Check for failed installations and try fallback
                        let failed_packages: Vec<String> = results
                            .iter()
                            .filter(|r| !r.success)
                            .map(|r| r.package.clone())
                            .collect();
                        
                        // If AUR installation failed for some packages, try pacman as fallback
                        if backend_id == "aur" && !failed_packages.is_empty() {
                            if let Some(pacman_backend) = self.get_backend("pacman") {
                                println!();
                                println!("--> Attempting fallback installation via pacman for failed packages...");
                                for failed_pkg in &failed_packages {
                                    if let Ok(pacman_packages) = pacman_backend.info(&[failed_pkg]).await {
                                        if !pacman_packages.is_empty() {
                                            println!("  --> Found '{}' in pacman, installing...", failed_pkg);
                                            if let Ok(mut fallback_results) = pacman_backend.install(&pacman_packages).await {
                                                // Replace failed result with successful fallback result
                                                results.retain(|r| r.package != *failed_pkg);
                                                all_results.append(&mut fallback_results);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        all_results.append(&mut results);
                    }
                    Err(e) => {
                        // If AUR installation completely failed, try pacman as fallback
                        if backend_id == "aur" {
                            if let Some(pacman_backend) = self.get_backend("pacman") {
                                println!();
                                println!("--> Installation via AUR failed, attempting fallback via pacman...");
                                let mut fallback_success = false;
                                for pkg in &packages {
                                    if let Ok(pacman_packages) = pacman_backend.info(&[&pkg.name]).await {
                                        if !pacman_packages.is_empty() {
                                            println!("  --> Found '{}' in pacman, installing...", pkg.name);
                                            if let Ok(mut fallback_results) = pacman_backend.install(&pacman_packages).await {
                                                all_results.append(&mut fallback_results);
                                                fallback_success = true;
                                                continue;
                                            }
                                        }
                                    }
                                }
                                
                                if !fallback_success {
                                    // Mark all packages as failed
                                    for pkg in packages {
                                        all_results.push(InstallResult {
                                            package: pkg.name,
                                            success: false,
                                            message: Some(format!("Installation via {} failed: {}", backend_id, e)),
                                        });
                                    }
                                }
                            } else {
                                // No pacman fallback available, mark as failed
                                for pkg in packages {
                                    all_results.push(InstallResult {
                                        package: pkg.name,
                                        success: false,
                                        message: Some(format!("Installation via {} failed: {}", backend_id, e)),
                                    });
                                }
                            }
                        } else {
                            // Mark all packages as failed
                            for pkg in packages {
                                all_results.push(InstallResult {
                                    package: pkg.name,
                                    success: false,
                                    message: Some(format!("Installation via {} failed: {}", backend_id, e)),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(all_results)
    }
}

/// Create a backend by its ID string
fn create_backend_by_id(id: &str) -> Result<Arc<dyn PackageManager>> {
    match id {
        "apt" => Ok(Arc::new(super::apt::AptBackend::new()?)),
        "aur" => Ok(Arc::new(super::aur::AurBackend::new()?)),
        "brew" => Ok(Arc::new(super::brew::BrewBackend::new()?)),
        "winget" => Ok(Arc::new(super::winget::WingetBackend::new()?)),
        "scoop" => Ok(Arc::new(super::scoop::ScoopBackend::new()?)),
        "choco" => Ok(Arc::new(super::choco::ChocoBackend::new()?)),
        "dnf" => Ok(Arc::new(super::dnf::DnfBackend::new()?)),
        "pacman" => Ok(Arc::new(super::pacman::PacmanBackend::new()?)),
        "pkg" => Ok(Arc::new(super::pkg::PkgBackend::new()?)),
        "zypper" => Ok(Arc::new(super::zypper::ZypperBackend::new()?)),
        "flatpak" => Ok(Arc::new(super::flatpak::FlatpakBackend::new()?)),
        "snap" => Ok(Arc::new(super::snap::SnapBackend::new()?)),
        "cargo" => Ok(Arc::new(super::cargo::CargoBackend::new()?)),
        "go" => Ok(Arc::new(super::go::GoBackend::new()?)),
        "pip" => Ok(Arc::new(super::pip::PipBackend::new()?)),
        "npm" => Ok(Arc::new(super::npm::NpmBackend::new()?)),
        "deno" => Ok(Arc::new(super::deno::DenoBackend::new()?)),
        "pub" => Ok(Arc::new(super::r#pub::PubBackend::new()?)),
        "dockerhub" => Ok(Arc::new(super::dockerhub::DockerhubBackend::new()?)),
        "zsh" => Ok(Arc::new(super::zsh::ZshBackend::new()?)),
        _ => anyhow::bail!("Unknown backend: {}", id),
    }
}

