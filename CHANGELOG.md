# Changelog

All notable changes to VoidDream will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [0.1.5] - 2026-03-21

> ⚠️ **Code freeze** — VoidDream has entered maintenance mode as of this release.
> The project has reached maturity. Bug fixes and compatibility updates will always be addressed. New features may still appear, but rarely and only when they genuinely fit — the pace of development has slowed significantly by design.
> See [What does code freeze mean?](#what-does-code-freeze-mean) below.

### Added
- **Open-with context menu** (`k`) — press `k` on any file to pick which app opens it, with all configured openers listed and the default for that file type shown first
- **Custom command input** — a "Custom command…" option at the bottom of the open-with menu lets you type any command directly; the file path is appended automatically
- **Folder size in preview pane** — hovering a directory shows its total size and item count at the bottom of the third column, calculated asynchronously so the UI never freezes
- **Disk usage matches file managers** — folder size now uses SI base-10 units (GB = 10⁹ bytes) to match what Nemo, Nautilus and other file managers display
- **Extended image format support** — preview now covers RAW camera formats (ARW, CR2, CR3, NEF, ORF, RAF, RW2, DNG and more), HEIC, HEIF, JXL, HDR, EXR, QOI, TGA, DDS, PSD, XCF; formats the `image` crate cannot decode fall back to an ffmpeg thumbnail automatically
- **Extended audio format support** — APE, MKA, AIFF, ALAC, DSD, MIDI, AMR, TTA, WavPack, AC3, DTS, TrueHD and more
- **Extended video format support** — TS, MTS, M2TS, VOB, OGV, 3GP, RM, RMVB, DIVX, MXF, AMV and more
- **HTML file support** — HTML/HTM/XHTML files show a preview with size and open in the configured browser on Enter; a dedicated `Browser (HTML)` opener is now configurable in Settings → Openers
- **Local timezone clock** — the tab bar clock now shows the correct local time instead of UTC
- **Extraction ESC kills the process** — pressing Esc during archive extraction now sends SIGTERM to the child process so it actually stops instead of continuing in the background
- **File list refresh on extraction cancel** — cancelling an extraction now immediately refreshes the file list so partially extracted files are visible without navigating away
- **Codebase split into modules** — `main.rs` has been split into `config.rs`, `types.rs`, `app.rs`, `ui.rs`, and `keys.rs` for maintainability

### Fixed
- **RAR total size was wildly wrong** — switched from `unrar l` summary line parsing (which returned ~2× the real size) to `unrar lt` per-file size accumulation for accuracy
- **RAR extraction progress showed KB instead of GB** — progress was reading compressed chunk sizes from output lines instead of tracking file count against total; now scales proportionally
- **Folder size cache not cleared after extraction** — navigating back to the extraction destination no longer shows the stale pre-extraction folder size
- **`opener_browser`, `opener_jar`, `opener_terminal` not saved** — these three openers were editable in Settings but changes were silently discarded; now correctly written to config
- **`s` keybind conflict removed** — `s` was listed as unbound but was silently intercepted in settings mode; the open-with menu uses `k` to avoid any ambiguity

### What does code freeze mean?

VoidDream has reached a point where it does what it was built to do. "Code freeze" here doesn't mean the project is dead or locked — it means the pace of development has slowed down significantly by design. Going forward:

- 🐛 **Bug fixes** will always be released when issues are found
- 🔧 **Dependency updates and compatibility fixes** are normal and expected
- 🌱 **New features may still appear**, but very slowly — only when something genuinely makes sense to add, not to hit a roadmap or fill a changelog
- 🧊 The project will not be actively developed the way it was during 0.1.x; it grows when it grows

This is not abandonment. VoidDream works well and will continue to. It just won't keep expanding indefinitely.

---

## [0.1.4] - 2026-03-14

### Fixed
- GUI openers (image, video, audio, doc) no longer bleed into the TUI
- File list cursor no longer goes out of bounds when scrolling up
- Thumbnail fetching is now non-blocking — image loading moved to a background thread
- Added 150ms debounce to image and video preview to avoid loading every file scrolled past
- Video thumbnails now use a hash-based temp filename to avoid collisions between same-named files in different directories
- Temp thumbnail file is now deleted immediately after being loaded into memory

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
