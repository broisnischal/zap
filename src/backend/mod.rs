pub mod aur;
pub mod apt;
pub mod brew;
pub mod dnf;
mod detect;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

pub use detect::{detect_system, System};

/// A generic package representation that works across all package managers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub popularity: f64,  // Normalized 0.0-100.0 for sorting
    pub installed: bool,
    pub maintainer: Option<String>,
    pub url: Option<String>,
    
    // Package manager specific metadata
    #[serde(default)]
    pub extra: PackageExtra,
}

/// Extra metadata that varies by package manager
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageExtra {
    // AUR specific
    pub aur_id: Option<u64>,
    pub aur_votes: Option<u32>,
    pub aur_url_path: Option<String>,
    pub out_of_date: Option<u64>,
    
    // APT specific
    pub apt_section: Option<String>,
    pub apt_priority: Option<String>,
    
    // Brew specific
    pub brew_tap: Option<String>,
    pub brew_cask: Option<bool>,
    
    // Common
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub license: Vec<String>,
}

impl Package {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: None,
            popularity: 0.0,
            installed: false,
            maintainer: None,
            url: None,
            extra: PackageExtra::default(),
        }
    }
    
    pub fn with_description(mut self, desc: Option<String>) -> Self {
        self.description = desc;
        self
    }
    
    pub fn with_popularity(mut self, pop: f64) -> Self {
        self.popularity = pop;
        self
    }
}

/// Result of a package installation
#[derive(Debug)]
pub struct InstallResult {
    pub package: String,
    pub success: bool,
    pub message: Option<String>,
}

/// The trait that all package manager backends must implement
#[async_trait]
pub trait PackageManager: Send + Sync {
    /// Get the name of this package manager (e.g., "AUR", "APT", "Homebrew")
    fn name(&self) -> &str;
    
    /// Get a short identifier (e.g., "aur", "apt", "brew")
    fn id(&self) -> &str;
    
    /// Search for packages matching a query
    async fn search(&self, query: &str) -> Result<Vec<Package>>;
    
    /// Get detailed info about specific packages
    async fn info(&self, packages: &[&str]) -> Result<Vec<Package>>;
    
    /// Install packages
    async fn install(&self, packages: &[Package]) -> Result<Vec<InstallResult>>;
    
    /// Check if a package is installed
    fn is_installed(&self, package: &str) -> Result<bool>;
    
    /// Get list of installed packages (that this manager handles)
    fn list_installed(&self) -> Result<Vec<(String, String)>>; // (name, version)
    
    /// Update/upgrade packages
    async fn update(&self, packages: &[Package]) -> Result<Vec<InstallResult>> {
        // Default implementation: just reinstall
        self.install(packages).await
    }
    
    /// Check which installed packages have updates available
    async fn check_updates(&self) -> Result<Vec<Package>>;
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.name, self.version)
    }
}

