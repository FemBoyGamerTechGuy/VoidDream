<div align="center">

# VoidDream

**A dreamy void-themed TUI file manager built with Rust and Ratatui.**

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blueviolet?style=flat-square)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Rust version](https://img.shields.io/badge/rust-%3E%3D1.85-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Version](https://img.shields.io/badge/version-0.1.5-blueviolet?style=flat-square)](CHANGELOG.md)
[![Status: Frozen](https://img.shields.io/badge/status-frozen%20%E2%9D%84%EF%B8%8F-blue?style=flat-square)](CHANGELOG.md#what-does-code-freeze-mean)
[![Part of Faded Dream](https://img.shields.io/badge/part%20of-Faded%20Dream-purple?style=flat-square)](https://github.com/FemBoyGamerTechGuy/Faded-Dream-dotfiles)

</div>

---

> ❄️ **Code freeze** — VoidDream is feature-complete as of v0.1.5. Bug fixes will still be released. [What does this mean?](#code-freeze)

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
| 🖼️ | **Image & video preview** — including RAW, HEIC, HDR, EXR and more via ffmpeg fallback |
| 🕐 | **Live clock** in tab bar with local timezone, toggleable file date/time column |
| 🎨 | **21 built-in themes** + community theme support |
| 🔤 | **Nerd Font, Emoji, Minimal and None** icon sets |
| ⌨️ | **Fully configurable keybinds** |
| 📂 | **Configurable file openers** per file type |
| 📦 | **Built-in archive extraction** for `.rar`, `.zip`, `.tar.*`, `.7z` and more |
| 📁 | **Folder size display** — async, non-blocking, matches file manager readings |
| 🖱️ | **Open-with menu** (`k`) — pick any app to open a file, or type a custom command |
| 🌐 | **HTML support** — opens in configured browser, configurable separately |
| ⚙️ | **Settings UI** with live apply |

---

## Installation

See **[packaging/README.md](packaging/README.md)**.

### Runtime apps

See **[scripts/README.md](scripts/README.md)**.

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
| `opener_browser` | *(auto-detected)* | Browser for HTML files |
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
| `f` | New file | `k` | Open with… |
| `m` | New directory | `q` / `Esc` | Quit |

---

## Theming

Themes live in `~/.local/share/VoidDream/themes/` as JSON files and are loaded automatically on launch. Drop any `.json` file there and it will appear in the Settings theme picker instantly.

**21 built-in themes** including Catppuccin (all four flavours), Dracula, Tokyo Night, Nord, Gruvbox, Rosé Pine, Everforest, Kanagawa and more.

For the full theme JSON format and icon reference, see [THEMING.md](THEMING.md).

---

## Project Structure

```
VoidDream/
├── src/
│   ├── main.rs              # Entry point
│   ├── config.rs            # Theme, IconData, Config, SettingsState
│   ├── types.rs             # FileKind, InputMode, Tab, file-type lists, helpers
│   ├── app.rs               # App struct and all logic
│   ├── ui.rs                # All TUI drawing functions
│   └── keys.rs              # Keyboard input handlers
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
├── themes/
│   └── *.json               # 21 built-in themes
├── scripts/
│   ├── README.md
│   ├── install-deps-arch.sh
│   ├── install-deps-debian.sh
│   └── install-deps-fedora.sh
├── .gitignore
├── CHANGELOG.md
├── CONTRIBUTING.md
├── Cargo.toml
├── LICENSE
├── README.md
└── THEMING.md
```

---

## Code Freeze

As of **v0.1.5**, VoidDream is feature-complete and has entered **maintenance mode**.

This means:
- 🐛 **Bug fixes** will always be released when issues are found
- 🔧 **Dependency updates and compatibility fixes** are normal and expected
- 🌱 **New features may still appear**, but very slowly and only when they genuinely make sense — not to hit a roadmap or fill a changelog
- 🧊 The project will not be actively developed the way it was; it grows when it grows

This is not abandonment. VoidDream has reached a point where it does what it was built to do, and does it well. The goal going forward is to keep it working correctly — not to keep making it bigger.

---

## Part of Faded Dream

VoidDream is part of the [Faded Dream dotfiles](https://github.com/FemBoyGamerTechGuy/Faded-Dream-dotfiles) ecosystem.

---

## License

Copyright (C) 2026 FemBoyGamerTechGuy

This project is licensed under the **GNU General Public License v3.0**.

You are free to use, modify, and distribute this project, but any derivative work must also be open source under the same license. Nobody can take this project and release it under a different or proprietary license.

See the [LICENSE](LICENSE) file for the full license text.
