use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

#[cfg(not(target_os = "windows"))]
use super::sudo;

static AUTO_APPROVE: AtomicBool = AtomicBool::new(false);

/// Targets that can be bootstrapped automatically
#[derive(Debug, Clone, Copy)]
pub enum BootstrapTarget {
    Winget,
    Scoop,
    Choco,
    Python,
}

pub fn set_auto_approve(value: bool) {
    AUTO_APPROVE.store(value, Ordering::Relaxed);
}

fn auto_approve() -> bool {
    AUTO_APPROVE.load(Ordering::Relaxed)
}

pub fn ensure_tool(target: BootstrapTarget) -> Result<()> {
    if target.is_installed() {
        return Ok(());
    }

    if !target.supported_on_current_platform() {
        anyhow::bail!(
            "{} bootstrap is not supported on this platform",
            target.display_name()
        );
    }

    if !confirm_install(target)? {
        anyhow::bail!(
            "{} is required but was not installed",
            target.display_name()
        );
    }

    target.install()?;

    if !target.is_installed() {
        anyhow::bail!(
            "Failed to install {} (command still unavailable)",
            target.display_name()
        );
    }

    Ok(())
}

fn confirm_install(target: BootstrapTarget) -> Result<bool> {
    if auto_approve() {
        println!(
            "--> Auto-confirmed installation for {}",
            target.display_name()
        );
        return Ok(true);
    }

    println!("{} is required but not installed.", target.display_name());
    print!("Install {} now? [Y/n]: ", target.display_name());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    Ok(trimmed.is_empty() || trimmed == "y" || trimmed == "yes")
}

impl BootstrapTarget {
    fn display_name(&self) -> &'static str {
        match self {
            Self::Winget => "winget",
            Self::Scoop => "Scoop",
            Self::Choco => "Chocolatey",
            Self::Python => "Python",
        }
    }

    fn command(&self) -> Option<&'static str> {
        match self {
            Self::Winget => Some("winget"),
            Self::Scoop => Some("scoop"),
            Self::Choco => Some("choco"),
            Self::Python => None,
        }
    }

    fn supported_on_current_platform(&self) -> bool {
        match self {
            Self::Winget | Self::Scoop | Self::Choco => cfg!(target_os = "windows"),
            Self::Python => true,
        }
    }

    fn is_installed(&self) -> bool {
        match self {
            Self::Python => {
                command_exists("python3") || command_exists("python") || command_exists("py")
            }
            _ => self.command().map(command_exists).unwrap_or(false),
        }
    }

    fn install(&self) -> Result<()> {
        match self {
            Self::Winget => install_winget(),
            Self::Scoop => install_scoop(),
            Self::Choco => install_choco(),
            Self::Python => install_python(),
        }
    }
}

fn install_winget() -> Result<()> {
    if !cfg!(target_os = "windows") {
        anyhow::bail!("winget install is only available on Windows");
    }

    println!("--> Installing winget (App Installer)...");
    let script = r#"
$ErrorActionPreference = 'Stop'
$bundle = "$env:TEMP\winget.msixbundle"
Invoke-WebRequest -UseBasicParsing -Uri https://aka.ms/getwinget -OutFile $bundle
Add-AppxPackage -Path $bundle
"#;
    run_powershell(script)?;
    Ok(())
}

fn install_scoop() -> Result<()> {
    if !cfg!(target_os = "windows") {
        anyhow::bail!("Scoop installation is only available on Windows");
    }

    println!("--> Installing Scoop...");
    let script = r#"
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned -Force
$env:SCOOP='C:\Scoop'
[Environment]::SetEnvironmentVariable('SCOOP', $env:SCOOP, 'User')
Invoke-Expression (New-Object System.Net.WebClient).DownloadString('https://get.scoop.sh')
"#;
    run_powershell(script)?;
    Ok(())
}

fn install_choco() -> Result<()> {
    if !cfg!(target_os = "windows") {
        anyhow::bail!("Chocolatey installation is only available on Windows");
    }

    println!("--> Installing Chocolatey...");
    let script = r#"
Set-ExecutionPolicy Bypass -Scope Process -Force
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
"#;
    run_powershell(script)?;
    Ok(())
}

fn install_python() -> Result<()> {
    if cfg!(target_os = "windows") {
        install_python_windows()
    } else {
        install_python_posix()
    }
}

#[cfg(target_os = "windows")]
fn install_python_windows() -> Result<()> {
    println!("--> Installing Python via available Windows package manager...");

    if command_exists("winget") {
        let mut cmd = Command::new("winget");
        cmd.args([
            "install",
            "--id",
            "Python.Python.3",
            "--exact",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ]);
        return run_command(cmd, "winget install Python");
    }

    if command_exists("scoop") {
        let mut cmd = Command::new("scoop");
        cmd.args(["install", "python"]);
        return run_command(cmd, "scoop install python");
    }

    if command_exists("choco") {
        let mut cmd = Command::new("choco");
        cmd.args(["install", "python", "-y"]);
        return run_command(cmd, "choco install python");
    }

    anyhow::bail!("No supported package manager found to install Python automatically");
}

#[cfg(not(target_os = "windows"))]
fn install_python_windows() -> Result<()> {
    anyhow::bail!("Python install for Windows invoked on a non-Windows target");
}

#[cfg(not(target_os = "windows"))]
fn install_python_posix() -> Result<()> {
    println!("--> Installing python3 + pip via system package manager...");

    if command_exists("apt") {
        run_sudo_cmd(&["apt", "update"])?;
        run_sudo_cmd(&["apt", "install", "-y", "python3", "python3-pip"])?;
        return Ok(());
    }

    if command_exists("dnf") {
        run_sudo_cmd(&["dnf", "install", "-y", "python3", "python3-pip"])?;
        return Ok(());
    }

    if command_exists("pacman") {
        run_sudo_cmd(&["pacman", "-Sy", "--noconfirm", "python", "python-pip"])?;
        return Ok(());
    }

    if command_exists("brew") {
        let mut cmd = Command::new("brew");
        cmd.args(["install", "python"]);
        return run_command(cmd, "brew install python");
    }

    anyhow::bail!("Automatic Python install is not supported on this platform");
}

#[cfg(target_os = "windows")]
fn install_python_posix() -> Result<()> {
    anyhow::bail!("POSIX install helper invoked on Windows target");
}

fn run_powershell(script: &str) -> Result<()> {
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to run PowerShell")?;

    if !status.success() {
        anyhow::bail!("PowerShell command failed with status {:?}", status.code());
    }
    Ok(())
}

fn run_command(mut command: Command, description: &str) -> Result<()> {
    let status = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to run {}", description))?;

    if !status.success() {
        anyhow::bail!("{} failed with status {:?}", description, status.code());
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn run_sudo_cmd(args: &[&str]) -> Result<()> {
    let status = sudo::run_sudo(args)?;
    if !status.success() {
        anyhow::bail!("Command {:?} failed with status {:?}", args, status.code());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_sudo_cmd(_args: &[&str]) -> Result<()> {
    anyhow::bail!("run_sudo_cmd should not be called on Windows");
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
