#!/usr/bin/env bash
# Builds an .rpm package for VoidDream using cargo-generate-rpm.
# Run from the repo root: bash packaging/build-rpm.sh
set -euo pipefail

# Install cargo-generate-rpm if not present
if ! cargo generate-rpm --version &>/dev/null; then
    echo "==> Installing cargo-generate-rpm..."
    cargo install cargo-generate-rpm
fi

echo "==> Building release binary..."
cargo build --release

echo "==> Building .rpm..."
cargo generate-rpm

echo ""
echo "Done. Package is in target/generate-rpm/"
echo "Install with: sudo dnf install ./target/generate-rpm/VoidDream-*.rpm"
