#!/usr/bin/env bash
# install-deps-debian.sh — Install default runtime apps for VoidDream on Debian/Ubuntu
set -euo pipefail

echo "==> Installing VoidDream runtime dependencies..."
echo "    (these are the default apps used by VoidDream's openers)"
echo ""

sudo apt-get update

sudo apt-get install -y \
    mirage \
    mpv \
    libreoffice \
    neovim \
    default-jre \
    kitty \
    ffmpeg \
    chafa \
    unrar \
    unzip \
    p7zip-full \
    zstd

echo ""
echo "Done! All default VoidDream runtime dependencies are installed."
echo ""
echo "Optional — install a Nerd Font for the nerdfont icon set:"
echo "  mkdir -p ~/.local/share/fonts"
echo "  curl -fLo ~/.local/share/fonts/FiraCodeNerdFont-Regular.ttf \\"
echo "    https://github.com/ryanoasis/nerd-fonts/raw/HEAD/patched-fonts/FiraCode/Regular/FiraCodeNerdFont-Regular.ttf"
echo "  fc-cache -fv"
