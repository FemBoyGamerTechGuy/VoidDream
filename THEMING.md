# VoidDream — Theming & Icons API

This document explains how to create your own themes and icon sets for VoidDream.
No coding required — everything is plain JSON.

---

## Themes

### Where to put theme files

```
~/.local/share/VoidDream/themes/
```

This directory is created automatically on first launch. Drop any `.json` file in
here and VoidDream will pick it up immediately — no restart needed, just open the
Settings UI (`S`) and select your theme from the dropdown.

The **filename stem** becomes the theme name:

```
Fire Aura.json       →  "Fire Aura"
my-custom.json       →  "my-custom"
Void Sunset.json     →  "Void Sunset"
```

Spaces, hyphens, and unicode characters are all valid in filenames.

---

### Theme file format

A theme file is a single JSON object with **13 required color fields**.
All colors must be `"#RRGGBB"` hex strings.

```json
{
  "base":     "#1e1e2e",
  "surface0": "#313244",
  "surface1": "#45475a",
  "overlay0": "#6c7086",
  "text":     "#cdd6f4",
  "subtext":  "#a6adc8",
  "mauve":    "#cba6f7",
  "blue":     "#89b4fa",
  "teal":     "#94e2d5",
  "green":    "#a6e3a1",
  "red":      "#f38ba8",
  "yellow":   "#f9e2af",
  "pink":     "#f5c2e7"
}
```

#### Field reference

| Field      | Used for                                              |
|------------|-------------------------------------------------------|
| `base`     | Main background                                       |
| `surface0` | Slightly elevated surfaces (tab bar, settings rows)   |
| `surface1` | Borders, dividers, selected-but-unfocused rows        |
| `overlay0` | Muted text, file sizes, timestamps, help bar          |
| `text`     | Primary text                                          |
| `subtext`  | Secondary text, status bar                            |
| `mauve`    | Accent — active tab, selected cursor, headings        |
| `blue`     | Directories, current path in status bar               |
| `teal`     | Executables, symlinks                                 |
| `green`    | Code / text files                                     |
| `red`      | Archives, errors, delete confirmation                 |
| `yellow`   | Documents (pdf, docx…), unsaved settings warning      |
| `pink`     | Image files                                           |

> **Tip:** If you name your theme the same as a built-in (e.g. `"nord"`),
> your file takes priority and overrides it.

---

### Minimal working example

**`~/.local/share/VoidDream/themes/Void Red.json`**
```json
{
  "base":     "#0d0005",
  "surface0": "#1a000a",
  "surface1": "#2a0010",
  "overlay0": "#5a2030",
  "text":     "#f0d0d8",
  "subtext":  "#c09098",
  "mauve":    "#ff4060",
  "blue":     "#ff6090",
  "teal":     "#ff80a0",
  "green":    "#c0f080",
  "red":      "#ff2040",
  "yellow":   "#ffcc60",
  "pink":     "#ff90b0"
}
```

---

### Removing or renaming a theme

- **Remove** — delete the `.json` file.
- **Rename** — rename the file. The old name disappears from the list automatically.

---

## Icon Sets

VoidDream ships four built-in icon sets selectable from Settings:

| Name        | Description                                   |
|-------------|-----------------------------------------------|
| `nerdfont`  | Nerd Font glyphs (requires a patched font)    |
| `emoji`     | Unicode emoji — works on most modern terminals |
| `minimal`   | Single ASCII characters                        |
| `none`       | No icons                                      |

> **Custom JSON icon sets** are planned for a future release.
> The `~/.local/share/VoidDream/icons/` directory is already created on launch
> and reserved for this feature.

---

### Built-in icon reference

#### NerdFont — file kinds

| Kind      | Glyph | Codepoint  |
|-----------|-------|------------|
| Directory | `󰉋`   | `U+F024B`  |
| Symlink   | ``   | `U+F0C1`   |
| Image     | ``   | `U+F03E`   |
| Video     | ``   | `U+F03D`   |
| Audio     | ``   | `U+F001`   |
| Archive   | ``   | `U+F410`   |
| PDF       | ``   | `U+F1C1`   |
| Word doc  | ``   | `U+F1C2`   |
| Spreadsheet | `` | `U+F1C3`  |
| Rust      | ``   | `U+E7A8`   |
| Python    | ``   | `U+E606`   |
| JavaScript| ``   | `U+E74E`   |
| Shell     | ``   | `U+F489`   |
| Markdown  | ``   | `U+F48A`   |
| Config    | ``   | `U+F013`   |
| Lock file | ``   | `U+F023`   |
| Generic   | ``   | `U+F15B`   |

#### NerdFont — named directories

| Directory name(s)              | Glyph |
|--------------------------------|-------|
| `.config`                      | ``   |
| `.ssh`                         | ``   |
| `.git`, `.github`              | ``   |
| `downloads`                    | ``   |
| `documents`                    | ``   |
| `desktop`                      | ``   |
| `pictures`, `photos`, `images` | ``   |
| `videos`                       | ``   |
| `music`, `audio`               | ``   |
| `games`                        | ``   |
| `projects`, `dev`, `code`, `src` | `` |
| `home`                         | ``   |
| `bin`, `scripts`               | ``   |
| `themes`                       | ``   |
| `fonts`                        | ``   |
| `node_modules`                 | ``   |
| `target`                       | ``   |
| `dotfiles`                     | ``   |

#### Emoji — file kinds

| Kind      | Emoji |
|-----------|-------|
| Directory | 📁    |
| Symlink   | 🔗    |
| Image     | 🖼     |
| Video     | 🎬    |
| Audio     | 🎵    |
| Archive   | 📦    |
| Document  | 📄    |
| Code      | 📝    |
| Executable | ⚙    |

#### Minimal — file kinds

| Kind      | Char |
|-----------|------|
| Directory | `▸`  |
| Symlink   | `↪`  |
| Image     | `i`  |
| Video     | `v`  |
| Audio     | `a`  |
| Archive   | `z`  |
| Document  | `d`  |
| Code      | `c`  |
| Executable | `x` |
| Other     | `f`  |

---

## Sharing your theme

If you'd like your theme included as a built-in or shared with the community:

1. Create your `.json` file and test it in VoidDream.
2. Open a pull request at [github.com/FemBoyGamerTechGuy/VoidDream](https://github.com/FemBoyGamerTechGuy/VoidDream)
   adding your file to the `themes/` directory.
3. Include a short description of the vibe/inspiration in the PR description.

---

## Quick checklist

- [ ] File is valid JSON (no trailing commas, no comments)
- [ ] All 13 color fields are present
- [ ] All values are `"#RRGGBB"` format (6 hex digits, `#` prefix)
- [ ] File is saved to `~/.local/share/VoidDream/themes/`
- [ ] Theme name (filename stem) is unique enough to not clash with built-ins unintentionally
