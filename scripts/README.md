# VoidDream — Scripts

Helper scripts for installing VoidDream's required runtime tools, plus a
reference list of optional apps VoidDream can use as openers.

---

## Required vs optional

VoidDream is intentionally self-contained — every "opener" (image viewer,
video/audio player, document viewer, editor, terminal) has a fallback
chain in the code, ending in `xdg-open` or a custom command you set
yourself in Settings. None of them are required for VoidDream to build or
run; they're just the app's suggested defaults, and you're free to pick
whatever you already have installed instead.

Two tools are the exception — VoidDream shells out to them directly, and
visibly degrades without them rather than falling back to something else:

| Tool | Why it's required | What breaks without it |
|------|--------------------|--------------------------|
| `ffmpeg` | Generates video thumbnails via an external process | Preview pane shows "ffmpeg not available" instead of a thumbnail |
| `unrar` | `.rar` extraction shells out to it directly (no pure-Rust `.rar` support) | Extracting a `.rar` fails with a clear "unrar" error instead of succeeding |

Everything else — `.zip`, `.tar`, `.gz`, `.bz2`, `.xz`, `.zst` — is handled
by Rust crates linked directly into the VoidDream binary (no external
`unzip`/`p7zip`/`zstd` CLI is ever invoked), so those don't need to be
installed separately. `.7z` files are recognized by VoidDream but not
currently extractable by any path, external or built-in.

`chafa` and `glib` are real dependencies too, but at **build time**, not
runtime — VoidDream links against them to render images in the terminal.
They're not something you install yourself; they come in automatically as
part of installing the VoidDream package itself (declared in the
Arch/xbps/deb/rpm packaging, or pulled in by `cargo build` via
`ratatui-image` if building from source).

---

## Usage

Run the script for your distro from the repo root — this installs just
`ffmpeg` and `unrar`:

### Arch / Artix
```bash
bash scripts/install-deps-arch.sh
```

### Debian / Ubuntu
```bash
bash scripts/install-deps-debian.sh
```

### Fedora / RHEL
```bash
bash scripts/install-deps-fedora.sh
```
> RPM Fusion is enabled automatically if not already present — required for `unrar` and full (non-crippled) `ffmpeg` on Fedora.

### Void Linux
```bash
bash scripts/install-deps-void.sh
```

Each script also prints a reference list of optional opener apps and how
to install them for that distro — nothing beyond `ffmpeg`/`unrar` is
installed automatically. Pick what you want, install it yourself, and set
it in VoidDream's Settings if it isn't picked up automatically.

---

## Optional openers (reference)

| Category | Options |
|----------|---------|
| Image viewer | `mirage`, `feh`, `nsxiv`, `eog`, `gwenview`, `imv`, `gimp` |
| Video / audio | `mpv`, `vlc`, `celluloid`, `totem`, `rhythmbox`, `audacious` |
| Document viewer | `libreoffice`, `okular`, `evince`, `zathura` |
| Editor | `nvim`/`neovim`, `vim`, `nano`, `helix`, `emacs`, `micro` |
| Terminal | `kitty`, `alacritty`, `foot`, `wezterm`, `gnome-terminal`, `konsole`, `xterm` |

`mirage` (the image viewer) is AUR-only on Arch (`yay -S mirage` /
`paru -S mirage`); check your distro's actual package name before
installing elsewhere — a Matrix chat client shares the same name in some
repos.

A Nerd Font (for the nerdfont icon set) is also optional — each
distro script prints install instructions for FiraCode Nerd Font at the
end.
