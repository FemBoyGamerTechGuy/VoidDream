#!/usr/bin/env bash
# Builds a .deb package for VoidDream using cargo-deb.
# Run from the repo root: bash packaging/build-deb.sh
set -euo pipefail

# ── Ensure ~/.cargo/bin is in PATH immediately for all users ──────────────────
export PATH="$HOME/.cargo/bin:$PATH"
if ! grep -q 'cargo/bin' /etc/profile.d/cargo.sh 2>/dev/null; then
    echo "==> Adding ~/.cargo/bin to /etc/profile.d/cargo.sh for all users..."
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' | sudo tee /etc/profile.d/cargo.sh > /dev/null
    sudo chmod +x /etc/profile.d/cargo.sh
    source /etc/profile.d/cargo.sh
    echo "   Done. Active immediately."
fi

# ── Build dependencies ────────────────────────────────────────────────────────
echo "==> Checking build dependencies..."
MISSING=()
dpkg -s chafa        &>/dev/null || MISSING+=(chafa)
dpkg -s libchafa-dev &>/dev/null || MISSING+=(libchafa-dev)
dpkg -s pkg-config   &>/dev/null || MISSING+=(pkg-config)

if [ ${#MISSING[@]} -gt 0 ]; then
    echo "==> Installing missing build dependencies: ${MISSING[*]}"
    sudo apt-get install -y "${MISSING[@]}"
fi

# ── cargo-deb ─────────────────────────────────────────────────────────────────
if ! command -v cargo-deb &>/dev/null; then
    echo "==> Installing cargo-deb..."
    cargo install cargo-deb
fi

# ── Build ─────────────────────────────────────────────────────────────────────
echo "==> Building .deb..."
cargo deb

DEB=$(find target/debian -name "*.deb" | head -1)
echo ""
echo "Done: $DEB"
echo "Install with: sudo dpkg -i $DEB"
