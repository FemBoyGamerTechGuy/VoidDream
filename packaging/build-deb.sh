#!/usr/bin/env bash
# Builds a .deb package for VoidDream.
# Run from the repo root: bash packaging/build-deb.sh
set -euo pipefail

PKG_NAME="voiddream"
PKG_VERSION="0.1.2"
PKG_ARCH="amd64"
PKG_DIR="$(mktemp -d)/voiddream_${PKG_VERSION}_${PKG_ARCH}"

echo "==> Building release binary..."
cargo build --release

echo "==> Staging package tree..."

# ── Binary ────────────────────────────────────────────────────────────────────
install -Dm755 target/release/VoidDream \
    "$PKG_DIR/usr/bin/VoidDream"

# ── Themes ────────────────────────────────────────────────────────────────────
install -dm755 "$PKG_DIR/usr/share/VoidDream/themes"
install -Dm644 themes/*.json \
    "$PKG_DIR/usr/share/VoidDream/themes/"

# ── Icon sets ─────────────────────────────────────────────────────────────────
install -dm755 "$PKG_DIR/usr/share/VoidDream/icons"
install -Dm644 icons/*.json \
    "$PKG_DIR/usr/share/VoidDream/icons/"

# ── Desktop entry ─────────────────────────────────────────────────────────────
install -Dm644 assets/desktop/io.github.FemBoyGamerTechGuy.VoidDream.desktop \
    "$PKG_DIR/usr/share/applications/io.github.FemBoyGamerTechGuy.VoidDream.desktop"

# ── DEBIAN control ────────────────────────────────────────────────────────────
install -dm755 "$PKG_DIR/DEBIAN"
cat > "$PKG_DIR/DEBIAN/control" << EOF
Package: $PKG_NAME
Version: $PKG_VERSION
Architecture: $PKG_ARCH
Maintainer: FemBoyGamerTechGuy <https://github.com/FemBoyGamerTechGuy>
Description: A dreamy void-themed TUI file manager built with Rust and Ratatui
 VoidDream is a fast, keyboard-driven file manager for the terminal.
 It features a three-pane layout, live file previews, fuzzy search,
 multi-tab navigation, and a fully themeable interface.
Homepage: https://github.com/FemBoyGamerTechGuy/VoidDream
Depends: libc6
Recommends: ffmpeg, mpv, neovim, chafa, unrar, unzip, p7zip-full, zstd
Section: utils
Priority: optional
EOF

# ── Build the .deb ────────────────────────────────────────────────────────────
DEB_FILE="${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.deb"
echo "==> Building $DEB_FILE..."
dpkg-deb --build --root-owner-group "$PKG_DIR" "$DEB_FILE"

echo ""
echo "Done: $DEB_FILE"
echo "Install with: sudo dpkg -i $DEB_FILE"
