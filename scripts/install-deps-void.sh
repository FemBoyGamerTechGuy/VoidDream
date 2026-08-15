#!/usr/bin/env bash
# install-deps-void.sh — Install required runtime tools for VoidDream on Void Linux
set -euo pipefail

echo "==> Installing VoidDream required runtime tools..."
echo "    (ffmpeg and unrar are not build-time dependencies, but VoidDream"
echo "     visibly degrades without them — video thumbnails and RAR"
echo "     extraction won't work, so these are installed by default.)"
echo ""

sudo xbps-install -Sy \
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
echo "Editor         (any one):  nvim, vim, nano, helix, emacs, micro"
echo "Terminal       (any one):  kitty, alacritty, foot, wezterm, xterm"
echo ""
echo "Example — install a common desktop-friendly set, if you want one:"
echo "  sudo xbps-install -S mpv libreoffice neovim kitty"
echo ""
echo "mirage specifically: check availability first, package naming for"
echo "image-viewer 'mirage' vs the unrelated matrix client of the same"
echo "name can vary by repo — confirm with:"
echo "  xbps-query -Rs mirage"
echo ""
echo "Optional — install a Nerd Font for the nerdfont icon set:"
echo "  mkdir -p ~/.local/share/fonts"
echo "  curl -fLo ~/.local/share/fonts/FiraCodeNerdFont-Regular.ttf \\"
echo "    https://github.com/ryanoasis/nerd-fonts/raw/HEAD/patched-fonts/FiraCode/Regular/FiraCodeNerdFont-Regular.ttf"
echo "  fc-cache -fv"
