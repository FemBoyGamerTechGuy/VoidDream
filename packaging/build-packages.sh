#!/usr/bin/env bash
# build-packages.sh — Build both .deb and .rpm packages for NovaDream
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

# ── Argument parsing ──────────────────────────────────────────────────────────
BUILD_DEB=true
BUILD_RPM=true

for arg in "$@"; do
    case "$arg" in
        --deb-only) BUILD_RPM=false ;;
        --rpm-only) BUILD_DEB=false ;;
        --help|-h)
            echo "Usage: $0 [--deb-only | --rpm-only]"
            exit 0 ;;
    esac
done

# ── Icon check ────────────────────────────────────────────────────────────────
ICON_256="assets/icons/hicolor/256x256/apps/io.github.FemBoyGamerTechGuy.NovaDream.png"
if [ ! -f "$ICON_256" ]; then
    echo "ERROR: Icon not found at $ICON_256"
    echo ""
    echo "Add PNG icons at the following sizes before packaging:"
    for size in 16 32 48 64 128 256; do
        echo "  assets/icons/hicolor/${size}x${size}/apps/io.github.FemBoyGamerTechGuy.NovaDream.png"
    done
    echo ""
    echo "If you have a single SVG/PNG, you can resize with ImageMagick:"
    echo "  for s in 16 32 48 64 128 256; do"
    echo "    convert icon.png -resize \${s}x\${s} assets/icons/hicolor/\${s}x\${s}/apps/io.github.FemBoyGamerTechGuy.NovaDream.png"
    echo "  done"
    exit 1
fi

# ── Build release binary once ─────────────────────────────────────────────────
echo "==> Building release binary..."
cargo build --release
strip -s target/release/NovaDream

# ── .deb ──────────────────────────────────────────────────────────────────────
if $BUILD_DEB; then
    echo ""
    echo "==> Building .deb package..."
    if ! command -v cargo-deb &>/dev/null; then
        echo "  Installing cargo-deb..."
        cargo install cargo-deb
    fi
    cargo deb --no-build
    DEB=$(find target/debian -name "*.deb" | head -1)
    echo "  ✓ $DEB"
fi

# ── .rpm ──────────────────────────────────────────────────────────────────────
if $BUILD_RPM; then
    echo ""
    echo "==> Building .rpm package..."
    if ! command -v cargo-generate-rpm &>/dev/null; then
        echo "  Installing cargo-generate-rpm..."
        cargo install cargo-generate-rpm
    fi
    cargo generate-rpm
    RPM=$(find target/generate-rpm -name "*.rpm" | head -1)
    echo "  ✓ $RPM"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "Done! Packages:"
$BUILD_DEB && find target/debian      -name "*.deb" -exec echo "  {}" \;
$BUILD_RPM && find target/generate-rpm -name "*.rpm" -exec echo "  {}" \;
echo ""
echo "Install:"
$BUILD_DEB && echo "  Debian/Ubuntu:   sudo dpkg -i target/debian/NovaDream_*.deb && sudo apt-get install -f"
$BUILD_RPM && echo "  Fedora/RHEL:     sudo dnf install target/generate-rpm/NovaDream-*.rpm"
