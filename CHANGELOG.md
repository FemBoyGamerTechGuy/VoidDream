# Changelog

All notable changes to VoidDream will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [0.1.3] - 2026-03-13

### Added
- `tokyo-night-light` theme

### Changed
- Icon sets (`nerdfont`, `emoji`, `minimal`, `none`) now fully data-driven via JSON files in `icons/`
- NerdFont icon set expanded — added Nix, Zig, Swift, Dart, Julia, Haskell, Erlang, SQL, torrent, patch/diff, Blender, STL, Jupyter and more
- NerdFont `by_name` expanded — added `docker-compose.yml/yaml`, `yarn.lock`, `license`, `flake.nix`, `shell.nix`, `default.nix`, `.env`, `.env.local`, `.env.example`
- NerdFont named directories expanded — added `build`, `dist`, `docs`, `assets`, `data`, `logs`, `config`, `test`, `venv`, `vendor`, `backup`, `migrations`, `cache`, `snap`, `flatpak`
- Emoji icon set `by_name` now includes shell config files (`.bashrc`, `.zshrc`, etc.)
- Packaging now correctly bundles all themes and icon sets into `/usr/share/VoidDream/` for `.deb`, `.rpm` and Arch `PKGBUILD`
- Build scripts updated to use `cargo-deb` and `cargo-generate-rpm`

### Removed
- `btop-dark` theme
- `btop-default` theme

---

## [0.1.2] - 2026-03-12

### Added
- Live clock in tab bar showing HH:MM:SS and date, updates every tick
- File date/time column in the files pane
- Both clock and file mtime display are toggleable in Settings → Behaviour
- Cycle tab keybind is now configurable (defaults to `Tab`)
- Built-in per-format archive extraction — no external meta-tool required
  - `.rar` → `unrar`, `.zip` → `unzip`, `.7z` → `7z`
  - `.tar.gz/tgz`, `.tar.bz2/tbz2`, `.tar.xz`, `.tar.zst`, `.tar` → `tar` with correct flags
  - `.gz`, `.bz2`, `.xz`, `.zst` → native decompressors
- Archive extractor commands visible in Settings → Openers as read-only reference rows
- Keybinds section in Settings now lists every key in the app including fixed non-configurable keys
- Fixed keys shown dimmed with `(fixed)` label, pressing Enter on them does nothing
- Help bar simplified to show only `?:help`

### Fixed
- Archive extraction output no longer bleeds into the TUI (all stdio suppressed)
- Removed unused `key_matches` closure, `format_mtime` function, `which()` function, and `ch` variable causing compiler warnings

---

## [0.1.1] - 2026-03-10

### Fixed
- Video thumbnail preview now renders correctly and centered in the preview pane
- Fixed large file reading causing the TUI to freeze for very large files (preview now skipped above 512 MB)
- Fixed `Theme` type visibility warning (`private_interfaces`)

---

## [0.1.0] - 2026-03-10

### Added
- 3-pane layout — parent / files / preview
- Multi-tab support
- Fuzzy search
- Image & video preview via `ratatui-image` and `ffmpeg`
- 23 built-in themes including Catppuccin, Dracula, Tokyo Night, Nord, Gruvbox and more
- Community theme support — load custom themes from `~/.local/share/VoidDream/themes/`
- Nerd Font, Emoji, Minimal and None icon sets
- Fully configurable keybinds via settings UI (`S`)
- Configurable file openers per type (image, video, audio, doc, editor, archive, terminal)
- Settings UI with live apply
- Auto-generated config at `~/.config/VoidDream/config.json` on first launch
- GPL-3.0-or-later license
