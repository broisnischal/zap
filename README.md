# zap ⚡

A fast, cross-platform package manager that auto-detects your system.

## Supported Systems

| System | Package Manager | Status |
|--------|-----------------|--------|
| Arch Linux | AUR + pacman | ✅ Full support |
| Debian/Ubuntu | APT | ✅ Full support |
| Fedora/RHEL | DNF | ✅ Full support |
| macOS | Homebrew | ✅ Full support |

## Features

- **🔍 Smart Search** - Search packages with live results
- **⚡ Auto-Detection** - Automatically detects your OS and uses the right package manager
- **🎨 Beautiful UI** - Colorful, interactive terminal interface
- **📦 Unified Interface** - Same commands work on all platforms
- **✨ Interactive Mode** - Live search and select packages

## Installation

```bash
# Build from source
cargo build --release

# Install to system
cargo install --path .

# Or run directly
cargo run -- <command>
```

## Usage

### Quick Install
```bash
# Install packages directly
zap package1 package2 package3

# With auto-accept (no prompts)
zap -y package1 package2
```

### Interactive Mode
```bash
# Start interactive mode
zap

# Or explicitly
zap interactive
```

In interactive mode:
- Type a search query and press Enter
- Use Space to select/deselect packages
- Press Enter to confirm selection
- Press Esc to quit

### Search Packages
```bash
# Search for packages
zap search firefox

# Search with detailed info
zap search -i firefox

# Search with interactive selection
zap search -I firefox
```

### Package Info
```bash
# Get detailed package info
zap info neovim
```

### Update Packages
```bash
# Check and update packages
zap update
```

### System Info
```bash
# Show detected system and package manager
zap system
```

## Commands

| Command | Alias | Description |
|---------|-------|-------------|
| `search <query>` | `s` | Search for packages |
| `install <packages>` | `i` | Install packages |
| `info <package>` | - | Show package details |
| `interactive` | `int` | Interactive mode |
| `update` | - | Update packages |
| `system` | - | Show system info |

## How It Works

1. **Detection**: `zap` reads `/etc/os-release` and checks for package manager executables
2. **Backend Selection**: Based on the detected OS, it loads the appropriate backend:
   - Arch → AUR backend (searches AUR API, builds with makepkg)
   - Debian/Ubuntu → APT backend (uses apt-cache and apt)
   - Fedora → DNF backend (uses dnf commands)
   - macOS → Homebrew backend (uses brew API and CLI)
3. **Unified Interface**: All backends implement the same trait, providing consistent behavior

## Requirements

### Arch Linux
- `base-devel` package group
- `git` for cloning PKGBUILDs

### Debian/Ubuntu
- `apt` and `dpkg` (pre-installed)

### Fedora
- `dnf` and `rpm` (pre-installed)

### macOS
- [Homebrew](https://brew.sh) installed

### Building
- Rust toolchain (rustc 1.70+)

## License

MIT
