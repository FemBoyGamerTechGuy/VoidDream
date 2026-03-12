#!/usr/bin/env bash
# build-deb.sh — Build a .deb package for NovaDream
# Requires: cargo-deb, libgtk-4-dev, pkg-config
# Install cargo-deb: cargo install cargo-deb
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

echo "==> Checking dependencies..."
if ! command -v cargo-deb &>/dev/null; then
    echo "cargo-deb not found. Installing..."
    cargo install cargo-deb
fi

# Icons must exist before packaging — placeholder check
ICON_256="assets/icons/hicolor/256x256/apps/io.github.FemBoyGamerTechGuy.NovaDream.png"
if [ ! -f "$ICON_256" ]; then
    echo "ERROR: Icon not found at $ICON_256"
    echo "Please add your icons to assets/icons/hicolor/<size>/apps/ before packaging."
    exit 1
fi

echo "==> Building release binary..."
cargo build --release

echo "==> Building .deb package..."
cargo deb --no-build

DEB=$(find target/debian -name "*.deb" | head -1)
echo ""
echo "✓ Package built: $DEB"
echo ""
echo "Install with:"
echo "  sudo dpkg -i $DEB"
echo "  sudo apt-get install -f   # fix any missing deps"
