# VoidDream — Packaging

## Supported formats

| Format | Tool | Distros |
|--------|------|---------|
| Arch/Artix | `PKGBUILD` + `makepkg` | Arch, Artix, Manjaro, EndeavourOS, and derivatives |
| `.deb` | `cargo-deb` | Debian, Ubuntu, Linux Mint, Pop!_OS, and derivatives |
| `.rpm` | `cargo-generate-rpm` | Fedora, RHEL, CentOS Stream, AlmaLinux, Rocky, openSUSE |

---

## Prerequisites

### Rust toolchain

**Arch/Artix:**
```bash
sudo pacman -S rust
```

**Debian/Ubuntu:**
```bash
sudo apt install rustc cargo gcc
```

**Fedora/RHEL:**
```bash
sudo dnf install rust cargo gcc
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
sudo dpkg -i target/debian/VoidDream_*.deb
```

> `cargo-deb` will be installed automatically if not present.

### Fedora / RHEL
```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
chmod +x packaging/build-rpm.sh
./packaging/build-rpm.sh
sudo dnf install ./target/generate-rpm/VoidDream-*.rpm
```

> `cargo-generate-rpm` will be installed automatically if not present.

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
