#!/usr/bin/env bash
# install-deps-fedora.sh — Install default runtime apps for VoidDream on Fedora/RHEL
set -euo pipefail

echo "==> Installing VoidDream runtime dependencies..."
echo "    (these are the default apps used by VoidDream's openers)"
echo ""

# Enable RPM Fusion for unrar and ffmpeg
if ! rpm -q rpmfusion-free-release &>/dev/null; then
  echo "==> Enabling RPM Fusion (needed for unrar and full ffmpeg)..."
  sudo dnf install -y \
    "https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm"
fi

sudo dnf install -y \
  mirage \
  mpv \
  libreoffice \
  neovim \
  java-latest-openjdk \
  kitty \
  ffmpeg \
  chafa \
  unrar \
  unzip \
  p7zip \
  zstd \
  google-noto-emoji-color-fonts

echo ""
echo "Done! All default VoidDream runtime dependencies are installed."
echo ""
echo "Optional — install a Nerd Font for the nerdfont icon set:"
echo "  mkdir -p ~/.local/share/fonts"
echo "  curl -fLo ~/.local/share/fonts/FiraCodeNerdFont-Regular.ttf \\"
echo "    https://github.com/ryanoasis/nerd-fonts/raw/HEAD/patched-fonts/FiraCode/Regular/FiraCodeNerdFont-Regular.ttf"
echo "  fc-cache -fv"
