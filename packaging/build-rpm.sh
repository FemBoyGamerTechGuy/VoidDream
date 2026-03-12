#!/usr/bin/env bash
# build-rpm.sh — Build a .rpm package for NovaDream
# Requires: cargo-generate-rpm, gtk4-devel, pkg-config
# Install cargo-generate-rpm: cargo install cargo-generate-rpm
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

echo "==> Checking dependencies..."
if ! command -v cargo-generate-rpm &>/dev/null; then
    echo "cargo-generate-rpm not found. Installing..."
    cargo install cargo-generate-rpm
fi

# Icons must exist before packaging
ICON_256="assets/icons/hicolor/256x256/apps/io.github.FemBoyGamerTechGuy.NovaDream.png"
if [ ! -f "$ICON_256" ]; then
    echo "ERROR: Icon not found at $ICON_256"
    echo "Please add your icons to assets/icons/hicolor/<size>/apps/ before packaging."
    exit 1
fi

echo "==> Building release binary..."
cargo build --release

echo "==> Stripping binary..."
strip -s target/release/NovaDream

echo "==> Building .rpm package..."
cargo generate-rpm

RPM=$(find target/generate-rpm -name "*.rpm" | head -1)
echo ""
echo "✓ Package built: $RPM"
echo ""
echo "Install with:"
echo "  sudo rpm -i $RPM"
echo "  # or with dnf:"
echo "  sudo dnf install $RPM"
