# VoidDream — Packaging

## Supported formats

| Format | Tool | Distros |
|--------|------|---------|
| Arch/Artix | `PKGBUILD` + `makepkg` | Arch, Artix, Manjaro, EndeavourOS, and derivatives |
| `.deb` | `cargo-deb` | Debian, Ubuntu, Linux Mint, Pop!_OS, and derivatives |
| `.rpm` | `cargo-generate-rpm` | Fedora, RHEL, CentOS Stream, AlmaLinux, Rocky, openSUSE |

---

## Prerequisites

> **Arch/Artix users:** No manual setup needed — `makepkg -si` handles everything automatically including all build dependencies.

**Debian/Ubuntu:**
```bash
sudo apt install rustc cargo gcc chafa libchafa-dev pkg-config libglib2.0-dev dpkg-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install rust cargo gcc chafa chafa-devel pkgconf-pkg-config
```

---

## Building

### Arch / Artix
```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream/packaging
makepkg -si
```

> `makepkg -si` clones the repo, installs all dependencies, builds the binary,
> and installs the package via pacman in one step. Nothing else needed.

### Debian / Ubuntu
```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
chmod +x packaging/build-deb.sh
./packaging/build-deb.sh
sudo dpkg -i target/debian/voiddream_*.deb
```

> `cargo-deb` will be installed automatically if not present.
> The script also writes `/etc/profile.d/cargo.sh` so `~/.cargo/bin` is in PATH
> for all users immediately — no reboot or re-login needed.

### Fedora / RHEL
```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
chmod +x packaging/build-rpm.sh
./packaging/build-rpm.sh
sudo dnf install ./target/generate-rpm/VoidDream-*.rpm
```

> `cargo-generate-rpm` will be installed automatically if not present.
> The script also writes `/etc/profile.d/cargo.sh` so `~/.cargo/bin` is in PATH
> for all users immediately — no reboot or re-login needed.

---

## Fonts

VoidDream uses two icon sets that require specific fonts to render correctly.

### Nerd Font (for `nerdfont` icon set)

Any [Nerd Font](https://www.nerdfonts.com/) patched font works. The recommended font is **FiraCode Nerd Font**:

**Arch/Artix:**
```bash
sudo pacman -S ttf-firacode-nerd
```

**Debian/Ubuntu:**
```bash
# Not in apt — install manually:
mkdir -p ~/.local/share/fonts
curl -fLo ~/.local/share/fonts/FiraCodeNerdFont-Regular.ttf \
    https://github.com/ryanoasis/nerd-fonts/raw/HEAD/patched-fonts/FiraCode/Regular/FiraCodeNerdFont-Regular.ttf
fc-cache -fv
```

**Fedora/RHEL:**
```bash
# Not in dnf — install manually:
mkdir -p ~/.local/share/fonts
curl -fLo ~/.local/share/fonts/FiraCodeNerdFont-Regular.ttf \
    https://github.com/ryanoasis/nerd-fonts/raw/HEAD/patched-fonts/FiraCode/Regular/FiraCodeNerdFont-Regular.ttf
fc-cache -fv
```

Then set your terminal to use **FiraCode Nerd Font**.

---

### Emoji font (for `emoji` icon set)

**Arch/Artix:**
```bash
sudo pacman -S noto-fonts-emoji
```

**Debian/Ubuntu:**
```bash
sudo apt install fonts-noto-color-emoji
```

**Fedora/RHEL:**
```bash
sudo dnf install google-noto-emoji-color-fonts
```

---

## What gets installed

All three package formats install the same files:

| File | Destination |
|------|-------------|
| `VoidDream` binary | `/usr/bin/VoidDream` |
| Theme JSON files | `/usr/share/VoidDream/themes/` |
| Icon set JSON files | `/usr/share/VoidDream/icons/` |

---

## Uninstalling

```bash
# Arch / Artix
sudo pacman -R voiddream

# Debian / Ubuntu
sudo apt remove voiddream

# Fedora / RHEL
sudo dnf remove VoidDream
```

---

## Runtime dependencies

All optional — the app works without them but loses certain features.

| Package | Purpose |
|---------|---------|
| `ffmpeg` | Video thumbnails in preview pane |
| `chafa` | Image preview fallback |
| `mpv` | Video and audio playback |
| `neovim` | Text editor integration |
| `unrar` | `.rar` extraction |
| `unzip` | `.zip` extraction |
| `p7zip` / `p7zip-full` | `.7z` extraction |
| `zstd` | `.zst` / `.tar.zst` extraction |

> `tar`, `gzip`, `bzip2` and `xz` are part of the base system and always present.
