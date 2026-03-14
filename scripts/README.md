# VoidDream — Scripts

Helper scripts for installing the default runtime apps that VoidDream uses to open files.

---

## What gets installed

| App | Purpose | Default for |
|-----|---------|-------------|
| `mirage` | Image viewer | `opener_image` |
| `mpv` | Video and audio player | `opener_video`, `opener_audio` |
| `libreoffice` | Document viewer | `opener_doc` |
| `neovim` | Text editor | `opener_editor` |
| `java` / `jdk` | JAR file launcher | `opener_jar` |
| `kitty` | Terminal emulator | `opener_terminal` |
| `ffmpeg` | Video thumbnail generation | Preview pane |
| `chafa` | Image preview fallback | Preview pane |
| `unrar` | `.rar` extraction | Archive opener |
| `unzip` | `.zip` extraction | Archive opener |
| `p7zip` / `p7zip-full` | `.7z` extraction | Archive opener |
| `zstd` | `.zst` / `.tar.zst` extraction | Archive opener |

All openers can be changed from the Settings UI — these scripts just install the defaults.

---

## Usage

Run the script for your distro from the repo root:

### Arch / Artix
```bash
bash scripts/install-deps-arch.sh
```

> `mirage` is installed from the AUR via `yay` or `paru`. All other packages come from the official repos. `ttf-firacode-nerd` and `noto-fonts-emoji` are also installed for the icon sets.

### Debian / Ubuntu
```bash
bash scripts/install-deps-debian.sh
```

> A Nerd Font install snippet is printed at the end since FiraCode Nerd Font is not in apt.

### Fedora / RHEL
```bash
bash scripts/install-deps-fedora.sh
```

> RPM Fusion is enabled automatically if not already present — required for `unrar` and full `ffmpeg`.
