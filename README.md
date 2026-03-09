# VoidDream
> A dreamy void-themed TUI file manager built with Rust and Ratatui

![License](https://img.shields.io/badge/license-GPL--3.0-blue)
![Rust](https://img.shields.io/badge/built%20with-Rust-orange)
![Part of](https://img.shields.io/badge/part%20of-Faded%20Dream-purple)

---

## Features

- 3-pane layout — parent / files / preview
- Multi-tab support
- Fuzzy search
- Image & video preview
- 23 built-in themes + community theme support
- Nerd Font, Emoji, Minimal and None icon sets
- Fully configurable keybinds
- Configurable file openers per type
- Settings UI with live apply
- External themes loaded from `~/.local/share/VoidDream/themes/`

---

## Installation

### Arch / Artix (from source)

```bash
git clone https://github.com/FemBoyGamerTechGuy/VoidDream
cd VoidDream
cargo build --release
sudo install -Dm755 target/release/VoidDream /usr/bin/VoidDream
```

### Dependencies

```
rust >= 1.85
```

Optional runtime dependencies for file previews and openers:

```
ffmpeg       # video thumbnails
mpv          # video / audio playback
mirage       # image viewer
nvim         # editor
libreoffice  # documents
ouch         # archives
```

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

All keybinds are configurable from the settings UI (`S`).

| Key | Action |
|-----|--------|
| `q` | Quit |
| `c` | Copy |
| `u` | Cut |
| `p` | Paste |
| `d` | Delete |
| `r` | Rename |
| `f` | New file |
| `m` | New directory |
| `/` | Search |
| `.` | Toggle hidden files |
| `Tab` | New tab |
| `x` | Close tab |
| `Space` | Select |
| `Ctrl+a` | Select all |

---

## Themes

Themes live in `~/.local/share/VoidDream/themes/` as JSON files and are loaded automatically. The directory is created on first launch.

23 themes are built in including Catppuccin, Dracula, Tokyo Night, Nord, Gruvbox and more.

---

## Part of Faded Dream

VoidDream is part of the [Faded Dream dotfiles](https://github.com/FemBoyGamerTechGuy/Faded-Dream-dotfiles) ecosystem.

---

## License

[GPL-3.0-or-later](LICENSE) © FemBoyGamerTechGuy
