use std::process::Command;

/// Detected system/distro information
#[derive(Debug, Clone, PartialEq)]
pub enum System {
    /// Arch Linux and derivatives (Manjaro, EndeavourOS, etc.)
    Arch,
    /// Debian and derivatives (Ubuntu, Mint, Pop!_OS, etc.)
    Debian,
    /// Ubuntu specifically (for PPA support)
    Ubuntu,
    /// Fedora and derivatives
    Fedora,
    /// macOS
    MacOS,
    /// Unknown system
    Unknown(String),
}

impl System {
    pub fn package_manager_name(&self) -> &str {
        match self {
            System::Arch => "AUR + pacman",
            System::Debian | System::Ubuntu => "APT",
            System::Fedora => "DNF",
            System::MacOS => "Homebrew",
            System::Unknown(_) => "Unknown",
        }
    }
}

/// Detect the current operating system and distribution
pub fn detect_system() -> System {
    // Check for macOS first
    if cfg!(target_os = "macos") {
        return System::MacOS;
    }
    
    // On Linux, check /etc/os-release
    if let Ok(os_release) = std::fs::read_to_string("/etc/os-release") {
        let os_release = os_release.to_lowercase();
        
        // Check ID and ID_LIKE fields
        if os_release.contains("id=arch") || os_release.contains("id_like=arch") 
           || os_release.contains("id_like=\"arch") || os_release.contains("manjaro")
           || os_release.contains("endeavouros") || os_release.contains("garuda") {
            return System::Arch;
        }
        
        if os_release.contains("id=ubuntu") {
            return System::Ubuntu;
        }
        
        if os_release.contains("id=debian") || os_release.contains("id_like=debian")
           || os_release.contains("id_like=\"debian") || os_release.contains("linuxmint")
           || os_release.contains("pop") || os_release.contains("elementary") {
            return System::Debian;
        }
        
        if os_release.contains("id=fedora") || os_release.contains("id_like=fedora")
           || os_release.contains("id_like=\"fedora") || os_release.contains("rhel")
           || os_release.contains("centos") || os_release.contains("rocky")
           || os_release.contains("alma") {
            return System::Fedora;
        }
    }
    
    // Fallback: check for package manager executables
    if command_exists("pacman") {
        return System::Arch;
    }
    
    if command_exists("apt") || command_exists("apt-get") {
        // Try to distinguish Ubuntu from Debian
        if let Ok(output) = Command::new("lsb_release").arg("-i").output() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if stdout.contains("ubuntu") {
                return System::Ubuntu;
            }
        }
        return System::Debian;
    }
    
    if command_exists("dnf") {
        return System::Fedora;
    }
    
    if command_exists("brew") {
        return System::MacOS;
    }
    
    // Try to get some identifier
    if let Ok(output) = Command::new("uname").arg("-s").output() {
        let os = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return System::Unknown(os);
    }
    
    System::Unknown("Unknown".to_string())
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_system() {
        let system = detect_system();
        // Just ensure it doesn't panic
        println!("Detected system: {:?}", system);
        println!("Package manager: {}", system.package_manager_name());
    }
}

