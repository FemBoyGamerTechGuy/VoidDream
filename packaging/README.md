# VoidDream — Packaging

## Supported formats

| Format | Script | Distros |
|--------|--------|---------|
| Arch/Artix | `PKGBUILD` | Arch, Artix, Manjaro, EndeavourOS, and derivatives |
| `.deb` | `build-deb.sh` | Debian, Ubuntu, Linux Mint, Pop!_OS, and derivatives |
| `.rpm` | `build-rpm.sh` | Fedora, RHEL, CentOS Stream, AlmaLinux, Rocky, openSUSE |

---

## Prerequisites

### Rust toolchain
```bash
# Arch/Artix
sudo pacman -S rust

# Debian/Ubuntu
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Fedora
sudo dnf install rust cargo
```

### Package build tools

**Debian/Ubuntu** (for `.deb`):
```bash
sudo apt install dpkg-dev
```

**Fedora/RHEL** (for `.rpm`):
```bash
sudo dnf install rpm-build
```

---

## Building

### Arch / Artix
```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream/packaging
makepkg -si
```

### Debian / Ubuntu
```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
chmod +x packaging/build-deb.sh
./packaging/build-deb.sh
sudo dpkg -i voiddream_*.deb
```

### Fedora / RHEL
```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
chmod +x packaging/build-rpm.sh
./packaging/build-rpm.sh
sudo dnf install ./voiddream-*.rpm
```

---

## What gets installed

All three package formats install the same files:

| File | Destination |
|------|-------------|
| `VoidDream` binary | `/usr/bin/VoidDream` |
| Theme JSON files | `/usr/share/VoidDream/themes/` |
| Icon set JSON files | `/usr/share/VoidDream/icons/` |
| Desktop entry | `/usr/share/applications/` |
| License | `/usr/share/licenses/voiddream/` |

---

## Uninstalling

```bash
# Arch / Artix
sudo pacman -R voiddream

# Debian / Ubuntu
sudo apt remove voiddream

# Fedora / RHEL
sudo dnf remove voiddream
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
| `p7zip` | `.7z` extraction |
| `zstd` | `.zst` / `.tar.zst` extraction |

> `tar`, `gzip`, `bzip2` and `xz` are part of the base system and always present.
