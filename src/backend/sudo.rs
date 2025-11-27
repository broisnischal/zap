use anyhow::{Context, Result};
use std::io::{self, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::OnceLock;

/// Global password storage - only set once per session
static SUDO_PASSWORD: OnceLock<String> = OnceLock::new();

/// Check if we need sudo (not running as root)
pub fn needs_sudo() -> bool {
    unsafe { libc::geteuid() != 0 }
}

/// Prompt for sudo password if not already cached
pub fn ensure_password() -> Result<()> {
    if !needs_sudo() {
        return Ok(()); // Running as root, no password needed
    }

    if SUDO_PASSWORD.get().is_some() {
        return Ok(()); // Already have password
    }

    // First try to see if we already have sudo access (e.g., from recent sudo use)
    let check = Command::new("sudo")
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if let Ok(status) = check {
        if status.success() {
            // We have passwordless sudo access, set empty password
            let _ = SUDO_PASSWORD.set(String::new());
            return Ok(());
        }
    }

    // Need to prompt for password
    prompt_password()?;
    Ok(())
}

/// Prompt user for their sudo password
fn prompt_password() -> Result<()> {
    println!();
    print!("\x1b[1;33m[sudo]\x1b[0m Password required for installation. Enter password: ");
    io::stdout().flush()?;

    // Read password without echoing
    let password = rpassword::read_password().context("Failed to read password")?;

    // Verify the password works
    let mut child = Command::new("sudo")
        .args(["-S", "-v"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn sudo")?;

    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{}", password)?;
    }

    let status = child.wait().context("Failed to wait for sudo")?;

    if !status.success() {
        anyhow::bail!("Invalid password");
    }

    // Store the password
    let _ = SUDO_PASSWORD.set(password);
    println!("\x1b[1;32m✓\x1b[0m Password verified\n");

    Ok(())
}

/// Get the cached password (if any)
fn get_password() -> Option<&'static String> {
    SUDO_PASSWORD.get()
}

/// Run a command with sudo, using cached password
pub fn run_sudo(args: &[&str]) -> Result<ExitStatus> {
    ensure_password()?;

    if !needs_sudo() {
        // Running as root, just run the command directly
        let status = Command::new(args[0])
            .args(&args[1..])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run command")?;
        return Ok(status);
    }

    let password = get_password();

    // If we have an empty password (passwordless sudo), run without -S
    if password.map(|p| p.is_empty()).unwrap_or(false) {
        let status = Command::new("sudo")
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run sudo command")?;
        return Ok(status);
    }

    // Run with password via stdin
    let mut child = Command::new("sudo")
        .arg("-S")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn sudo command")?;

    if let (Some(mut stdin), Some(password)) = (child.stdin.take(), password) {
        // Write password followed by newline
        let _ = writeln!(stdin, "{}", password);
        // Drop stdin to close it - this is important!
    }

    let status = child.wait().context("Failed to wait for sudo command")?;
    Ok(status)
}

/// Run a command with sudo and return its output
pub fn run_sudo_output(args: &[&str]) -> Result<std::process::Output> {
    ensure_password()?;

    if !needs_sudo() {
        // Running as root, just run the command directly
        let output = Command::new(args[0])
            .args(&args[1..])
            .output()
            .context("Failed to run command")?;
        return Ok(output);
    }

    let password = get_password();

    // If we have an empty password (passwordless sudo), run without -S
    if password.map(|p| p.is_empty()).unwrap_or(false) {
        let output = Command::new("sudo")
            .args(args)
            .output()
            .context("Failed to run sudo command")?;
        return Ok(output);
    }

    // Run with password via stdin
    let mut child = Command::new("sudo")
        .arg("-S")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn sudo command")?;

    if let (Some(mut stdin), Some(password)) = (child.stdin.take(), password) {
        let _ = writeln!(stdin, "{}", password);
    }

    let output = child
        .wait_with_output()
        .context("Failed to wait for sudo command")?;
    Ok(output)
}

/// Run a command with sudo, piping from a specific directory (for makepkg, etc.)
pub fn run_sudo_in_dir(args: &[&str], dir: &std::path::Path) -> Result<ExitStatus> {
    ensure_password()?;

    if !needs_sudo() {
        let status = Command::new(args[0])
            .args(&args[1..])
            .current_dir(dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run command")?;
        return Ok(status);
    }

    let password = get_password();

    if password.map(|p| p.is_empty()).unwrap_or(false) {
        let status = Command::new("sudo")
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run sudo command")?;
        return Ok(status);
    }

    let mut child = Command::new("sudo")
        .arg("-S")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn sudo command")?;

    if let (Some(mut stdin), Some(password)) = (child.stdin.take(), password) {
        let _ = writeln!(stdin, "{}", password);
    }

    let status = child.wait().context("Failed to wait for sudo command")?;
    Ok(status)
}
