#!/usr/bin/env bash
# build-packages.sh — Build both .deb and .rpm packages for VoidDream
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

# ── Build release binary once ─────────────────────────────────────────────────
echo "==> Building release binary..."
cargo build --release

# ── .deb ──────────────────────────────────────────────────────────────────────
if $BUILD_DEB; then
    echo ""
    echo "==> Building .deb package..."
    if ! cargo deb --version &>/dev/null; then
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
    if ! cargo generate-rpm --version &>/dev/null; then
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
$BUILD_DEB && find target/debian       -name "*.deb" -exec echo "  {}" \;
$BUILD_RPM && find target/generate-rpm -name "*.rpm" -exec echo "  {}" \;
echo ""
echo "Install:"
$BUILD_DEB && echo "  Debian/Ubuntu:  sudo dpkg -i target/debian/VoidDream_*.deb"
$BUILD_RPM && echo "  Fedora/RHEL:    sudo dnf install ./target/generate-rpm/VoidDream-*.rpm"
