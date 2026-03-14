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

# ── Ensure ~/.cargo/bin is in PATH immediately for all users ──────────────────
export PATH="$HOME/.cargo/bin:$PATH"
if ! grep -q 'cargo/bin' /etc/profile.d/cargo.sh 2>/dev/null; then
    echo "==> Adding ~/.cargo/bin to /etc/profile.d/cargo.sh for all users..."
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' | sudo tee /etc/profile.d/cargo.sh > /dev/null
    sudo chmod +x /etc/profile.d/cargo.sh
    source /etc/profile.d/cargo.sh
    echo "   Done. Active immediately."
fi

# ── Detect distro and install missing build dependencies ─────────────────────
if command -v apt-get &>/dev/null; then
    echo "==> Checking build dependencies (Debian/Ubuntu)..."
    MISSING=()
    dpkg -s chafa       &>/dev/null || MISSING+=(chafa)
    dpkg -s libchafa-dev &>/dev/null || MISSING+=(libchafa-dev)
    dpkg -s pkg-config  &>/dev/null || MISSING+=(pkg-config)
    if [ ${#MISSING[@]} -gt 0 ]; then
        echo "  Installing: ${MISSING[*]}"
        sudo apt-get install -y "${MISSING[@]}"
    fi
elif command -v dnf &>/dev/null; then
    echo "==> Checking build dependencies (Fedora/RHEL)..."
    MISSING=()
    rpm -q chafa              &>/dev/null || MISSING+=(chafa)
    rpm -q chafa-devel        &>/dev/null || MISSING+=(chafa-devel)
    rpm -q pkgconf-pkg-config &>/dev/null || MISSING+=(pkgconf-pkg-config)
    if [ ${#MISSING[@]} -gt 0 ]; then
        echo "  Installing: ${MISSING[*]}"
        sudo dnf install -y "${MISSING[@]}"
    fi
fi

# ── Build release binary once ─────────────────────────────────────────────────
echo "==> Building release binary..."
cargo build --release

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
$BUILD_DEB && find target/debian       -name "*.deb" -exec echo "  {}" \;
$BUILD_RPM && find target/generate-rpm -name "*.rpm" -exec echo "  {}" \;
echo ""
echo "Install:"
$BUILD_DEB && echo "  Debian/Ubuntu:  sudo dpkg -i target/debian/VoidDream_*.deb"
$BUILD_RPM && echo "  Fedora/RHEL:    sudo dnf install ./target/generate-rpm/VoidDream-*.rpm"
