<div align="center">

# VoidDream

**A dreamy void-themed TUI file manager built with Rust and Ratatui.**

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blueviolet?style=flat-square)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Rust version](https://img.shields.io/badge/rust-%3E%3D1.85-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Part of Faded Dream](https://img.shields.io/badge/part%20of-Faded%20Dream-purple?style=flat-square)](https://github.com/FemBoyGamerTechGuy/Faded-Dream-dotfiles)

</div>

---

## Overview

VoidDream is a fast, keyboard-driven file manager for the terminal. It features a classic three-pane layout, live file previews, fuzzy search, multi-tab navigation, and a fully themeable interface — all configurable without touching a config file manually.

---

## Features

| | Feature |
|---|---|
| 🗂️ | **3-pane layout** — parent / files / preview |
| 🗃️ | **Multi-tab support** |
| 🔍 | **Fuzzy search** with live streaming results |
| 🖼️ | **Image & video preview** |
| 🎨 | **23 built-in themes** + community theme support |
| 🔤 | **Nerd Font, Emoji, Minimal and None** icon sets |
| ⌨️ | **Fully configurable keybinds** |
| 📂 | **Configurable file openers** per file type |
| ⚙️ | **Settings UI** with live apply |
| 🌐 | **External themes** loaded from `~/.local/share/VoidDream/themes/` |

---

## Installation

### Arch / Artix

```bash
# Install Rust if you don't have it
sudo pacman -S rust

# Clone and build
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
cargo build --release
sudo install -Dm755 target/release/VoidDream /usr/bin/VoidDream
```

**Optional runtime dependencies:**

```bash
sudo pacman -S ffmpeg mpv mirage neovim libreoffice-fresh
yay -S ouch   # or: paru -S ouch
```

---

### Fedora

**Install build dependencies first:**

```bash
sudo dnf install rust cargo pkg-config chafa chafa-devel
```

> [!WARNING]
> There may be one additional missing build dependency — this list is not yet complete.
> If you hit a compile error, please [open an issue](https://github.com/FemBoyGamerTechGuy/VoidDream/issues) with the error output so it can be documented here.

```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
cargo build --release
sudo install -Dm755 target/release/VoidDream /usr/bin/VoidDream
```

**Optional runtime dependencies:**

```bash
sudo dnf install ffmpeg mpv neovim libreoffice
```

---

### Runtime dependencies

Requires **Rust ≥ 1.85** to build.

| Package       | Purpose                  | Required |
|---------------|--------------------------|----------|
| `ffmpeg`      | Video thumbnails         | Optional |
| `mpv`         | Video / audio playback   | Optional |
| `mirage`      | Image viewer             | Optional |
| `nvim`        | Text editor              | Optional |
| `libreoffice` | Document opener          | Optional |
| `ouch`        | Archive extraction       | Optional |

---

## Configuration

Config is stored at `~/.config/VoidDream/config.json` and is created automatically on first launch with sane defaults.

| Key | Default | Description |
|-----|---------|-------------|
| `theme` | `catppuccin-macchiato` | Active theme |
| `icon_set` | `nerdfont` | `nerdfont` / `emoji` / `minimal` / `none` |
| `show_hidden` | `false` | Show hidden files |
| `date_format` | `%Y-%m-%d` | Date format in file list |
| `opener_image` | `mirage` | Image opener |
| `opener_video` | `mpv` | Video opener |
| `opener_audio` | `mpv` | Audio opener |
| `opener_doc` | `libreoffice` | Document opener |
| `opener_editor` | `nvim` | Text editor |
| `opener_archive` | `ouch decompress` | Archive opener |
| `opener_terminal` | ` ` | Terminal opener |

---

## Keybinds

All keybinds are configurable from the settings UI — press `S` to open it.

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `↑` / `↓` | Navigate | `Space` | Select file |
| `→` / `Enter` | Open / enter dir | `Ctrl+a` | Select all |
| `←` / `Backspace` | Go up | `/` | Fuzzy search |
| `c` | Copy | `.` | Toggle hidden files |
| `u` | Cut | `Tab` | Next tab |
| `p` | Paste | `t` | New tab |
| `d` | Delete | `x` | Close tab |
| `r` | Rename | `S` | Settings |
| `f` | New file | `q` | Quit |
| `m` | New directory | | |

---

## Theming

Themes live in `~/.local/share/VoidDream/themes/` as JSON files and are loaded automatically on launch. Drop any `.json` file there and it will appear in the Settings theme picker instantly.

**23 built-in themes** including Catppuccin (all four flavours), Dracula, Tokyo Night, Nord, Gruvbox, Rosé Pine, Everforest, Kanagawa and more.

For the full theme JSON format and icon reference, see [THEMING.md](THEMING.md).

---

## Project Structure

```
VoidDream/
├── src/                  # Rust source code
├── themes/               # Built-in theme JSON files
├── icons/                # Icon set JSON files
├── CHANGELOG.md          # Version history
├── CONTRIBUTING.md       # Contributor License Agreement
├── THEMING.md            # Theme & icon set API for users
├── Cargo.toml            # Rust package manifest
├── LICENSE               # GPL-3.0-or-later
└── README.md
```

---

## Part of Faded Dream

VoidDream is part of the [Faded Dream dotfiles](https://github.com/FemBoyGamerTechGuy/Faded-Dream-dotfiles) ecosystem.

---

## License

Copyright (C) 2026 FemBoyGamerTechGuy

This project is licensed under the **GNU General Public License v3.0**.

You are free to use, modify, and distribute this project, but any derivative work must also be open source under the same license. Nobody can take this project and release it under a different or proprietary license.

See the [LICENSE](LICENSE) file for the full license text.
