#!/usr/bin/env bash
# Builds a .deb package for VoidDream using cargo-deb.
# Run from the repo root: bash packaging/build-deb.sh
set -euo pipefail

# Install cargo-deb if not present
if ! cargo deb --version &>/dev/null; then
    echo "==> Installing cargo-deb..."
    cargo install cargo-deb
fi

echo "==> Building .deb..."
cargo deb

echo ""
echo "Done. Package is in target/debian/"
echo "Install with: sudo dpkg -i target/debian/VoidDream_*.deb"
