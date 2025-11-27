# zap ⚡

A fast, cross-platform universal package manager that auto-detects your system.

[![Release](https://img.shields.io/github/v/release/broisnischal/zap?style=flat-square)](https://github.com/broisnischal/zap/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

## Installation

### Quick Install (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/broisnischal/zap/main/install.sh | bash
```

### Install Specific Version

```bash
curl -fsSL https://raw.githubusercontent.com/broisnischal/zap/main/install.sh | bash -s v0.1.0
```

### Manual Download

Download the latest release for your platform from [GitHub Releases](https://github.com/broisnischal/zap/releases):

| Platform | Architecture | Download |
|----------|--------------|----------|
| Linux | x86_64 | [zap-x86_64-unknown-linux-gnu.tar.gz](https://github.com/broisnischal/zap/releases/latest/download/zap-x86_64-unknown-linux-gnu.tar.gz) |
| Linux | ARM64 | [zap-aarch64-unknown-linux-gnu.tar.gz](https://github.com/broisnischal/zap/releases/latest/download/zap-aarch64-unknown-linux-gnu.tar.gz) |
| macOS | Intel | [zap-x86_64-apple-darwin.tar.gz](https://github.com/broisnischal/zap/releases/latest/download/zap-x86_64-apple-darwin.tar.gz) |
| macOS | Apple Silicon | [zap-aarch64-apple-darwin.tar.gz](https://github.com/broisnischal/zap/releases/latest/download/zap-aarch64-apple-darwin.tar.gz) |
| FreeBSD | x86_64 | [zap-x86_64-unknown-freebsd.tar.gz](https://github.com/broisnischal/zap/releases/latest/download/zap-x86_64-unknown-freebsd.tar.gz) |

```bash
# Example: Linux x86_64
curl -LO https://github.com/broisnischal/zap/releases/latest/download/zap-x86_64-unknown-linux-gnu.tar.gz
tar -xzf zap-x86_64-unknown-linux-gnu.tar.gz
sudo mv zap /usr/local/bin/
```

### Windows Install (PowerShell)

To install globally on Windows, run PowerShell as Administrator and execute:

```powershell
$dest = "$env:ProgramFiles\zap"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Invoke-WebRequest https://github.com/broisnischal/zap/releases/latest/download/zap-x86_64-pc-windows-msvc.zip -OutFile zap.zip
Expand-Archive zap.zip -DestinationPath $dest -Force
Remove-Item zap.zip
setx PATH "$($env:PATH);$dest"
```

Restart your terminal and `zap.exe` will be available everywhere (e.g., `zap system`).

### Build from Source

```bash
# Clone the repository
git clone https://github.com/broisnischal/zap.git
cd zap

# Build and install
cargo install --path . --force
```

## Supported Package Managers

### System Package Managers

| System | Package Manager | Backend Flag | Status |
|--------|-----------------|--------------|--------|
| Arch Linux | pacman + AUR | `-b aur` | ✅ Full support |
| Arch Linux | pacman (repos only) | `-b pacman` | ✅ Full support |
| Manjaro | pacman + AUR | `-b aur` | ✅ Full support |
| EndeavourOS | pacman + AUR | `-b aur` | ✅ Full support |
| Debian | APT | `-b apt` | ✅ Full support |
| Ubuntu | APT | `-b apt` | ✅ Full support |
| Linux Mint | APT | `-b apt` | ✅ Full support |
| Pop!_OS | APT | `-b apt` | ✅ Full support |
| Fedora | DNF | `-b dnf` | ✅ Full support |
| RHEL/CentOS | DNF | `-b dnf` | ✅ Full support |
| Rocky Linux | DNF | `-b dnf` | ✅ Full support |
| AlmaLinux | DNF | `-b dnf` | ✅ Full support |
| openSUSE | zypper | `-b zypper` | ✅ Full support |
| SUSE Linux | zypper | `-b zypper` | ✅ Full support |
| FreeBSD | pkg | `-b pkg` | ✅ Full support |
| macOS | Homebrew | `-b brew` | ✅ Full support |
| Windows 10/11 | winget / Scoop / Chocolatey | `-b winget`, `-b scoop`, `-b choco` | ✅ Full support |

### Universal Package Managers

| Package Manager | Backend Flag | Status |
|-----------------|--------------|--------|
| Flatpak | `-b flatpak` | ✅ Full support |
| Snap | `-b snap` | ✅ Full support |

### Language Package Managers

| Language | Package Manager | Backend Flag | Status |
|----------|-----------------|--------------|--------|
| Python | pip | `-b pip` | ✅ Full support |
| Rust | Cargo | `-b cargo` | ✅ Full support |
| Go | go install | `-b go` | ✅ Full support |
| Node.js | npm | `-b npm` | ✅ Full support |

## Features

- **⚡ Auto-Detection** - Automatically detects your OS and uses the right package manager
- **🔍 Smart Search** - Search packages with live interactive results
- **📦 Unified Interface** - Same commands work across 12+ package managers
- **🎨 Beautiful UI** - Colorful, interactive terminal interface
- **🔄 Multi-Backend** - Switch between package managers with `-b` flag
- **🌍 Cross-Platform** - Works on Linux, macOS, FreeBSD, and more
- **🧰 Self-Bootstrapping** - Installs missing package managers (winget/scoop/choco) and runtimes (Python for pip) on demand

## Usage

### Quick Install
```bash
# Install packages directly (uses auto-detected backend)
zap package1 package2 package3

# Install from a specific backend
zap -b flatpak package1
zap -b pip numpy pandas
zap -b cargo ripgrep
```

### Interactive Mode
```bash
# Start interactive live search
zap

# Interactive mode with specific backend
zap -b aur
```

In interactive mode:
- Type to search (minimum 2 characters)
- `Space` - Select/deselect package
- `Enter` - Install selected packages
- `Up/Down` - Navigate
- `Esc` - Quit

### Search Packages
```bash
# Search for packages
zap search firefox

# Search with detailed info
zap search -i firefox

# Search with interactive selection
zap search -I firefox

# Search using a specific backend
zap -b flatpak search firefox
```

### Package Info
```bash
# Get detailed package info
zap info neovim

# Info from specific backend
zap -b pip info numpy
```

### Update Packages
```bash
# Check and update packages
zap update

# Update packages from specific backend
zap -b cargo update
```

### System Info
```bash
# Show detected system and package manager
zap system
```

### List Available Package Managers
```bash
# Show all available package managers on your system
zap managers
```

### List Installed Packages
```bash
# Show everything installed through the current backend
zap list

# Force a specific backend
zap -b pip list
zap -b npm list
```

### Language Package Managers
```bash
# Search npm interactively
zap -b npm

# Install npm packages directly
zap -b npm install typescript eslint

# Live search PyPI packages
zap -b pip

# Fetch detailed info from pip/npm
zap -b pip info numpy
zap -b npm info react
```

Use `zap search -I <term>` with `-b npm` or `-b pip` for fuzzy selection inside those ecosystems.

## Commands

| Command | Alias | Description |
|---------|-------|-------------|
| `search <query>` | `s` | Search for packages |
| `install <packages>` | `i` | Install packages |
| `info <package>` | - | Show package details |
| `interactive` | `int` | Interactive mode |
| `update` | - | Update packages |
| `system` | - | Show system info |
| `managers` | `pm` | List available package managers |
| `list` | `ls` | Show packages installed via current backend |
| `self-update` | `selfupdate` | Check for zap CLI updates |

## Global Options

| Option | Short | Description |
|--------|-------|-------------|
| `--backend <backend>` | `-b` | Select specific package manager backend |
| `--yes` | `-y` | Auto-accept all prompts |
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |

### Available Backends

```
System:     apt, aur, brew, dnf, pacman, pkg, zypper, winget, scoop, choco
Universal:  flatpak, snap
Language:   cargo, go, pip, npm
```

`zap` will automatically prompt to install missing Windows package managers (winget, Scoop, Chocolatey) or Python for the `pip` backend. Use `-y/--yes` to auto-approve those prompts in non-interactive environments.

## Updating zap

- `zap` automatically checks GitHub Releases once per run (set `ZAP_DISABLE_UPDATE_CHECK=1` to skip).
- Run `zap self-update` at any time to compare your binary against the latest release.
- To update manually, rerun the install script:
  - Linux/macOS: `curl -fsSL https://raw.githubusercontent.com/broisnischal/zap/main/install.sh | bash -s latest`
  - Windows: download the newest ZIP from the [releases page](https://github.com/broisnischal/zap/releases) and replace `zap.exe`.

## How It Works

```
┌─────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   zap CLI   │────▶│  OS Detection   │────▶│ Backend Select  │
└─────────────┘     └─────────────────┘     └─────────────────┘
                                                     │
        ┌────────────────────────────────────────────┼────────────────────────────────────────────┐
        │                                            │                                            │
        ▼                                            ▼                                            ▼
┌───────────────────┐                    ┌───────────────────┐                    ┌───────────────────┐
│  System Backends  │                    │ Universal Backends│                    │Language Backends  │
├───────────────────┤                    ├───────────────────┤                    ├───────────────────┤
│ • apt (Debian)    │                    │ • flatpak         │                    │ • pip (Python)    │
│ • aur (Arch)      │                    │ • snap            │                    │ • cargo (Rust)    │
│ • dnf (Fedora)    │                    └───────────────────┘                    │ • go (Go)         │
│ • zypper (SUSE)   │                                                             └───────────────────┘
│ • pkg (FreeBSD)   │
│ • brew (macOS)    │
│ • pacman (Arch)   │
└───────────────────┘
```

1. **Detection**: `zap` reads `/etc/os-release` and checks for package manager executables
2. **Backend Selection**: Based on the detected OS or `-b` flag, it loads the appropriate backend
3. **Unified Interface**: All backends implement the same trait, providing consistent behavior

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ZAP_INSTALL_DIR` | Installation directory for the install script | `~/.local/bin` |

## Examples

### Installing packages across different systems

```bash
# On Arch Linux (auto-detects AUR)
zap neovim firefox

# On Ubuntu (auto-detects APT)
zap neovim firefox

# On macOS (auto-detects Homebrew)
zap neovim firefox

# On Fedora (auto-detects DNF)
zap neovim firefox
```

### Using specific backends

```bash
# Install a Flatpak app
zap -b flatpak org.mozilla.firefox

# Install a Snap package
zap -b snap firefox

# Install Python packages
zap -b pip numpy pandas matplotlib

# Install Rust crates
zap -b cargo ripgrep bat fd-find

# Install Go tools
zap -b go github.com/junegunn/fzf
```

### Searching across backends

```bash
# Search in AUR
zap -b aur search visual-studio-code

# Search in Flatpak
zap -b flatpak search vscode

# Search in Cargo (crates.io)
zap -b cargo search json
```

## Uninstall

```bash
# If installed via install script
rm ~/.local/bin/zap

# If installed via cargo
cargo uninstall zap

# If installed manually
sudo rm /usr/local/bin/zap
```

## Development

### Requirements
- Rust 1.70+
- OpenSSL development libraries

### Building
```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

### Project Structure
```
src/
├── main.rs              # CLI entry point
├── backend/
│   ├── mod.rs           # PackageManager trait
│   ├── detect.rs        # OS detection
│   ├── bootstrap.rs     # Package-manager/runtime bootstrap helpers
│   ├── apt.rs           # Debian/Ubuntu backend
│   ├── aur.rs           # Arch Linux AUR backend
│   ├── brew.rs          # macOS Homebrew backend
│   ├── choco.rs         # Windows Chocolatey backend
│   ├── cargo.rs         # Rust Cargo backend
│   ├── dnf.rs           # Fedora/RHEL backend
│   ├── flatpak.rs       # Flatpak backend
│   ├── go.rs            # Go install backend
│   ├── pacman.rs        # Arch Linux pacman backend
│   ├── pip.rs           # Python pip backend
│   ├── pkg.rs           # FreeBSD pkg backend
│   ├── scoop.rs         # Windows Scoop backend
│   ├── snap.rs          # Snap backend
│   ├── winget.rs        # Windows winget backend
│   └── zypper.rs        # openSUSE zypper backend
└── ui/
    └── mod.rs           # TUI components
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Adding a New Backend

To add support for a new package manager:

1. Create a new file in `src/backend/` (e.g., `newpm.rs`)
2. Implement the `PackageManager` trait
3. Add the module to `src/backend/mod.rs`
4. Add the backend choice to `main.rs`
5. Update OS detection in `detect.rs` if needed

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/) 🦀
- TUI powered by [ratatui](https://github.com/ratatui-org/ratatui)
- Inspired by [yay](https://github.com/Jguer/yay), [paru](https://github.com/Morganamilo/paru), [topgrade](https://github.com/topgrade-rs/topgrade)
