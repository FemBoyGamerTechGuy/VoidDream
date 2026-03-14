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

See **[scripts/README.md](../scripts/README.md)**.
