#!/bin/bash
#
# LXE CLI Installer
# Installs the lxe package builder tool
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/lxe-core/lxe/main/install.sh | bash
#

set -e

REPO="lxe-core/lxe"
INSTALL_DIR="${LXE_INSTALL_DIR:-/usr/local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64)
        BINARY="lxe-x86_64-linux-musl.tar.gz"
        ;;
    aarch64)
        BINARY="lxe-aarch64-linux.tar.gz"
        ;;
    *)
        error "Unsupported architecture: $ARCH"
        ;;
esac

# Check if running as root
SUDO=""
if [ "$EUID" -ne 0 ]; then
    if command -v sudo &> /dev/null; then
        SUDO="sudo"
    else
        warn "Not running as root and sudo not found."
        warn "Installing to ~/.local/bin instead."
        INSTALL_DIR="$HOME/.local/bin"
        mkdir -p "$INSTALL_DIR"
    fi
fi

info "LXE CLI Installer"
info "Architecture: $ARCH"
info "Install directory: $INSTALL_DIR"
echo

# Get latest release URL
RELEASE_URL="https://github.com/$REPO/releases/latest/download/$BINARY"

info "Downloading LXE from $RELEASE_URL..."
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

curl -fsSL "$RELEASE_URL" -o "$TEMP_DIR/lxe.tar.gz" || error "Failed to download LXE"

info "Extracting..."
tar -xzf "$TEMP_DIR/lxe.tar.gz" -C "$TEMP_DIR"

# Find the extracted binary
EXTRACTED_BINARY=$(find "$TEMP_DIR" -name "lxe*" -type f ! -name "*.tar.gz" | head -1)
if [ -z "$EXTRACTED_BINARY" ]; then
    error "Could not find extracted binary"
fi

info "Installing to $INSTALL_DIR/lxe..."
$SUDO mv "$EXTRACTED_BINARY" "$INSTALL_DIR/lxe"
$SUDO chmod +x "$INSTALL_DIR/lxe"

# Verify installation
if command -v lxe &> /dev/null; then
    echo
    info "✅ LXE installed successfully!"
    echo
    lxe --version
    echo
    info "Run 'lxe --help' to get started."
else
    echo
    info "✅ LXE installed to $INSTALL_DIR/lxe"
    echo
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warn "Add $INSTALL_DIR to your PATH:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    fi
fi
