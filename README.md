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

## Previews

<div align="center">

<img src="Previews/2026-03-12-220027_hyprshot.png" width="49%"/> <img src="Previews/2026-03-12-220040_hyprshot.png" width="49%"/>
<img src="Previews/2026-03-12-220053_hyprshot.png" width="49%"/> <img src="Previews/2026-03-12-220059_hyprshot.png" width="49%"/>

</div>

---

## Features

| | Feature |
|---|---|
| 🗂️ | **3-pane layout** — parent / files / preview |
| 🗃️ | **Multi-tab support** |
| 🔍 | **Fuzzy search** with live streaming results |
| 🖼️ | **Image & video preview** |
| 🕐 | **Live clock** in tab bar with toggleable file date/time column |
| 🎨 | **23 built-in themes** + community theme support |
| 🔤 | **Nerd Font, Emoji, Minimal and None** icon sets |
| ⌨️ | **Fully configurable keybinds** |
| 📂 | **Configurable file openers** per file type |
| 📦 | **Built-in archive extraction** for `.rar`, `.zip`, `.tar.*`, `.7z` and more |
| ⚙️ | **Settings UI** with live apply |
| 🌐 | **External themes** loaded from `~/.local/share/VoidDream/themes/` |

---

## Installation

See **[packaging/README.md](packaging/README.md)**.

---

## Configuration

Config is stored at `~/.config/VoidDream/config.json` and is created automatically on first launch with sane defaults.

| Key | Default | Description |
|-----|---------|-------------|
| `theme` | `catppuccin-macchiato` | Active theme |
| `icon_set` | `nerdfont` | `nerdfont` / `emoji` / `minimal` / `none` |
| `show_hidden` | `true` | Show hidden files |
| `date_format` | `%d/%m/%Y %H:%M` | Date format in file list |
| `show_clock` | `true` | Live clock in tab bar |
| `show_file_mtime` | `true` | Date/time column in file list |
| `opener_image` | `mirage` | Image opener |
| `opener_video` | `mpv` | Video opener |
| `opener_audio` | `mpv` | Audio opener |
| `opener_doc` | `libreoffice` | Document opener |
| `opener_editor` | `nvim` | Text editor |

---

## Keybinds

All configurable keybinds can be changed from the settings UI — press `:` to open it.

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `↑` / `↓` | Navigate | `Space` | Select / deselect |
| `→` / `Enter` | Open / enter dir | `Ctrl+a` / `A` | Select all |
| `←` / `Backspace` | Go up | `Ctrl+r` | Deselect all |
| `Page Up/Down` | Jump 10 entries | `/` | Fuzzy search |
| `Home` / `End` | First / last entry | `.` | Toggle hidden files |
| `c` | Copy | `Tab` | Cycle tabs |
| `u` | Cut | `t` | New tab |
| `p` | Paste | `x` | Close tab |
| `d` | Delete | `:` | Settings |
| `r` | Rename | `?` | Help |
| `f` | New file | `q` / `Esc` | Quit |
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
├── src/
│   └── main.rs
├── assets/
│   └── desktop/
│       └── io.github.FemBoyGamerTechGuy.VoidDream.desktop
├── icons/
│   ├── emoji.json
│   ├── minimal.json
│   ├── nerdfont.json
│   └── none.json
├── packaging/
│   ├── README.md
│   ├── PKGBUILD
│   ├── build-deb.sh
│   ├── build-rpm.sh
│   └── build-packages.sh
├── Previews/
│   ├── 2026-03-12-220027_hyprshot.png
│   ├── 2026-03-12-220040_hyprshot.png
│   ├── 2026-03-12-220053_hyprshot.png
│   └── 2026-03-12-220059_hyprshot.png
├── themes/
│   ├── ayu-dark.json
│   ├── btop-dark.json
│   ├── btop-default.json
│   ├── catppuccin-frappe.json
│   ├── catppuccin-latte.json
│   ├── catppuccin-macchiato.json
│   ├── catppuccin-mocha.json
│   ├── dracula.json
│   ├── everforest-dark.json
│   ├── gruvbox-dark.json
│   ├── gruvbox-light.json
│   ├── kanagawa.json
│   ├── material-ocean.json
│   ├── nord.json
│   ├── onedark.json
│   ├── rose-pine-dawn.json
│   ├── rose-pine-moon.json
│   ├── rose-pine.json
│   ├── solarized-dark.json
│   ├── solarized-light.json
│   ├── tokyo-night-light.json
│   ├── tokyo-night-storm.json
│   └── tokyo-night.json
├── .gitignore
├── CHANGELOG.md
├── CONTRIBUTING.md
├── Cargo.toml
├── LICENSE
├── Makefile
├── README.md
└── THEMING.md
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
