#!/bin/bash
#
# zap installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/broisnischal/zap/master/install.sh | bash
#

set -e

# Configuration
REPO="broisnischal/zap"
INSTALL_DIR="${ZAP_INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="zap"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

print_banner() {
    echo -e "${CYAN}"
    echo '  ⚡ zap installer'
    echo '  Fast cross-platform package manager'
    echo -e "${NC}"
}

info() {
    echo -e "${BLUE}==>${NC} $1"
}

success() {
    echo -e "${GREEN}==>${NC} $1"
}

warn() {
    echo -e "${YELLOW}warning:${NC} $1"
}

error() {
    echo -e "${RED}error:${NC} $1"
    exit 1
}

detect_os() {
    local os=""
    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="darwin" ;;
        *)       error "Unsupported operating system: $(uname -s)" ;;
    esac
    echo "$os"
}

detect_arch() {
    local arch=""
    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
    echo "$arch"
}

get_target() {
    local os="$1"
    local arch="$2"
    
    if [ "$os" = "linux" ]; then
        echo "${arch}-unknown-linux-gnu"
    elif [ "$os" = "darwin" ]; then
        echo "${arch}-apple-darwin"
    fi
}

get_latest_version() {
    local version
    version=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    
    if [ -z "$version" ]; then
        error "Failed to fetch latest version. Check your internet connection."
    fi
    
    echo "$version"
}

download_and_install() {
    local version="$1"
    local target="$2"
    local url="https://github.com/${REPO}/releases/download/${version}/zap-${target}.tar.gz"
    local tmp_dir
    
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT
    
    info "Downloading zap ${version} for ${target}..."
    
    if ! curl -fsSL "$url" -o "$tmp_dir/zap.tar.gz"; then
        error "Failed to download from ${url}"
    fi
    
    info "Extracting..."
    tar -xzf "$tmp_dir/zap.tar.gz" -C "$tmp_dir"
    
    info "Installing to ${INSTALL_DIR}..."
    mkdir -p "$INSTALL_DIR"
    mv "$tmp_dir/zap" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
}

check_path() {
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warn "$INSTALL_DIR is not in your PATH"
        echo ""
        echo "Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo ""
        echo -e "  ${CYAN}export PATH=\"\$PATH:$INSTALL_DIR\"${NC}"
        echo ""
    fi
}

verify_installation() {
    if [ -x "$INSTALL_DIR/$BINARY_NAME" ]; then
        success "zap installed successfully!"
        echo ""
        "$INSTALL_DIR/$BINARY_NAME" --version
        echo ""
    else
        error "Installation failed"
    fi
}

main() {
    print_banner
    
    local os arch target version
    
    os=$(detect_os)
    arch=$(detect_arch)
    target=$(get_target "$os" "$arch")
    
    info "Detected: ${os} ${arch}"
    info "Target: ${target}"
    
    # Get version (from argument or latest)
    if [ -n "$1" ]; then
        version="$1"
    else
        version=$(get_latest_version)
    fi
    
    info "Version: ${version}"
    
    download_and_install "$version" "$target"
    check_path
    verify_installation
    
    echo "Run 'zap --help' to get started!"
}

main "$@"

