# NovaDream — Packaging

## Supported formats

| Format | Distros |
|--------|---------|
| `.deb` | Debian, Ubuntu, Linux Mint, Pop!_OS, elementary OS, and derivatives |
| `.rpm` | Fedora, RHEL, CentOS Stream, AlmaLinux, Rocky Linux, openSUSE, and derivatives |

---

## Prerequisites

### Build dependencies

**Debian/Ubuntu:**
```bash
sudo apt install build-essential pkg-config libgtk-4-dev libglib2.0-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install gcc pkg-config gtk4-devel glib2-devel
```

### Rust toolchain
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## Icons

Before building packages, add PNG icons at the following sizes:

```
assets/icons/hicolor/16x16/apps/io.github.FemBoyGamerTechGuy.NovaDream.png
assets/icons/hicolor/32x32/apps/io.github.FemBoyGamerTechGuy.NovaDream.png
assets/icons/hicolor/48x48/apps/io.github.FemBoyGamerTechGuy.NovaDream.png
assets/icons/hicolor/64x64/apps/io.github.FemBoyGamerTechGuy.NovaDream.png
assets/icons/hicolor/128x128/apps/io.github.FemBoyGamerTechGuy.NovaDream.png
assets/icons/hicolor/256x256/apps/io.github.FemBoyGamerTechGuy.NovaDream.png
```

If you have a single source image, resize with ImageMagick:
```bash
for s in 16 32 48 64 128 256; do
    convert icon.png -resize ${s}x${s} \
        assets/icons/hicolor/${s}x${s}/apps/io.github.FemBoyGamerTechGuy.NovaDream.png
done
```

---

## Building

### Both formats at once
```bash
chmod +x packaging/build-packages.sh
./packaging/build-packages.sh
```

### Debian/Ubuntu only
```bash
./packaging/build-packages.sh --deb-only
# or directly:
chmod +x packaging/build-deb.sh
./packaging/build-deb.sh
```

### Fedora/RHEL only
```bash
./packaging/build-packages.sh --rpm-only
# or directly:
chmod +x packaging/build-rpm.sh
./packaging/build-rpm.sh
```

---

## Installing

### Debian/Ubuntu
```bash
sudo dpkg -i target/debian/NovaDream_*.deb
sudo apt-get install -f   # install any missing dependencies
```

### Fedora
```bash
sudo dnf install target/generate-rpm/NovaDream-*.rpm
```

### RHEL/CentOS/AlmaLinux/Rocky
```bash
sudo rpm -i target/generate-rpm/NovaDream-*.rpm
```

### openSUSE
```bash
sudo zypper install target/generate-rpm/NovaDream-*.rpm
```

---

## Runtime dependencies

| Package (Debian) | Package (Fedora) | Purpose |
|------------------|------------------|---------|
| `libgtk-4-1` | `gtk4` | UI toolkit |
| `libglib2.0-0` | `glib2` | GLib runtime |
| `libayatana-appindicator3-1` | `libayatana-appindicator` | System tray |

Wine/Proton runners are **not** packaged — users download them separately
(e.g. Proton-GE from GitHub, or system Wine via `apt install wine` / `dnf install wine`).
