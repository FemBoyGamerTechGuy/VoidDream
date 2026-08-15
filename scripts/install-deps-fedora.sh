#!/usr/bin/env bash
# install-deps-fedora.sh — Install required runtime tools for VoidDream on Fedora/RHEL
set -euo pipefail

echo "==> Installing VoidDream required runtime tools..."
echo "    (ffmpeg and unrar are not build-time dependencies, but VoidDream"
echo "     visibly degrades without them — video thumbnails and RAR"
echo "     extraction won't work, so these are installed by default.)"
echo ""

# Enable RPM Fusion — needed for unrar and full (non-crippled) ffmpeg
if ! rpm -q rpmfusion-free-release &>/dev/null; then
  echo "==> Enabling RPM Fusion (needed for unrar and full ffmpeg)..."
  sudo dnf install -y \
    "https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm"
fi

sudo dnf install -y \
  ffmpeg \
  unrar

echo ""
echo "Done. ffmpeg and unrar are installed."
echo ""
echo "-------------------------------------------------------------------"
echo "Everything below this line is OPTIONAL. VoidDream doesn't require"
echo "any specific app for opening images, video, audio, documents, or"
echo "for its built-in editor/terminal handoff — every opener category"
echo "has a fallback chain (ending in xdg-open, or a custom command you"
echo "set yourself in VoidDream's Settings). Nothing below is installed"
echo "by this script — install only what you actually want to use, then"
echo "set it as your default opener in VoidDream's Settings if it isn't"
echo "picked up automatically."
echo "-------------------------------------------------------------------"
echo ""
echo "Image viewer   (any one):  mirage, feh, nsxiv, eog, gwenview, imv, gimp"
echo "Video / audio  (any one):  mpv, vlc, celluloid, totem, rhythmbox, audacious"
echo "Document viewer(any one):  libreoffice, okular, evince, zathura"
echo "Editor         (any one):  neovim, vim, nano, helix, emacs"
echo "Terminal       (any one):  kitty, alacritty, foot, wezterm, gnome-terminal, konsole, xterm"
echo ""
echo "Example — install a common desktop-friendly set, if you want one:"
echo "  sudo dnf install -y mirage mpv libreoffice neovim kitty"
echo ""
echo "Optional — install a Nerd Font for the nerdfont icon set:"
echo "  mkdir -p ~/.local/share/fonts"
echo "  curl -fLo ~/.local/share/fonts/FiraCodeNerdFont-Regular.ttf \\"
echo "    https://github.com/ryanoasis/nerd-fonts/raw/HEAD/patched-fonts/FiraCode/Regular/FiraCodeNerdFont-Regular.ttf"
echo "  fc-cache -fv"
