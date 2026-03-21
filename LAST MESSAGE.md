# Why VoidDream entered Code Freeze

## The philosophy

VoidDream was never meant to become a perpetually growing project. From the start the goal was simple: build a TUI file manager that works well, looks good, and gets out of your way. Once that goal is met, continuing to add features for the sake of having something to commit is the wrong move. It turns a focused tool into a bloated one.

Code freeze is not the end of the project. It is the point where the project has become what it was supposed to be.

---

## The final update — v0.1.5

Before putting VoidDream into maintenance mode, one last substantial update was prepared to make sure the project was left in the best possible shape for users, contributors, and anyone who wants to fork or modify it. This update focused on three things:

---

### 1. Code decentralisation

The entire codebase lived in a single `main.rs` file of over 3,000 lines. That works fine while one person is actively developing it, but it is a nightmare for anyone else trying to understand, modify, or contribute to it.

v0.1.5 splits `main.rs` into five focused modules:

| File | Responsibility |
|------|---------------|
| `config.rs` | Theme system, icon data, app configuration, settings state |
| `types.rs` | File kind detection, input modes, tab state, all file type lists |
| `app.rs` | The `App` struct and all application logic |
| `ui.rs` | Every drawing function — what you see on screen |
| `keys.rs` | Every keyboard handler |

**Why this matters:**

- A contributor fixing a rendering bug only needs to look at `ui.rs`
- Someone adding a new keybind only touches `keys.rs`
- Someone improving archive extraction only touches `app.rs`
- Nothing is hidden inside a 3,000-line file where you have to grep to find anything

The goal was to make the codebase legible to someone who has never seen it before.

---

### 2. Format support — usability for daily use

A file manager is only useful for daily use if it can handle the files you actually have. Before this update, VoidDream could preview common image formats and play common video and audio files — but anything outside the mainstream would just show a generic icon.

v0.1.5 expands this significantly:

**Images** — RAW camera formats (ARW, CR2, CR3, NEF, ORF, RAF, RW2, DNG, PEF and more), HEIC, HEIF, JXL, HDR, EXR, QOI, TGA, DDS, PSD, XCF. Formats the `image` crate cannot decode natively fall back to an ffmpeg thumbnail automatically — so you still get a visual preview rather than nothing.

**Audio** — APE, MKA, AIFF, ALAC, DSD, MIDI, AMR, TTA, WavPack, AC3, DTS, TrueHD and more added alongside the existing formats.

**Video** — TS, MTS, M2TS, VOB, OGV, 3GP, RM, RMVB, DIVX, MXF, AMV and more — every format mpv can play now has preview support.

**HTML** — HTML files open in your configured browser on Enter, with a dedicated browser opener setting separate from the image/video/audio openers.

The aim was: if you can play it or view it on your system, VoidDream should be able to preview it and open it correctly.

---

### 3. Open-with menu

The last missing piece of a practical daily-use file manager was the ability to open a file with something other than the default app. Press `k` on any file and a menu appears showing all configured openers, with the correct default for that file type at the top. At the bottom is a "Custom command…" option that lets you type any command directly — the file path is appended automatically.

This means VoidDream no longer forces you to go into settings every time you want to open a video in GIMP or a document in a different viewer.

---

## Why code freeze rather than continued development

A few honest reasons:

**The tool is complete.** VoidDream does what a file manager should do. Three-pane layout, fuzzy search, tabs, file operations, archive extraction, previews, theming, configurable keybinds, configurable openers. There is no glaring missing feature.

**Continued development without a clear goal produces bloat.** Every feature added from here is a feature nobody asked for, adding complexity and maintenance burden without meaningfully improving the tool for the people using it.

**Maintenance is valuable.** A project in maintenance mode gets bugs fixed and stays working. A project in active development introduces new bugs faster than old ones are fixed. For a daily-use tool, stability matters more than novelty.

**The code is now in good shape for others.** With the module split done and the format support expanded, anyone who wants to take VoidDream further has a clean foundation to work from. Fork it, patch it, extend it — the structure now makes that straightforward.

---

VoidDream will keep receiving bug fixes. It will keep working. It may occasionally gain a feature when something genuinely makes sense. But the frantic pace of 0.1.x development is done, and that is a good thing.

*— FemBoyGamerTechGuy, March 2026*
