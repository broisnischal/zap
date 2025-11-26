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
    /// openSUSE (Leap, Tumbleweed)
    OpenSUSE,
    /// FreeBSD
    FreeBSD,
    /// macOS
    MacOS,
    /// Windows
    Windows,
    /// Unknown system
    Unknown(String),
}

impl System {
    pub fn package_manager_name(&self) -> &str {
        match self {
            System::Arch => "AUR + pacman",
            System::Debian | System::Ubuntu => "APT",
            System::Fedora => "DNF",
            System::OpenSUSE => "zypper",
            System::FreeBSD => "pkg",
            System::MacOS => "Homebrew",
            System::Windows => "winget/scoop/choco",
            System::Unknown(_) => "Unknown",
        }
    }

    pub fn is_linux(&self) -> bool {
        matches!(
            self,
            System::Arch | System::Debian | System::Ubuntu | System::Fedora | System::OpenSUSE
        )
    }

    pub fn is_bsd(&self) -> bool {
        matches!(self, System::FreeBSD)
    }

    pub fn is_macos(&self) -> bool {
        matches!(self, System::MacOS)
    }

    pub fn is_windows(&self) -> bool {
        matches!(self, System::Windows)
    }
}

/// Detect the current operating system and distribution
pub fn detect_system() -> System {
    // Check for Windows first
    if cfg!(target_os = "windows") {
        return System::Windows;
    }

    // Check for macOS
    if cfg!(target_os = "macos") {
        return System::MacOS;
    }

    // Check for FreeBSD
    if cfg!(target_os = "freebsd") {
        return System::FreeBSD;
    }

    // On Unix-like systems, check uname first for BSD
    if let Ok(output) = Command::new("uname").arg("-s").output() {
        let os = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
        if os.contains("freebsd") {
            return System::FreeBSD;
        }
    }

    // On Linux, check /etc/os-release
    if let Ok(os_release) = std::fs::read_to_string("/etc/os-release") {
        let os_release_lower = os_release.to_lowercase();

        // Check ID and ID_LIKE fields
        if os_release_lower.contains("id=arch")
            || os_release_lower.contains("id_like=arch")
            || os_release_lower.contains("id_like=\"arch")
            || os_release_lower.contains("manjaro")
            || os_release_lower.contains("endeavouros")
            || os_release_lower.contains("garuda")
            || os_release_lower.contains("artix")
            || os_release_lower.contains("cachyos")
        {
            return System::Arch;
        }

        if os_release_lower.contains("id=opensuse")
            || os_release_lower.contains("id=suse")
            || os_release_lower.contains("id_like=opensuse")
            || os_release_lower.contains("id_like=\"suse")
        {
            return System::OpenSUSE;
        }

        if os_release_lower.contains("id=ubuntu") {
            return System::Ubuntu;
        }

        if os_release_lower.contains("id=debian")
            || os_release_lower.contains("id_like=debian")
            || os_release_lower.contains("id_like=\"debian")
            || os_release_lower.contains("linuxmint")
            || os_release_lower.contains("pop")
            || os_release_lower.contains("elementary")
            || os_release_lower.contains("zorin")
            || os_release_lower.contains("kali")
            || os_release_lower.contains("parrot")
            || os_release_lower.contains("mx")
        {
            return System::Debian;
        }

        if os_release_lower.contains("id=fedora")
            || os_release_lower.contains("id_like=fedora")
            || os_release_lower.contains("id_like=\"fedora")
            || os_release_lower.contains("rhel")
            || os_release_lower.contains("centos")
            || os_release_lower.contains("rocky")
            || os_release_lower.contains("alma")
            || os_release_lower.contains("oracle")
            || os_release_lower.contains("nobara")
        {
            return System::Fedora;
        }
    }

    // Fallback: check for package manager executables
    if command_exists("pacman") {
        return System::Arch;
    }

    if command_exists("zypper") {
        return System::OpenSUSE;
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

    if command_exists("dnf") || command_exists("yum") {
        return System::Fedora;
    }

    if command_exists("pkg") {
        // Could be FreeBSD or Termux
        if let Ok(output) = Command::new("uname").arg("-s").output() {
            let os = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
            if os.contains("freebsd") {
                return System::FreeBSD;
            }
        }
    }

    if command_exists("brew") {
        return System::MacOS;
    }

    // Windows package managers
    if command_exists("winget") || command_exists("scoop") || command_exists("choco") {
        return System::Windows;
    }

    // Try to get some identifier
    if let Ok(output) = Command::new("uname").arg("-s").output() {
        let os = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return System::Unknown(os);
    }

    System::Unknown("Unknown".to_string())
}

/// Get a list of available package managers on the system
pub fn detect_available_package_managers() -> Vec<&'static str> {
    let mut managers = vec![];

    // Native package managers
    if command_exists("pacman") {
        managers.push("pacman");
    }
    if command_exists("yay") || command_exists("paru") {
        managers.push("aur");
    }
    if command_exists("apt") || command_exists("apt-get") {
        managers.push("apt");
    }
    if command_exists("dnf") {
        managers.push("dnf");
    }
    if command_exists("zypper") {
        managers.push("zypper");
    }
    if command_exists("pkg") {
        managers.push("pkg");
    }
    if command_exists("brew") {
        managers.push("brew");
    }

    // Universal package managers
    if command_exists("flatpak") {
        managers.push("flatpak");
    }
    if command_exists("snap") {
        managers.push("snap");
    }

    // Language package managers
    if command_exists("pip") || command_exists("pip3") {
        managers.push("pip");
    }
    if command_exists("cargo") {
        managers.push("cargo");
    }
    if command_exists("go") {
        managers.push("go");
    }
    if command_exists("npm") {
        managers.push("npm");
    }

    managers
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

    #[test]
    fn test_detect_available_package_managers() {
        let managers = detect_available_package_managers();
        println!("Available package managers: {:?}", managers);
        // Should at least detect something on most systems
    }
}
