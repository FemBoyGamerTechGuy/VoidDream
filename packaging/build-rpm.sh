#!/usr/bin/env bash
# Builds an .rpm package for VoidDream.
# Run from the repo root: bash packaging/build-rpm.sh
# Requires: rpm-build (sudo dnf install rpm-build)
set -euo pipefail

PKG_NAME="voiddream"
PKG_VERSION="0.1.2"
PKG_RELEASE="1"
ARCH="$(uname -m)"
BUILD_ROOT="$(mktemp -d)/rpmbuild"

echo "==> Building release binary..."
cargo build --release

echo "==> Staging package tree at $BUILD_ROOT..."

# ── Binary ────────────────────────────────────────────────────────────────────
install -Dm755 target/release/VoidDream \
    "$BUILD_ROOT/usr/bin/VoidDream"

# ── Themes ────────────────────────────────────────────────────────────────────
install -dm755 "$BUILD_ROOT/usr/share/VoidDream/themes"
install -Dm644 themes/*.json \
    "$BUILD_ROOT/usr/share/VoidDream/themes/"

# ── Icon sets ─────────────────────────────────────────────────────────────────
install -dm755 "$BUILD_ROOT/usr/share/VoidDream/icons"
install -Dm644 icons/*.json \
    "$BUILD_ROOT/usr/share/VoidDream/icons/"

# ── Desktop entry ─────────────────────────────────────────────────────────────
install -Dm644 assets/desktop/io.github.FemBoyGamerTechGuy.VoidDream.desktop \
    "$BUILD_ROOT/usr/share/applications/io.github.FemBoyGamerTechGuy.VoidDream.desktop"

# ── Generate file list for %files section ─────────────────────────────────────
FILE_LIST=$(find "$BUILD_ROOT" -type f | sed "s|$BUILD_ROOT||")

# ── Write spec file ───────────────────────────────────────────────────────────
SPEC_FILE="$(mktemp /tmp/voiddream-XXXXXX.spec)"
cat > "$SPEC_FILE" << SPEC
Name:       $PKG_NAME
Version:    $PKG_VERSION
Release:    $PKG_RELEASE%{?dist}
Summary:    A dreamy void-themed TUI file manager built with Rust and Ratatui
License:    GPL-3.0-or-later
URL:        https://github.com/FemBoyGamerTechGuy/VoidDream
BuildArch:  $ARCH

Requires:       glibc
Recommends:     ffmpeg
Recommends:     mpv
Recommends:     neovim
Recommends:     chafa
Recommends:     unrar
Recommends:     unzip
Recommends:     p7zip
Recommends:     zstd

%description
VoidDream is a fast, keyboard-driven file manager for the terminal.
It features a three-pane layout, live file previews, fuzzy search,
multi-tab navigation, and a fully themeable interface.

%install
cp -a $BUILD_ROOT/. %{buildroot}/

%files
/usr/bin/VoidDream
/usr/share/VoidDream/
/usr/share/applications/io.github.FemBoyGamerTechGuy.VoidDream.desktop

%changelog
* $(date "+%a %b %d %Y") FemBoyGamerTechGuy - $PKG_VERSION-$PKG_RELEASE
- Packaged by build-rpm.sh
SPEC

# ── Build the RPM ─────────────────────────────────────────────────────────────
RPM_DIR="$(mktemp -d)"
RPM_FILE="${PKG_NAME}-${PKG_VERSION}-${PKG_RELEASE}.${ARCH}.rpm"

echo "==> Building $RPM_FILE..."
rpmbuild -bb "$SPEC_FILE" \
    --buildroot "$BUILD_ROOT" \
    --define "_rpmdir $RPM_DIR" \
    --define "_build_name_fmt %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm" \
    --nodebuginfo 2>&1 | tail -5

find "$RPM_DIR" -name "*.rpm" -exec cp {} "./$RPM_FILE" \;

echo ""
echo "Done: $RPM_FILE"
echo "Install with: sudo rpm -i $RPM_FILE"
echo "         or:  sudo dnf install ./$RPM_FILE"
