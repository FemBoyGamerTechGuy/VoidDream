#!/usr/bin/env bash
# install-deps-arch.sh — Install default runtime apps for VoidDream on Arch/Artix
set -euo pipefail

echo "==> Installing VoidDream runtime dependencies..."
echo "    (these are the default apps used by VoidDream's openers)"
echo ""

sudo pacman -S --needed --noconfirm \
    mpv \
    libreoffice-fresh \
    neovim \
    jdk-openjdk \
    kitty \
    ffmpeg \
    chafa \
    unrar \
    unzip \
    p7zip \
    zstd \
    noto-fonts-emoji \
    ttf-firacode-nerd

echo ""
echo "==> Installing mirage from AUR..."
if command -v yay &>/dev/null; then
    yay -S --needed --noconfirm mirage
elif command -v paru &>/dev/null; then
    paru -S --needed --noconfirm mirage
else
    echo "  No AUR helper found (yay or paru). Install mirage manually:"
    echo "  yay -S mirage  or  paru -S mirage"
fi

echo ""
echo "FiraCode Nerd Font and Noto Emoji are also installed."
echo "Set your terminal font to 'FiraCode Nerd Font' to enable the nerdfont icon set."
