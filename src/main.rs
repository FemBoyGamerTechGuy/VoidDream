use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
    Frame, Terminal,
};
use ratatui_image::{
    picker::Picker,
    protocol::StatefulProtocol,
    StatefulImage,
};
use ratatui::widgets::StatefulWidget;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{self, Read, Write},
    mem,
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}},
    time::{Duration, Instant},
};

// ─── User-defined themes ──────────────────────────────────────────────────────
/// One user theme, loaded from a single JSON file in
///   ~/.local/share/fd-files/themes/
///
/// Each file in that directory is one theme.
/// The filename (without the .json extension) becomes the theme name,
/// so spaces and special characters are fully supported:
///   "Fire Aura.json"  →  theme name "Fire Aura"
///   "My Dark.json"    →  theme name "My Dark"
///
/// File format (all colours are "#RRGGBB" hex):
/// {
///   "base":     "#1e1e2e",
///   "surface0": "#313244",
///   "surface1": "#45475a",
///   "overlay0": "#6c7086",
///   "text":     "#cdd6f4",
///   "subtext":  "#a6adc8",
///   "mauve":    "#cba6f7",
///   "blue":     "#89b4fa",
///   "teal":     "#94e2d5",
///   "green":    "#a6e3a1",
///   "red":      "#f38ba8",
///   "yellow":   "#f9e2af",
///   "pink":     "#f5c2e7"
/// }
///
/// To ADD a theme  → drop a new .json file into the themes directory.
/// To REMOVE a theme → delete its .json file.
/// To RENAME a theme → rename the file.
/// All fields except the 13 base palette keys are optional.
/// Missing fields fall back to a computed value derived from the base palette
/// so old 13-key themes continue to work with no changes.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserThemeColors {
    // ── Base palette (required) ───────────────────────────────────────────
    pub base:     String,
    pub surface0: String,
    pub surface1: String,
    pub overlay0: String,
    pub text:     String,
    pub subtext:  String,
    pub mauve:    String,
    pub blue:     String,
    pub teal:     String,
    pub green:    String,
    pub red:      String,
    pub yellow:   String,
    pub pink:     String,

    // ── Tab bar ───────────────────────────────────────────────────────────
    #[serde(default)] pub tab_active_fg:      Option<String>,
    #[serde(default)] pub tab_active_bg:      Option<String>,
    #[serde(default)] pub tab_inactive_fg:    Option<String>,
    #[serde(default)] pub tab_inactive_bg:    Option<String>,

    // ── File list ─────────────────────────────────────────────────────────
    #[serde(default)] pub cursor_fg:          Option<String>,
    #[serde(default)] pub cursor_bg:          Option<String>,
    #[serde(default)] pub selected_fg:        Option<String>,
    #[serde(default)] pub selected_bg:        Option<String>,
    #[serde(default)] pub border_fg:          Option<String>,
    #[serde(default)] pub panel_bg:           Option<String>,
    #[serde(default)] pub title_fg:           Option<String>,

    // ── File kind colors ──────────────────────────────────────────────────
    #[serde(default)] pub color_dir:          Option<String>,
    #[serde(default)] pub color_symlink:      Option<String>,
    #[serde(default)] pub color_image:        Option<String>,
    #[serde(default)] pub color_video:        Option<String>,
    #[serde(default)] pub color_audio:        Option<String>,
    #[serde(default)] pub color_archive:      Option<String>,
    #[serde(default)] pub color_doc:          Option<String>,
    #[serde(default)] pub color_code:         Option<String>,
    #[serde(default)] pub color_exec:         Option<String>,
    #[serde(default)] pub color_other:        Option<String>,

    // ── Status bar ────────────────────────────────────────────────────────
    #[serde(default)] pub status_path_fg:     Option<String>,
    #[serde(default)] pub status_path_bg:     Option<String>,
    #[serde(default)] pub status_msg_fg:      Option<String>,
    #[serde(default)] pub status_msg_bg:      Option<String>,
    #[serde(default)] pub status_err_fg:      Option<String>,
    #[serde(default)] pub status_err_bg:      Option<String>,
    #[serde(default)] pub status_yank_fg:     Option<String>,
    #[serde(default)] pub status_yank_bg:     Option<String>,
    #[serde(default)] pub status_sel_fg:      Option<String>,
    #[serde(default)] pub status_sel_bg:      Option<String>,
    #[serde(default)] pub status_hint_fg:     Option<String>,
    #[serde(default)] pub status_bar_fg:      Option<String>,
    #[serde(default)] pub status_bar_bg:      Option<String>,

    // ── Overlays ──────────────────────────────────────────────────────────
    #[serde(default)] pub popup_border_fg:    Option<String>,
    #[serde(default)] pub popup_bg:           Option<String>,
    #[serde(default)] pub popup_text_fg:      Option<String>,
    #[serde(default)] pub popup_dim_fg:       Option<String>,

    // ── Progress bar ──────────────────────────────────────────────────────
    #[serde(default)] pub progress_filled_fg: Option<String>,
    #[serde(default)] pub progress_empty_fg:  Option<String>,

    // ── Settings UI ───────────────────────────────────────────────────────
    #[serde(default)] pub settings_key_fg:    Option<String>,
    #[serde(default)] pub settings_val_fg:    Option<String>,
    #[serde(default)] pub settings_cursor_fg: Option<String>,
    #[serde(default)] pub settings_cursor_bg: Option<String>,
}

/// A resolved user theme: name (from filename) + parsed colors.
#[derive(Clone, Debug)]
struct UserThemeEntry {
    name:   String,
    colors: UserThemeColors,
}

impl UserThemeEntry {
    /// Returns ~/.local/share/fd-files/themes/
    fn themes_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        PathBuf::from(home).join(".local").join("share").join("fd-files").join("themes")
    }

    /// Scan the themes directory and load every *.json file.
    /// The theme name is the filename stem (e.g. "Fire Aura.json" → "Fire Aura").
    /// Files that fail to parse are silently skipped.
    fn load_all() -> Vec<Self> {
        let dir = Self::themes_dir();
        if !dir.exists() { return vec![]; }
        let mut themes = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .collect();
            paths.sort(); // deterministic order
            for path in paths {
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() { continue; }
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(colors) = serde_json::from_str::<UserThemeColors>(&text) {
                        themes.push(UserThemeEntry { name, colors });
                    }
                }
            }
        }
        themes
    }

    /// Parse a "#RRGGBB" string into a ratatui Color.
    fn parse_hex(s: &str) -> Color {
        let s = s.trim_start_matches('#');
        if s.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&s[0..2], 16),
                u8::from_str_radix(&s[2..4], 16),
                u8::from_str_radix(&s[4..6], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        }
        Color::Reset
    }

    /// Convert to a runtime Theme, applying optional overrides and deriving
    /// sensible defaults for any missing fields from the base palette.
    fn to_theme(&self) -> Theme {
        let c = &self.colors;
        let p = |s: &str| UserThemeEntry::parse_hex(s);
        let o = |opt: &Option<String>, fallback: &str| -> Color {
            opt.as_deref().map(UserThemeEntry::parse_hex).unwrap_or_else(|| p(fallback))
        };

        // Resolved base palette — only the ones actually used directly in Theme fields
        let base     = p(&c.base);
        let surface0 = p(&c.surface0);
        let text     = p(&c.text);

        Theme {
            base, surface0,
            text,

            // Tab bar
            tab_active_fg:   o(&c.tab_active_fg,   &c.base),
            tab_active_bg:   o(&c.tab_active_bg,   &c.mauve),
            tab_inactive_fg: o(&c.tab_inactive_fg, &c.subtext),
            tab_inactive_bg: o(&c.tab_inactive_bg, &c.surface0),

            // File list cursor/selection
            cursor_fg:  o(&c.cursor_fg,   &c.base),
            cursor_bg:  o(&c.cursor_bg,   &c.mauve),
            selected_fg: o(&c.selected_fg, &c.mauve),
            selected_bg: o(&c.selected_bg, &c.surface0),
            border_fg:  o(&c.border_fg,   &c.surface1),
            panel_bg:   o(&c.panel_bg,    &c.base),
            title_fg:   o(&c.title_fg,    &c.blue),

            // File kind colors
            color_dir:     o(&c.color_dir,     &c.blue),
            color_symlink: o(&c.color_symlink,  &c.pink),
            color_image:   o(&c.color_image,    &c.mauve),
            color_video:   o(&c.color_video,    &c.mauve),
            color_audio:   o(&c.color_audio,    &c.pink),
            color_archive: o(&c.color_archive,  &c.yellow),
            color_doc:     o(&c.color_doc,      &c.teal),
            color_code:    o(&c.color_code,     &c.green),
            color_exec:    o(&c.color_exec,     &c.red),
            color_other:   o(&c.color_other,    &c.text),

            // Status bar
            status_path_fg: o(&c.status_path_fg, &c.base),
            status_path_bg: o(&c.status_path_bg, &c.blue),
            status_msg_fg:  o(&c.status_msg_fg,  &c.base),
            status_msg_bg:  o(&c.status_msg_bg,  &c.teal),
            status_err_fg:  o(&c.status_err_fg,  &c.base),
            status_err_bg:  o(&c.status_err_bg,  &c.red),
            status_yank_fg: o(&c.status_yank_fg, &c.base),
            status_yank_bg: o(&c.status_yank_bg, &c.yellow),
            status_sel_fg:  o(&c.status_sel_fg,  &c.base),
            status_sel_bg:  o(&c.status_sel_bg,  &c.mauve),
            status_hint_fg: o(&c.status_hint_fg, &c.mauve),
            status_bar_fg:  o(&c.status_bar_fg,  &c.subtext),
            status_bar_bg:  o(&c.status_bar_bg,  &c.surface0),

            // Overlays
            popup_border_fg: o(&c.popup_border_fg, &c.mauve),
            popup_bg:        o(&c.popup_bg,        &c.base),
            popup_text_fg:   o(&c.popup_text_fg,   &c.text),
            popup_dim_fg:    o(&c.popup_dim_fg,    &c.overlay0),

            // Progress bar
            progress_filled_fg: o(&c.progress_filled_fg, &c.mauve),
            progress_empty_fg:  o(&c.progress_empty_fg,  &c.surface1),

            // Settings UI
            settings_key_fg:    o(&c.settings_key_fg,    &c.blue),
            settings_val_fg:    o(&c.settings_val_fg,    &c.text),
            settings_cursor_fg: o(&c.settings_cursor_fg, &c.base),
            settings_cursor_bg: o(&c.settings_cursor_bg, &c.mauve),

            // Warn (unsaved changes etc)
            warn_fg: o(&c.status_err_fg,  &c.base),
            warn_bg: o(&c.status_yank_bg, &c.yellow),
        }
    }
}

// ─── User-defined icon sets ───────────────────────────────────────────────────
/// One icon set, loaded from a single JSON file in
///   ~/.local/share/fd-files/icons/
///
/// The filename stem becomes the icon set name:
///   "nerdfont.json"   →  "nerdfont"
///   "My Icons.json"   →  "My Icons"
///
/// File format — all values are single icon strings (unicode / emoji / text):
/// {
///   "dir":        "",     ← default folder
///   "symlink":    "",
///   "image":      "",
///   "video":      "",
///   "audio":      "",
///   "archive":    "",
///   "doc":        "",
///   "code":       "",
///   "exec":       "",
///   "other":      "",
///
///   "by_ext": {           ← per-extension overrides (optional)
///     "rs": "",
///     "py": "",
///     ...
///   },
///   "by_name": {          ← per-filename overrides (optional)
///     "Dockerfile": "",
///     "Makefile":   "",
///     ...
///   },
///   "named_dirs": {       ← per-directory-name overrides (optional)
///     "downloads": "",
///     ".git":      "",
///     ...
///   }
/// }
///
/// To ADD an icon set  → drop a new .json file in the icons directory.
/// To REMOVE           → delete the file.
/// To RENAME           → rename the file.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserIconColors {
    // File kind defaults
    dir:     String,
    symlink: String,
    image:   String,
    video:   String,
    audio:   String,
    archive: String,
    doc:     String,
    code:    String,
    exec:    String,
    other:   String,
    // Optional lookup tables
    #[serde(default)] by_ext:    std::collections::HashMap<String, String>,
    #[serde(default)] by_name:   std::collections::HashMap<String, String>,
    #[serde(default)] named_dirs: std::collections::HashMap<String, String>,
}

#[derive(Clone, Debug)]
struct UserIconEntry {
    name:   String,
    icons:  UserIconColors,
}

impl UserIconEntry {
    fn icons_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        PathBuf::from(home).join(".local").join("share").join("fd-files").join("icons")
    }

    fn load_all() -> Vec<Self> {
        let dir = Self::icons_dir();
        if !dir.exists() { return vec![]; }
        let mut sets = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .collect();
            paths.sort();
            for path in paths {
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("").to_string();
                if name.is_empty() { continue; }
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(icons) = serde_json::from_str::<UserIconColors>(&text) {
                        sets.push(UserIconEntry { name, icons });
                    }
                }
            }
        }
        sets
    }

    /// Look up the icon string for a given path + kind.
    fn get_icon(&self, path: &Path, kind: &FileKind) -> String {
        let ic = &self.icons;
        // 1. by_name exact match
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        if let Some(i) = ic.by_name.get(&fname) { return i.clone(); }

        // 2. named_dirs (for directories and symlinks-to-dirs)
        if *kind == FileKind::Dir || (*kind == FileKind::Symlink && path.is_dir()) {
            if let Some(i) = ic.named_dirs.get(&fname) { return i.clone(); }
            if *kind == FileKind::Dir { return ic.dir.clone(); }
        }
        if *kind == FileKind::Symlink { return ic.symlink.clone(); }

        // 3. by_ext
        let ext = path.extension().and_then(|e| e.to_str())
            .map(|s| s.to_lowercase()).unwrap_or_default();
        if !ext.is_empty() {
            if let Some(i) = ic.by_ext.get(&ext) { return i.clone(); }
        }

        // 4. fall back to kind default
        match kind {
            FileKind::Image   => ic.image.clone(),
            FileKind::Video   => ic.video.clone(),
            FileKind::Audio   => ic.audio.clone(),
            FileKind::Archive => ic.archive.clone(),
            FileKind::Doc     => ic.doc.clone(),
            FileKind::Code    => ic.code.clone(),
            FileKind::Exec    => ic.exec.clone(),
            FileKind::Other   => ic.other.clone(),
            FileKind::Dir     => ic.dir.clone(),
            FileKind::Symlink => ic.symlink.clone(),
        }
    }
}

// ─── Theme ────────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct Theme {
    // Base — only the ones still used directly in draw code
    base: Color, surface0: Color, text: Color,
    // Tab bar
    tab_active_fg:      Color, tab_active_bg:      Color,
    tab_inactive_fg:    Color, tab_inactive_bg:    Color,
    // File list
    cursor_fg:          Color, cursor_bg:          Color,
    selected_fg:        Color, selected_bg:        Color,
    border_fg:          Color, panel_bg:           Color,
    title_fg:           Color,
    // File kind colors
    color_dir:     Color, color_symlink: Color,
    color_image:   Color, color_video:   Color,
    color_audio:   Color, color_archive: Color,
    color_doc:     Color, color_code:    Color,
    color_exec:    Color, color_other:   Color,
    // Status bar
    status_path_fg:  Color, status_path_bg:  Color,
    status_msg_fg:   Color, status_msg_bg:   Color,
    status_err_fg:   Color, status_err_bg:   Color,
    status_yank_fg:  Color, status_yank_bg:  Color,
    status_sel_fg:   Color, status_sel_bg:   Color,
    status_hint_fg:  Color,
    status_bar_fg:   Color, status_bar_bg:   Color,
    // Overlays
    popup_border_fg: Color, popup_bg:        Color,
    popup_text_fg:   Color, popup_dim_fg:    Color,
    // Progress bar
    progress_filled_fg: Color, progress_empty_fg: Color,
    // Settings UI
    settings_key_fg:    Color, settings_val_fg:    Color,
    settings_cursor_fg: Color, settings_cursor_bg: Color,
    // Unsaved / warning highlight
    warn_fg: Color, warn_bg: Color,
}

impl Theme {
    /// Resolve a theme by name from user themes, falling back to a default dark theme.
    fn resolve(name: &str, user_themes: &[UserThemeEntry]) -> Self {
        if let Some(ut) = user_themes.iter().find(|t| t.name == name) {
            return ut.to_theme();
        }
        Self::fallback()
    }

    /// All theme names from user theme files only.
    fn all_names_merged(user_themes: &[UserThemeEntry]) -> Vec<String> {
        user_themes.iter().map(|u| u.name.clone()).collect()
    }

    /// Fallback theme (catppuccin-macchiato palette) used when no theme file is found.
    fn fallback() -> Self {
        let base     = Color::Rgb(30, 32, 48);    let surface0 = Color::Rgb(54, 58, 79);
        let surface1 = Color::Rgb(73, 77,100);    let overlay0 = Color::Rgb(110,115,141);
        let text     = Color::Rgb(202,211,245);   let subtext  = Color::Rgb(165,173,203);
        let mauve    = Color::Rgb(198,160,246);   let blue     = Color::Rgb(138,173,244);
        let teal     = Color::Rgb(139,213,202);   let green    = Color::Rgb(166,218,149);
        let red      = Color::Rgb(237,135,150);   let yellow   = Color::Rgb(238,212,159);
        let pink     = Color::Rgb(245,189,230);
        Self {
            base, surface0, text,
            tab_active_fg: base,      tab_active_bg: mauve,
            tab_inactive_fg: subtext, tab_inactive_bg: surface0,
            cursor_fg: base,    cursor_bg: mauve,
            selected_fg: mauve, selected_bg: surface0,
            border_fg: surface1, panel_bg: base, title_fg: blue,
            color_dir: blue,    color_symlink: pink,
            color_image: mauve, color_video: mauve,  color_audio: pink,
            color_archive: yellow, color_doc: teal,
            color_code: green,  color_exec: red,     color_other: text,
            status_path_fg: base,    status_path_bg: blue,
            status_msg_fg: base,     status_msg_bg: teal,
            status_err_fg: base,     status_err_bg: red,
            status_yank_fg: base,    status_yank_bg: yellow,
            status_sel_fg: base,     status_sel_bg: mauve,
            status_hint_fg: mauve,
            status_bar_fg: subtext,  status_bar_bg: surface0,
            popup_border_fg: mauve,  popup_bg: base,
            popup_text_fg: text,     popup_dim_fg: overlay0,
            progress_filled_fg: mauve, progress_empty_fg: surface1,
            settings_key_fg: blue,   settings_val_fg: text,
            settings_cursor_fg: base, settings_cursor_bg: mauve,
            warn_fg: base, warn_bg: yellow,
        }
    }

}

/// The active icon set is always a loaded UserIconEntry.
/// The built-in sets (nerdfont, emoji, minimal, none) are shipped as JSON files
/// in ~/.local/share/fd-files/icons/ — there is no hardcoded fallback logic.
/// If the configured set cannot be found, icons are simply empty strings.
#[derive(Clone, PartialEq)]
struct ResolvedIconSet(Option<String>); // holds the set name, looked up in user_icons at call time

impl ResolvedIconSet {
    fn resolve(name: &str, user_icons: &[UserIconEntry]) -> Self {
        if user_icons.iter().any(|u| u.name == name) {
            Self(Some(name.to_string()))
        } else {
            Self(None) // set not found → silent empty icons
        }
    }

    /// All icon set names from loaded files.
    fn all_names_merged(user_icons: &[UserIconEntry]) -> Vec<String> {
        user_icons.iter().map(|u| u.name.clone()).collect()
    }
}

fn get_icon(path: &Path, kind: &FileKind, icon_set: &ResolvedIconSet, user_icons: &[UserIconEntry]) -> String {
    let name = match &icon_set.0 { Some(n) => n, None => return String::new() };
    if let Some(entry) = user_icons.iter().find(|u| &u.name == name) {
        return entry.get_icon(path, kind);
    }
    String::new()
}

fn st(fg: Color) -> Style { Style::default().fg(fg) }
fn st_bg(fg: Color, bg: Color) -> Style { Style::default().fg(fg).bg(bg) }
fn bold(fg: Color) -> Style { Style::default().fg(fg).add_modifier(Modifier::BOLD) }
fn bold_bg(fg: Color, bg: Color) -> Style {
    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
}


#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub show_hidden:       bool,
    pub date_format:       String,
    pub col_parent:        u16,
    pub col_files:         u16,
    pub theme:             String,
    pub icon_set:          String,
    pub opener_image:      String,
    pub opener_video:      String,
    pub opener_audio:      String,
    pub opener_doc:        String,
    pub opener_editor:     String,
    pub opener_archive:    String,
    // Terminal emulator used to run executables and shell scripts.
    // The command is launched as:  <opener_terminal> -- <program>
    // Examples: "kitty", "foot", "alacritty", "wezterm start"
    pub opener_terminal:   String,
    pub key_copy:          String,
    pub key_cut:           String,
    pub key_paste:         String,
    pub key_delete:        String,
    pub key_rename:        String,
    pub key_new_file:      String,
    pub key_new_dir:       String,
    pub key_search:        String,
    pub key_toggle_hidden: String,
    pub key_quit:          String,
    pub key_new_tab:       String,
    pub key_close_tab:     String,
    pub key_switch_tab:    String,
    pub key_select:        String,
    pub key_select_all:    String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_hidden: true, date_format: "%d/%m/%Y %H:%M".into(),
            col_parent: 20, col_files: 37,
            theme: "catppuccin-macchiato".into(), icon_set: "nerdfont".into(),
            opener_image: "mirage".into(), opener_video: "mpv".into(),
            opener_audio: "mpv".into(), opener_doc: "libreoffice".into(),
            opener_editor: "nvim".into(), opener_archive: "ouch decompress".into(),
            opener_terminal: String::new(), // auto-detected at startup
            key_copy: "c".into(), key_cut: "u".into(), key_paste: "p".into(),
            key_delete: "d".into(), key_rename: "r".into(),
            key_new_file: "f".into(), key_new_dir: "m".into(),
            key_search: "/".into(), key_toggle_hidden: ".".into(),
            key_quit: "q".into(), key_new_tab: "t".into(),
            key_close_tab: "x".into(), key_switch_tab: "Tab".into(),
            key_select: "Space".into(),
            key_select_all: "Ctrl+a".into(),
        }
    }
}

impl Config {
    fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        PathBuf::from(home).join(".config").join("fd-files").join("config.json")
    }
    fn load() -> Self {
        let p = Self::config_path();
        if p.exists() {
            if let Ok(d) = fs::read_to_string(&p) {
                if let Ok(c) = serde_json::from_str(&d) { return c; }
            }
        }
        let c = Self::default(); let _ = c.save(); c
    }
    fn save(&self) -> Result<()> {
        let p = Self::config_path();
        if let Some(par) = p.parent() { fs::create_dir_all(par)?; }
        fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

// Shell script extensions — these are run in a new terminal with their
// appropriate interpreter even if they don't have the execute bit set.
const SHELL_EXT: &[&str] = &["sh","bash","zsh","fish"];
/// Java archive — run with `java -jar <file>` in a new terminal window.
const JAVA_EXT:  &[&str] = &["jar"];


const IMAGE_EXT:   &[&str] = &["png","jpg","jpeg","gif","bmp","webp","svg","ico","tiff","avif"];
const VIDEO_EXT:   &[&str] = &["mp4","mkv","avi","mov","webm","flv","wmv","m4v","mpg","mpeg"];
const AUDIO_EXT:   &[&str] = &["mp3","flac","ogg","wav","aac","m4a","opus","wma"];
const ARCHIVE_EXT: &[&str] = &["zip","tar","gz","bz2","xz","7z","rar","zst","tgz","tbz2"];
const DOC_EXT:     &[&str] = &["pdf","doc","docx","odt","xls","xlsx","ods","ppt","pptx","odp"];
const CODE_EXT:    &[&str] = &[
    "py","js","ts","rs","go","c","cpp","h","java","rb","php",
    "sh","bash","zsh","fish","lua","vim","toml","yaml","yml",
    "json","xml","html","css","scss","md","rst","txt","conf",
    "ini","cfg","env","lock",
];

#[derive(Clone, PartialEq)]
enum FileKind { Dir, Image, Video, Audio, Archive, Doc, Code, Exec, Symlink, Other }

fn file_kind(path: &Path) -> FileKind {
    if path.is_symlink() { return FileKind::Symlink; }
    if path.is_dir()     { return FileKind::Dir; }
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
    let ext = ext.as_str();
    if IMAGE_EXT.contains(&ext)   { return FileKind::Image; }
    if VIDEO_EXT.contains(&ext)   { return FileKind::Video; }
    if AUDIO_EXT.contains(&ext)   { return FileKind::Audio; }
    if ARCHIVE_EXT.contains(&ext) { return FileKind::Archive; }
    if DOC_EXT.contains(&ext)     { return FileKind::Doc; }
    if CODE_EXT.contains(&ext)    { return FileKind::Code; }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = path.metadata() {
            if m.permissions().mode() & 0o111 != 0 { return FileKind::Exec; }
        }
    }
    FileKind::Other
}

fn kind_color(k: &FileKind, t: &Theme) -> Color {
    match k {
        FileKind::Dir     => t.color_dir,     FileKind::Image   => t.color_image,
        FileKind::Video   => t.color_video,   FileKind::Audio   => t.color_audio,
        FileKind::Archive => t.color_archive, FileKind::Doc     => t.color_doc,
        FileKind::Code    => t.color_code,    FileKind::Exec    => t.color_exec,
        FileKind::Symlink => t.color_symlink, FileKind::Other   => t.color_other,
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────
fn human_size(size: u64) -> String {
    const U: &[&str] = &["B","K","M","G","T"];
    let mut s = size as f64; let mut u = 0;
    while s >= 1024.0 && u < U.len()-1 { s /= 1024.0; u += 1; }
    if u == 0 { format!("{:.0}B", s) } else { format!("{:.1}{}", s, U[u]) }
}
fn file_size_str(path: &Path) -> String {
    if path.is_dir() { return String::new(); }
    path.metadata().map(|m| human_size(m.len())).unwrap_or("?".into())
}
fn format_mtime(path: &Path, fmt: &str) -> String {
    use std::time::SystemTime;
    path.metadata().and_then(|m| m.modified()).map(|t| {
        let secs = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
        let (y,mo,d,h,mi) = secs_to_datetime(secs);
        fmt.replace("%Y", &format!("{:04}", y))
           .replace("%m", &format!("{:02}", mo))
           .replace("%d", &format!("{:02}", d))
           .replace("%H", &format!("{:02}", h))
           .replace("%M", &format!("{:02}", mi))
    }).unwrap_or_else(|_| "?".into())
}
fn secs_to_datetime(secs: u64) -> (u64,u64,u64,u64,u64) {
    let mi=(secs%3600)/60; let h=(secs%86400)/3600; let days=secs/86400;
    let mut y=1970u64; let mut rem=days;
    loop { let dy=if is_leap(y){366}else{365}; if rem<dy{break;} rem-=dy; y+=1; }
    let months=if is_leap(y){[31,29,31,30,31,30,31,31,30,31,30,31u64]}else{[31,28,31,30,31,30,31,31,30,31,30,31]};
    let mut mo=1u64;
    for &dm in &months { if rem<dm{break;} rem-=dm; mo+=1; }
    (y,mo,rem+1,h,mi)
}
fn is_leap(y:u64)->bool { y%4==0&&(y%100!=0||y%400==0) }

fn list_dir(path: &Path, show_hidden: bool) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = match fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return vec![],
    };
    if !show_hidden {
        entries.retain(|p| !p.file_name().and_then(|n|n.to_str()).map(|n|n.starts_with('.')).unwrap_or(false));
    }
    entries.sort_by(|a,b| {
        let ad=a.is_dir(); let bd=b.is_dir();
        if ad!=bd { return bd.cmp(&ad); }
        a.file_name().cmp(&b.file_name())
    });
    entries
}
fn dirs_home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"))
}
fn which(cmd: &str) -> bool {
    Command::new("which").arg(cmd).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Parse a config key string like "c", "Ctrl+c", "Alt+x" into a
/// (KeyCode, KeyModifiers) pair so configured keybinds actually work.
fn parse_key(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    let s = s.trim();
    if s.is_empty() { return None; }

    // Split off modifier prefix(es): "Ctrl+Shift+x" → mods=CONTROL|SHIFT, key="x"
    let mut mods = KeyModifiers::NONE;
    let mut rest = s;
    loop {
        if let Some(r) = rest.strip_prefix("Ctrl+").or_else(|| rest.strip_prefix("ctrl+")) {
            mods |= KeyModifiers::CONTROL; rest = r;
        } else if let Some(r) = rest.strip_prefix("Alt+").or_else(|| rest.strip_prefix("alt+")) {
            mods |= KeyModifiers::ALT; rest = r;
        } else if let Some(r) = rest.strip_prefix("Shift+").or_else(|| rest.strip_prefix("shift+")) {
            mods |= KeyModifiers::SHIFT; rest = r;
        } else {
            break;
        }
    }

    let code = match rest {
        "Space" | "space"   => KeyCode::Char(' '),
        "Enter" | "enter"   => KeyCode::Enter,
        "Esc"   | "esc"     => KeyCode::Esc,
        "Tab"   | "tab"     => KeyCode::Tab,
        "Backspace"         => KeyCode::Backspace,
        "Delete"            => KeyCode::Delete,
        "Up"                => KeyCode::Up,
        "Down"              => KeyCode::Down,
        "Left"              => KeyCode::Left,
        "Right"             => KeyCode::Right,
        "Home"              => KeyCode::Home,
        "End"               => KeyCode::End,
        "PageUp"            => KeyCode::PageUp,
        "PageDown"          => KeyCode::PageDown,
        c if c.chars().count() == 1 => KeyCode::Char(c.chars().next().unwrap()),
        _ => return None,
    };
    Some((code, mods))
}

/// Returns true if `key`+`mods` matches the configured string `cfg_key`.
fn key_matches(key: KeyCode, mods: KeyModifiers, cfg_key: &str) -> bool {
    match parse_key(cfg_key) {
        Some((k, m)) => k == key && m == mods,
        None         => false,
    }
}
// ─── Progress-aware file operations ──────────────────────────────────────────

/// A single progress update sent from the background worker to the UI thread.
#[derive(Debug)]
enum ProgressUpdate {
    /// How many total bytes need to be transferred across all files.
    TotalBytes(u64),
    /// Bytes transferred so far (cumulative).
    BytesDone(u64),
    /// Name of the file currently being processed.
    CurrentFile(String),
    /// The operation finished successfully.
    Done,
    /// The operation failed with this error string.
    Error(String),
}

/// Chunk size used when copying file data — 4 MiB gives good throughput
/// without burning too much memory.
const COPY_CHUNK: usize = 4 * 1024 * 1024;

/// Recursively calculate the total byte size of a path (file or directory).
fn total_size(path: &Path) -> u64 {
    if path.is_dir() {
        fs::read_dir(path)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| total_size(&e.path())).sum())
            .unwrap_or(0)
    } else {
        path.metadata().map(|m| m.len()).unwrap_or(0)
    }
}

/// Copy a single file in chunks, reporting byte progress through `tx`.
/// `done_so_far` is the running total before this file started.
fn copy_file_progress(
    src: &Path,
    dst: &Path,
    done_so_far: &mut u64,
    tx: &mpsc::Sender<ProgressUpdate>,
    cancel: &Arc<AtomicBool>,
) -> io::Result<()> {
    let name = src.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("…")
        .to_string();
    let _ = tx.send(ProgressUpdate::CurrentFile(name));

    let mut src_file = fs::File::open(src)?;
    // Create parent dirs if needed (can happen deep in a directory tree).
    if let Some(par) = dst.parent() { fs::create_dir_all(par)?; }
    let mut dst_file = fs::File::create(dst)?;

    let mut buf = vec![0u8; COPY_CHUNK];
    loop {
        // Respect cancellation requests.
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = src_file.read(&mut buf)?;
        if n == 0 { break; }
        dst_file.write_all(&buf[..n])?;
        *done_so_far += n as u64;
        let _ = tx.send(ProgressUpdate::BytesDone(*done_so_far));
    }
    Ok(())
}

/// Recursively copy a directory tree, reporting progress for each file.
fn copy_dir_progress(
    src: &Path,
    dst: &Path,
    done_so_far: &mut u64,
    tx: &mpsc::Sender<ProgressUpdate>,
    cancel: &Arc<AtomicBool>,
) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_progress(&src_path, &dst_path, done_so_far, tx, cancel)?;
        } else {
            copy_file_progress(&src_path, &dst_path, done_so_far, tx, cancel)?;
        }
    }
    Ok(())
}

/// Recursively delete a path, reporting the name of each item as it is removed.
fn delete_path_progress(
    path: &Path,
    tx: &mpsc::Sender<ProgressUpdate>,
    cancel: &Arc<AtomicBool>,
) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("…")
        .to_string();
    let _ = tx.send(ProgressUpdate::CurrentFile(name));
    if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

// ─── Settings ─────────────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
enum SettingsSection { Behaviour, Appearance, Openers, Keybinds }

struct SettingsState {
    section:    SettingsSection,
    cursor:     usize,
    editing:    bool,
    edit_buf:   String,
    dirty:      bool,
    dropdown:   bool,   // true when showing a dropdown picker
    dd_cursor:  usize,  // selected index within the dropdown
}
impl SettingsState {
    fn new() -> Self {
        Self { section: SettingsSection::Behaviour, cursor: 0, editing: false, edit_buf: String::new(), dirty: false, dropdown: false, dd_cursor: 0 }
    }
    fn section_items(s: &SettingsSection) -> Vec<(&'static str, &'static str)> {
        match s {
            SettingsSection::Behaviour  => vec![
                ("show_hidden","Show hidden files"), ("date_format","Date format"),
            ],
            SettingsSection::Appearance => vec![
                ("col_parent","Parent pane width (%)"), ("col_files","Files pane width (%)"),
                ("theme","Theme"), ("icon_set","Icon set"),
            ],
            SettingsSection::Openers    => vec![
                ("opener_image","Image"), ("opener_video","Video"), ("opener_audio","Audio"),
                ("opener_doc","Documents"), ("opener_editor","Editor"), ("opener_archive","Archives"),
                ("opener_terminal","Terminal  (for exec / scripts)"),
            ],
            SettingsSection::Keybinds   => vec![
                ("key_copy","Copy"), ("key_cut","Cut"), ("key_paste","Paste"),
                ("key_delete","Delete"), ("key_rename","Rename"),
                ("key_new_file","New file"), ("key_new_dir","New directory"),
                ("key_search","Search"), ("key_toggle_hidden","Toggle hidden"),
                ("key_quit","Quit"), ("key_new_tab","New tab"),
                ("key_close_tab","Close tab"), ("key_switch_tab","Switch tab"),
                ("key_select","Select"), ("key_select_all","Select all"),
            ],
        }
    }
    fn dropdown_options(key: &str, user_themes: &[UserThemeEntry], user_icons: &[UserIconEntry]) -> Option<Vec<String>> {
        match key {
            "theme"       => Some(Theme::all_names_merged(user_themes)),
            "icon_set"    => Some(ResolvedIconSet::all_names_merged(user_icons)),
            "show_hidden" => Some(vec!["true".to_string(), "false".to_string()]),
            _ => None,
        }
    }
    fn get_value(key: &str, cfg: &Config) -> String {
        match key {
            "show_hidden"       => cfg.show_hidden.to_string(),
            "date_format"       => cfg.date_format.clone(),
            "col_parent"        => cfg.col_parent.to_string(),
            "col_files"         => cfg.col_files.to_string(),
            "theme"             => cfg.theme.clone(),
            "icon_set"          => cfg.icon_set.clone(),
            "opener_image"      => cfg.opener_image.clone(),
            "opener_video"      => cfg.opener_video.clone(),
            "opener_audio"      => cfg.opener_audio.clone(),
            "opener_doc"        => cfg.opener_doc.clone(),
            "opener_editor"     => cfg.opener_editor.clone(),
            "opener_archive"    => cfg.opener_archive.clone(),
            "opener_terminal"   => cfg.opener_terminal.clone(),
            "key_copy"          => cfg.key_copy.clone(),
            "key_cut"           => cfg.key_cut.clone(),
            "key_paste"         => cfg.key_paste.clone(),
            "key_delete"        => cfg.key_delete.clone(),
            "key_rename"        => cfg.key_rename.clone(),
            "key_new_file"      => cfg.key_new_file.clone(),
            "key_new_dir"       => cfg.key_new_dir.clone(),
            "key_search"        => cfg.key_search.clone(),
            "key_toggle_hidden" => cfg.key_toggle_hidden.clone(),
            "key_quit"          => cfg.key_quit.clone(),
            "key_new_tab"       => cfg.key_new_tab.clone(),
            "key_close_tab"     => cfg.key_close_tab.clone(),
            "key_switch_tab"    => cfg.key_switch_tab.clone(),
            "key_select"        => cfg.key_select.clone(),
            "key_select_all"    => cfg.key_select_all.clone(),
            _ => String::new(),
        }
    }
    fn set_value(key: &str, val: &str, cfg: &mut Config) {
        match key {
            "show_hidden"       => cfg.show_hidden = val == "true",
            "date_format"       => cfg.date_format = val.into(),
            "col_parent"        => { if let Ok(n) = val.parse::<u16>() { cfg.col_parent = n.clamp(10,40); } }
            "col_files"         => { if let Ok(n) = val.parse::<u16>() { cfg.col_files  = n.clamp(20,60); } }
            "theme"             => cfg.theme    = val.into(),
            "icon_set"          => cfg.icon_set = val.into(),
            "opener_image"      => cfg.opener_image    = val.into(),
            "opener_video"      => cfg.opener_video    = val.into(),
            "opener_audio"      => cfg.opener_audio    = val.into(),
            "opener_doc"        => cfg.opener_doc      = val.into(),
            "opener_editor"     => cfg.opener_editor   = val.into(),
            "opener_archive"    => cfg.opener_archive  = val.into(),
            "opener_terminal"   => cfg.opener_terminal = val.into(),
            "key_copy"          => cfg.key_copy          = val.into(),
            "key_cut"           => cfg.key_cut           = val.into(),
            "key_paste"         => cfg.key_paste         = val.into(),
            "key_delete"        => cfg.key_delete        = val.into(),
            "key_rename"        => cfg.key_rename        = val.into(),
            "key_new_file"      => cfg.key_new_file      = val.into(),
            "key_new_dir"       => cfg.key_new_dir       = val.into(),
            "key_search"        => cfg.key_search        = val.into(),
            "key_toggle_hidden" => cfg.key_toggle_hidden = val.into(),
            "key_quit"          => cfg.key_quit          = val.into(),
            "key_new_tab"       => cfg.key_new_tab       = val.into(),
            "key_close_tab"     => cfg.key_close_tab     = val.into(),
            "key_switch_tab"    => cfg.key_switch_tab    = val.into(),
            "key_select"        => cfg.key_select        = val.into(),
            "key_select_all"    => cfg.key_select_all    = val.into(),
            _ => {}
        }
    }
}

// ─── Tab ─────────────────────────────────────────────────────────────────────
struct Tab {
    cwd:            PathBuf,
    entries:        Vec<PathBuf>,
    state:          ListState,
    scroll:         usize,
    selected:       HashSet<PathBuf>,
    show_hidden:    bool,
    search_query:   String,
    search_results: Option<Vec<PathBuf>>,
}
impl Tab {
    fn new(cwd: PathBuf, show_hidden: bool) -> Self {
        let mut t = Self {
            cwd, entries: vec![], state: ListState::default(), scroll: 0,
            selected: HashSet::new(), show_hidden,
            search_query: String::new(), search_results: None,
        };
        t.refresh();
        if !t.entries.is_empty() { t.state.select(Some(0)); }
        t
    }
    fn refresh(&mut self) {
        self.entries = list_dir(&self.cwd, self.show_hidden);
        let cur = self.state.selected().unwrap_or(0);
        if self.entries.is_empty() { self.state.select(None); }
        else { self.state.select(Some(cur.min(self.entries.len()-1))); }
        self.selected.retain(|p| self.entries.contains(p));
    }
    fn visible(&self) -> &[PathBuf] {
        if let Some(ref r) = self.search_results { r.as_slice() } else { &self.entries }
    }
    fn current(&self) -> Option<&PathBuf> {
        self.state.selected().and_then(|i| self.visible().get(i))
    }
    fn move_cursor(&mut self, delta: i32) {
        let len = self.visible().len(); if len == 0 { return; }
        let cur = self.state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).clamp(0, len as i32 - 1) as usize;
        self.state.select(Some(next));
    }
    fn enter(&mut self) {
        if let Some(p) = self.current().cloned() {
            if p.is_dir() {
                self.cwd = p; self.state.select(Some(0)); self.scroll = 0;
                self.selected.clear(); self.search_query.clear();
                self.search_results = None; self.refresh();
            }
        }
    }
    fn leave(&mut self) {
        if let Some(parent) = self.cwd.parent().map(|p| p.to_path_buf()) {
            if parent != self.cwd {
                let old = self.cwd.clone();
                self.cwd = parent; self.state.select(Some(0)); self.scroll = 0;
                self.selected.clear(); self.search_query.clear();
                self.search_results = None; self.refresh();
                if let Some(i) = self.entries.iter().position(|e| *e == old) {
                    self.state.select(Some(i));
                }
            }
        }
    }
    fn toggle_select(&mut self) {
        if let Some(p) = self.current().cloned() {
            if self.selected.contains(&p) { self.selected.remove(&p); }
            else { self.selected.insert(p); }
            self.move_cursor(1);
        }
    }
    fn select_all(&mut self)   { self.selected = self.entries.iter().cloned().collect(); }
    fn deselect_all(&mut self) { self.selected.clear(); }
}

// ─── InputMode ────────────────────────────────────────────────────────────────
#[derive(PartialEq)]
enum InputMode {
    Normal,
    FuzzySearch,
    Rename(String),
    NewFile,
    NewDir,
    Confirm,
    Settings,
    /// A background file operation is running — show the progress overlay.
    Progress,
    /// About to run a command — show the args box so the user can append
    /// extra arguments before launching.  Holds the base command args and cwd.
    RunArgs { args: Vec<String>, cwd: PathBuf },
    /// Showing the help overlay (press ? to open, any key to close).
    Help,
}

// ─── App ─────────────────────────────────────────────────────────────────────
struct App {
    cfg:         Config,
    theme:       Theme,
    icon_set:    ResolvedIconSet,
    tabs:        Vec<Tab>,
    tab_idx:     usize,
    yank:        Vec<PathBuf>,
    yank_cut:    bool,
    mode:        InputMode,
    input_buf:   String,
    status_msg:  String,
    status_err:  bool,
    msg_time:    Option<Instant>,
    nvim_path:   Option<PathBuf>,
    settings:    SettingsState,
    fuzzy_query:   String,
    fuzzy_index:   Vec<PathBuf>,
    fuzzy_results: Vec<PathBuf>,
    fuzzy_cursor:  usize,
    fuzzy_loading: bool,
    fuzzy_rx:      Option<mpsc::Receiver<PathBuf>>,
    last_preview_size: (u16, u16),
    // Image preview
    img_picker:   Option<Picker>,
    img_path:     Option<PathBuf>,
    img_state:    Option<StatefulProtocol>,
    // Video thumbnail
    thumb_src:    Option<PathBuf>,
    thumb_tmp:    Option<PathBuf>,
    thumb_rx:     Option<mpsc::Receiver<Option<PathBuf>>>,
    thumb_meta:   Option<String>,
    // User-defined themes / icons
    user_themes:  Vec<UserThemeEntry>,
    user_icons:   Vec<UserIconEntry>,
    // Background file-operation progress
    progress_rx:       Option<mpsc::Receiver<ProgressUpdate>>,
    progress_cancel:   Option<Arc<AtomicBool>>,
    progress_label:    String,
    progress_total:    u64,
    progress_done:     u64,
    progress_current:  String,
    progress_finished: bool,
}
impl App {
    fn new(start: PathBuf, mut cfg: Config) -> Self {
        let sh = cfg.show_hidden;
        let user_themes = UserThemeEntry::load_all();
        let user_icons  = UserIconEntry::load_all();
        let theme    = Theme::resolve(&cfg.theme, &user_themes);
        let icon_set = ResolvedIconSet::resolve(&cfg.icon_set, &user_icons);
        // Auto-detect terminal if not set in config.
        if cfg.opener_terminal.is_empty() {
            cfg.opener_terminal = Self::detect_terminal();
        }
        Self {
            theme, icon_set,
            tabs: vec![Tab::new(start, sh)], tab_idx: 0,
            yank: vec![], yank_cut: false,
            mode: InputMode::Normal, input_buf: String::new(),
            status_msg: String::new(), status_err: false,
            msg_time: None, nvim_path: None,
            settings: SettingsState::new(), cfg,
            fuzzy_query: String::new(), fuzzy_index: vec![],
            fuzzy_results: vec![], fuzzy_cursor: 0,
            fuzzy_loading: false, fuzzy_rx: None,
            last_preview_size: (0, 0),
            img_picker: Picker::from_query_stdio().ok(),
            img_path: None, img_state: None,
            thumb_src: None, thumb_tmp: None,
            thumb_rx: None, thumb_meta: None,
            user_themes, user_icons,
            progress_rx: None, progress_cancel: None,
            progress_label: String::new(), progress_total: 0,
            progress_done: 0, progress_current: String::new(),
            progress_finished: false,
        }
    }
    fn tab(&self)         -> &Tab     { &self.tabs[self.tab_idx] }
    fn tab_mut(&mut self) -> &mut Tab { &mut self.tabs[self.tab_idx] }
    fn msg(&mut self, text: &str, err: bool) {
        self.status_msg = text.to_string();
        self.status_err = err;
        self.msg_time   = Some(Instant::now());
    }
    fn tick(&mut self) {
        if let Some(t) = self.msg_time {
            if t.elapsed() > Duration::from_secs(4) {
                self.status_msg.clear(); self.msg_time = None;
            }
        }
        // Drain streamed paths — collect into local vec first to avoid borrow conflicts
        if self.fuzzy_loading {
            let mut new_paths: Vec<PathBuf> = Vec::new();
            let mut finished = false;
            if let Some(rx) = &self.fuzzy_rx {
                for _ in 0..500 {
                    match rx.try_recv() {
                        Ok(path) => new_paths.push(path),
                        Err(mpsc::TryRecvError::Empty)        => break,
                        Err(mpsc::TryRecvError::Disconnected) => { finished = true; break; }
                    }
                }
            }
            // rx borrow dropped here — now free to mutate self
            let got_any = !new_paths.is_empty() || finished;
            self.fuzzy_index.extend(new_paths);
            if finished {
                self.fuzzy_loading = false;
                self.fuzzy_rx      = None;
            }
            if got_any {
                if self.fuzzy_query.is_empty() {
                    self.fuzzy_results = self.fuzzy_index.clone();
                } else {
                    self.fuzzy_update_results();
                }
            }
        }
        // Poll thumbnail extraction result from background ffmpeg thread
        if self.thumb_rx.is_some() {
            let result = self.thumb_rx.as_ref().unwrap().try_recv().ok();
            if let Some(maybe_path) = result {
                self.thumb_rx = None;
                if let Some(tmp) = maybe_path {
                    self.thumb_tmp = Some(tmp.clone());
                    if let Some(picker) = self.img_picker.as_mut() {
                        if let Ok(img) = image::open(&tmp) {
                            self.img_state = Some(picker.new_resize_protocol(img));
                        }
                    }
                }
            }
        }
        // Clear image state if we've navigated away from an image or video
        let cur_ext = self.tab().current()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        let is_visual = IMAGE_EXT.contains(&cur_ext.as_str()) || VIDEO_EXT.contains(&cur_ext.as_str());
        if !is_visual && self.img_state.is_some() {
            self.img_path  = None;
            self.img_state = None;
        }
        // Drain progress messages from any running background file operation.
        self.tick_progress();
    }
    fn yank_files(&mut self, cut: bool) {
        let targets: Vec<PathBuf> = if !self.tab().selected.is_empty() {
            self.tab().selected.iter().cloned().collect()
        } else if let Some(p) = self.tab().current().cloned() { vec![p] }
        else { self.msg("Nothing to yank", true); return; };
        let n = targets.len();
        self.yank = targets; self.yank_cut = cut;
        self.tab_mut().selected.clear();
        self.msg(&format!("{} item(s) {}", n, if cut {"cut"} else {"copied"}), false);
    }
    fn paste_files(&mut self) {
        if self.yank.is_empty() { self.msg("Nothing to paste", true); return; }
        let dst      = self.tab().cwd.clone();
        let srcs     = self.yank.clone();
        let is_cut   = self.yank_cut;
        let label    = if is_cut { "Moving" } else { "Copying" }.to_string();

        // Calculate total bytes upfront so the progress bar has something to
        // work with. This is a fast directory walk — done on the UI thread
        // before spawning the worker so we can show an accurate total.
        let total: u64 = srcs.iter().map(|p| total_size(p)).sum();

        let (tx, rx)   = mpsc::channel::<ProgressUpdate>();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel_flag);

        // Spawn the worker thread.
        std::thread::spawn(move || {
            let _ = tx.send(ProgressUpdate::TotalBytes(total));
            let mut done: u64 = 0;
            for src in &srcs {
                if cancel_clone.load(Ordering::Relaxed) { break; }
                let target = dst.join(src.file_name().unwrap_or_default());
                let res: io::Result<()> = if is_cut {
                    // Try a cheap rename first (works on same filesystem).
                    // If the rename fails (cross-device), fall back to
                    // copy-then-delete so large moves still show progress.
                    fs::rename(src, &target).or_else(|_| {
                        if src.is_dir() {
                            copy_dir_progress(src, &target, &mut done, &tx, &cancel_clone)
                                .and_then(|_| fs::remove_dir_all(src))
                        } else {
                            copy_file_progress(src, &target, &mut done, &tx, &cancel_clone)
                                .and_then(|_| fs::remove_file(src))
                        }
                    })
                } else if src.is_dir() {
                    copy_dir_progress(src, &target, &mut done, &tx, &cancel_clone)
                } else {
                    copy_file_progress(src, &target, &mut done, &tx, &cancel_clone)
                };
                if let Err(e) = res {
                    if e.kind() == io::ErrorKind::Interrupted { break; }
                    let _ = tx.send(ProgressUpdate::Error(e.to_string()));
                    return;
                }
            }
            let _ = tx.send(ProgressUpdate::Done);
        });

        // Switch UI into progress mode.
        self.progress_rx       = Some(rx);
        self.progress_cancel   = Some(cancel_flag);
        self.progress_label    = label;
        self.progress_total    = total;
        self.progress_done     = 0;
        self.progress_current  = String::new();
        self.progress_finished = false;
        self.mode              = InputMode::Progress;

        // If this was a cut, clear the yank register immediately.
        if is_cut { self.yank.clear(); self.yank_cut = false; }
    }

    fn delete_files(&mut self) {
        let targets: Vec<PathBuf> = if !self.tab().selected.is_empty() {
            self.tab().selected.iter().cloned().collect()
        } else if let Some(p) = self.tab().current().cloned() { vec![p] }
        else { return; };

        let (tx, rx)    = mpsc::channel::<ProgressUpdate>();
        let cancel_flag  = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel_flag);

        std::thread::spawn(move || {
            for path in &targets {
                if cancel_clone.load(Ordering::Relaxed) { break; }
                if let Err(e) = delete_path_progress(path, &tx, &cancel_clone) {
                    if e.kind() != io::ErrorKind::Interrupted {
                        let _ = tx.send(ProgressUpdate::Error(e.to_string()));
                        return;
                    }
                    break;
                }
            }
            let _ = tx.send(ProgressUpdate::Done);
        });

        self.tab_mut().selected.clear();
        self.progress_rx       = Some(rx);
        self.progress_cancel   = Some(cancel_flag);
        self.progress_label    = "Deleting".to_string();
        self.progress_total    = 0; // delete doesn't track bytes
        self.progress_done     = 0;
        self.progress_current  = String::new();
        self.progress_finished = false;
        self.mode              = InputMode::Progress;
    }

    /// Drain pending progress messages from the worker thread.
    /// Call this from tick() every frame while in Progress mode.
    fn tick_progress(&mut self) {
        let rx = match &self.progress_rx { Some(r) => r, None => return };
        // Drain up to 256 messages per frame to keep the UI responsive.
        for _ in 0..256 {
            match rx.try_recv() {
                Ok(ProgressUpdate::TotalBytes(n))   => { self.progress_total   = n; }
                Ok(ProgressUpdate::BytesDone(n))    => { self.progress_done    = n; }
                Ok(ProgressUpdate::CurrentFile(f))  => { self.progress_current = f; }
                Ok(ProgressUpdate::Done) => {
                    self.progress_finished = true;
                    self.progress_rx     = None;
                    self.progress_cancel = None;
                    self.mode            = InputMode::Normal;
                    self.tab_mut().refresh();
                    self.msg(&format!("{} complete", self.progress_label), false);
                    break;
                }
                Ok(ProgressUpdate::Error(e)) => {
                    self.progress_finished = true;
                    self.progress_rx     = None;
                    self.progress_cancel = None;
                    self.mode            = InputMode::Normal;
                    self.tab_mut().refresh();
                    self.msg(&format!("{} error: {}", self.progress_label, e), true);
                    break;
                }
                Err(mpsc::TryRecvError::Empty)        => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.progress_rx     = None;
                    self.progress_cancel = None;
                    self.mode            = InputMode::Normal;
                    self.tab_mut().refresh();
                    break;
                }
            }
        }
    }
    fn open_current(&mut self) {
        let path = match self.tab().current().cloned() { Some(p) => p, None => return };
        if path.is_dir() { self.tab_mut().enter(); return; }

        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
        let ext = ext.as_str();
        let cfg = self.cfg.clone();

        // ── Shell scripts (.sh .bash .zsh .fish) ─────────────────────────────
        if SHELL_EXT.contains(&ext) {
            let interpreter = match ext {
                "bash" => "bash", "zsh" => "zsh", "fish" => "fish", _ => "sh",
            };
            let args = vec![interpreter.to_string(), path.to_string_lossy().into_owned()];
            let cwd  = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            self.input_buf.clear();
            self.mode = InputMode::RunArgs { args, cwd };
            return;
        }

        // ── Java archives (.jar) ──────────────────────────────────────────────
        if JAVA_EXT.contains(&ext) {
            let args = vec!["java".to_string(), "-jar".to_string(), path.to_string_lossy().into_owned()];
            let cwd  = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            self.input_buf.clear();
            self.mode = InputMode::RunArgs { args, cwd };
            return;
        }

        // ── Binary executables (unix +x bit, no recognised extension) ────────
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let is_exec = path.metadata()
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false);
            if is_exec && ext.is_empty() {
                let args = vec![path.to_string_lossy().into_owned()];
                let cwd  = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                self.input_buf.clear();
                self.mode = InputMode::RunArgs { args, cwd };
                return;
            }
        }

        // ── Everything else — delegate to the existing openers ───────────────
        // stdout/stderr are redirected to /dev/null so GUI apps (mpv, mirage,
        // etc.) don't spew status lines and GTK warnings into the TUI.
        let null = || std::process::Stdio::null();
        if IMAGE_EXT.contains(&ext) {
            let _ = Command::new(&cfg.opener_image).arg(&path)
                .stdout(null()).stderr(null()).spawn();
        } else if VIDEO_EXT.contains(&ext) {
            let _ = Command::new(&cfg.opener_video).arg(&path)
                .stdout(null()).stderr(null()).spawn();
        } else if AUDIO_EXT.contains(&ext) {
            let _ = Command::new(&cfg.opener_audio).arg(&path)
                .stdout(null()).stderr(null()).spawn();
        } else if DOC_EXT.contains(&ext) {
            let _ = Command::new(&cfg.opener_doc).arg(&path)
                .stdout(null()).stderr(null()).spawn();
        } else if ARCHIVE_EXT.contains(&ext) {
            self.extract_archive(&path.clone());
        } else {
            self.nvim_path = Some(path);
        }
    }

    /// Detect the best available terminal emulator by checking $TERM_PROGRAM,
    /// $TERMINAL, then probing a priority list of known binaries.
    /// Returns an empty string if we are on a TTY with no display server.
    fn detect_terminal() -> String {
        // Honour explicit environment overrides first.
        for var in &["TERMINAL", "TERM_PROGRAM"] {
            if let Ok(v) = std::env::var(var) {
                let v = v.trim().to_string();
                if !v.is_empty() && which(&v) { return v; }
            }
        }
        // Ordered preference list — common terminals across all DEs / WMs.
        // Wayland-native first, then X11, then universal fallbacks.
        const CANDIDATES: &[&str] = &[
            // Wayland-native
            "foot", "kitty", "alacritty", "wezterm",
            // Common across GNOME / KDE / XFCE / etc.
            "ghostty", "rio",
            "gnome-terminal", "kgx",            // GNOME
            "konsole",                           // KDE
            "xfce4-terminal",                    // XFCE
            "lxterminal",                        // LXDE
            "mate-terminal",                     // MATE
            "tilix", "terminator", "termite",
            // X11 universal fallbacks
            "urxvt", "rxvt", "xterm", "st",
            // Anything that speaks xterm protocol
            "xfce4-terminal",
        ];
        for bin in CANDIDATES {
            if which(bin) { return bin.to_string(); }
        }
        String::new() // TTY / no emulator found
    }

    /// Return the correct flag(s) that tell a terminal emulator to run a
    /// program.  The returned vec is inserted between the terminal binary
    /// and the program args.
    ///
    /// Most modern terminals use `--` but several older or DE-specific ones
    /// use `-e` or `-x`.
    fn terminal_exec_flags(bin: &str) -> Vec<&'static str> {
        // Extract just the binary name in case a full path was given.
        let name = bin.rsplit('/').next().unwrap_or(bin);
        match name {
            // These terminals use `-e` (execute) as their flag.
            "gnome-terminal" | "kgx"
            | "xfce4-terminal" | "lxterminal"
            | "mate-terminal" | "tilix"
            | "terminator"  | "xterm"
            | "urxvt" | "rxvt" | "st"
            | "sakura" | "pantheon-terminal"
            | "terminology" | "cool-retro-term"
                => vec!["-e"],

            // konsole uses -e but needs --noclose so the window stays open.
            "konsole" => vec!["--noclose", "-e"],

            // wezterm uses a subcommand.
            "wezterm" => vec!["start", "--"],

            // Everything else (kitty, foot, alacritty, ghostty, rio, termite,
            // wezterm-gui, …) uses the standard `--` separator.
            _ => vec!["--"],
        }
    }

    /// Returns true if we are running directly on a TTY with no display
    /// server available (i.e. no WAYLAND_DISPLAY and no DISPLAY).
    fn is_tty_only() -> bool {
        std::env::var("WAYLAND_DISPLAY").is_err() && std::env::var("DISPLAY").is_err()
    }

    /// Spawn `args` in a terminal window, or run them directly in the
    /// foreground if we are on a TTY.
    fn spawn_in_terminal(&mut self, terminal_cmd: &str, args: &[&str], cwd: &Path) {
        // ── TTY / no display server ──────────────────────────────────────────
        // There is no terminal emulator to spawn — run the program directly
        // in the foreground by suspending the TUI, waiting for the child to
        // finish, then restoring the TUI.
        if Self::is_tty_only() || terminal_cmd.is_empty() {
            if args.is_empty() { return; }
            // Restore the terminal to a sane state before handing it over.
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            let status = Command::new(args[0])
                .args(&args[1..])
                .current_dir(cwd)
                .status();
            // Re-enter TUI mode after the child exits.
            let _ = enable_raw_mode();
            let _ = execute!(io::stdout(), EnterAlternateScreen);
            match status {
                Ok(s)  => self.msg(&format!("Exited: {}", s), false),
                Err(e) => self.msg(&format!("Run error: {}", e), true),
            }
            return;
        }

        // ── GUI terminal emulator ────────────────────────────────────────────
        // Resolve which terminal binary to use.
        let resolved: String;
        let term_str: &str = if terminal_cmd.is_empty() {
            resolved = Self::detect_terminal();
            &resolved
        } else {
            terminal_cmd
        };

        // Split the configured command in case it has embedded flags,
        // e.g. "wezterm start" or "alacritty --class=launcher".
        let mut parts: Vec<&str> = term_str.split_whitespace().collect();
        if parts.is_empty() {
            self.msg("No terminal emulator found — set opener_terminal in settings", true);
            return;
        }

        let term_bin   = parts.remove(0);
        let exec_flags = Self::terminal_exec_flags(term_bin);

        let result = Command::new(term_bin)
            .args(&parts)        // any extra flags embedded in the config value
            .args(&exec_flags)   // -e / -- / start -- depending on the terminal
            .args(args)          // the program + its arguments
            .current_dir(cwd)
            .spawn();

        match result {
            Ok(_)  => self.msg(&format!("Running: {}", args.join(" ")), false),
            Err(e) => {
                // If the configured terminal failed, try auto-detection once
                // before giving up.
                if terminal_cmd == term_bin {
                    let fallback = Self::detect_terminal();
                    if !fallback.is_empty() && fallback != term_bin {
                        let flags = Self::terminal_exec_flags(&fallback);
                        let res2  = Command::new(&fallback)
                            .args(&flags)
                            .args(args)
                            .current_dir(cwd)
                            .spawn();
                        if res2.is_ok() {
                            self.msg(&format!("Running via {}: {}", fallback, args.join(" ")), false);
                            return;
                        }
                    }
                }
                self.msg(&format!("Terminal error: {}", e), true);
            }
        }
    }
    fn extract_archive(&mut self, path: &Path) {
        let dst = path.parent().unwrap_or(Path::new("."));
        let parts: Vec<&str> = self.cfg.opener_archive.split_whitespace().collect();
        let null = std::process::Stdio::null;
        let res = if parts.len() >= 2 {
            Command::new(parts[0]).args(&parts[1..]).arg(path).arg("--dir").arg(dst)
                .stdout(null()).stderr(null()).spawn()
        } else if which("tar") {
            Command::new("tar").args(["xf", &path.to_string_lossy(), "-C", &dst.to_string_lossy()])
                .stdout(null()).stderr(null()).spawn()
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "no extractor"))
        };
        match res { Ok(_) => self.msg("Extracting\u{2026}", false), Err(e) => self.msg(&e.to_string(), true) }
    }
    fn new_tab(&mut self) {
        let cwd = self.tab().cwd.clone(); let sh = self.cfg.show_hidden;
        self.tabs.push(Tab::new(cwd, sh));
        self.tab_idx = self.tabs.len() - 1;
        self.msg(&format!("Tab {} opened", self.tab_idx+1), false);
    }
    fn close_tab(&mut self) {
        if self.tabs.len() == 1 { self.msg("Can't close last tab", true); return; }
        self.tabs.remove(self.tab_idx);
        self.tab_idx = self.tab_idx.min(self.tabs.len()-1);
    }
    fn open_fuzzy(&mut self) {
        self.fuzzy_query.clear();
        self.fuzzy_cursor  = 0;
        self.fuzzy_index   = vec![];
        self.fuzzy_results = vec![];
        self.fuzzy_loading = true;
        self.mode          = InputMode::FuzzySearch;

        let cwd = self.tab().cwd.clone();
        let sh  = self.tab().show_hidden;
        let (tx, rx) = mpsc::channel();
        self.fuzzy_rx = Some(rx);

        std::thread::spawn(move || {
            collect_all_streaming(&cwd, &tx, 0, sh);
        });
    }
    fn fuzzy_update_results(&mut self) {
        let q = self.fuzzy_query.to_lowercase();
        if q.is_empty() { self.fuzzy_results = self.fuzzy_index.clone(); }
        else {
            let mut scored: Vec<(i32, &PathBuf)> = self.fuzzy_index.iter().filter_map(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                let s = fuzzy_score(&name, &q); if s > 0 { Some((s, p)) } else { None }
            }).collect();
            scored.sort_by(|a,b| b.0.cmp(&a.0));
            self.fuzzy_results = scored.into_iter().map(|(_,p)| p.clone()).collect();
        }
        // Only reset cursor if it's now out of bounds — don't clobber it during streaming
        if self.fuzzy_cursor >= self.fuzzy_results.len() {
            self.fuzzy_cursor = 0;
        }
    }
    fn fuzzy_accept(&mut self) {
        if let Some(path) = self.fuzzy_results.get(self.fuzzy_cursor).cloned() {
            self.mode = InputMode::Normal;
            let dir = if path.is_dir() { path.clone() }
                      else { path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| self.tab().cwd.clone()) };
            self.tabs[self.tab_idx].cwd = dir;
            self.tabs[self.tab_idx].search_query.clear();
            self.tabs[self.tab_idx].search_results = None;
            self.tabs[self.tab_idx].refresh();
            if let Some(i) = self.tab().entries.iter().position(|e| *e == path) {
                self.tab_mut().state.select(Some(i));
            }
            self.fuzzy_query.clear(); self.fuzzy_results.clear();
        }
    }
}


fn collect_all_streaming(dir: &Path, tx: &mpsc::Sender<PathBuf>, depth: usize, show_hidden: bool) {
    if depth > 6 { return; }
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !show_hidden && name.starts_with('.') { continue; }
            if tx.send(p.clone()).is_err() { return; }
            if p.is_dir() && !p.is_symlink() {
                collect_all_streaming(&p, tx, depth+1, show_hidden);
            }
        }
    }
}

fn fuzzy_score(hay: &str, needle: &str) -> i32 {
    if needle.is_empty() { return 1; }
    if hay == needle { return 1000; }
    if hay.contains(needle) { return 500 + (100 - hay.len().min(100)) as i32; }
    let hc: Vec<char> = hay.chars().collect();
    let nc: Vec<char> = needle.chars().collect();
    let mut hi=0; let mut score=0i32; let mut cons=0i32;
    for nc in &nc {
        let mut matched = false;
        while hi < hc.len() {
            if hc[hi] == *nc { score += 10+cons*5; cons+=1; hi+=1; matched=true; break; }
            cons=0; hi+=1;
        }
        if !matched { return 0; }
    }
    score
}

// ─── UI ───────────────────────────────────────────────────────────────────────
fn ui(f: &mut Frame, app: &mut App) {
    // ratatui 0.29: f.size()  |  ratatui 0.30+: f.area()
    #[allow(deprecated)]
    let sz = f.size();

    if app.mode == InputMode::Settings {
        draw_settings(f, app, sz);
        return;
    }

    // Layout: tab bar (1) | body (flex) | status (1)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(sz);

    draw_tab_bar(f, app, rows[0]);
    draw_body(f, app, rows[1]);
    draw_status_bar(f, app, rows[2]);

    // Overlays drawn last
    match &app.mode {
        InputMode::FuzzySearch => draw_fuzzy_overlay(f, app, sz),
        InputMode::Rename(_) | InputMode::NewFile | InputMode::NewDir => draw_input_overlay(f, app, sz),
        InputMode::Confirm   => draw_confirm_overlay(f, app, sz),
        InputMode::Progress  => draw_progress_overlay(f, app, sz),
        InputMode::RunArgs{..}=> draw_run_args_overlay(f, app, sz),
        InputMode::Help      => draw_help_overlay(f, app, sz),
        _ => {}
    }
}

fn draw_tab_bar(f: &mut Frame, app: &App, rect: Rect) {
    let titles: Vec<Line> = app.tabs.iter().enumerate().map(|(i, tab)| {
        let name = tab.cwd.file_name().and_then(|n| n.to_str()).unwrap_or("/");
        let label = format!(" {} {} ", i+1, name);
        if i == app.tab_idx {
            Line::from(Span::styled(label, bold_bg(app.theme.tab_active_fg, app.theme.tab_active_bg)))
        } else {
            Line::from(Span::styled(label, st_bg(app.theme.tab_inactive_fg, app.theme.tab_inactive_bg)))
        }
    }).collect();
    let tabs = Tabs::new(titles)
        .select(app.tab_idx)
        .style(st_bg(app.theme.tab_inactive_fg, app.theme.tab_inactive_bg))
        .divider(Span::raw(""));
    f.render_widget(tabs, rect);
}

fn draw_body(f: &mut Frame, app: &mut App, rect: Rect) {
    let pct_preview = 100u16
        .saturating_sub(app.cfg.col_parent)
        .saturating_sub(app.cfg.col_files);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.cfg.col_parent),
            Constraint::Percentage(app.cfg.col_files),
            Constraint::Percentage(pct_preview),
        ])
        .split(rect);

    draw_parent_pane(f, app, cols[0]);
    draw_files_pane(f, app, cols[1]);
    draw_preview_pane(f, app, cols[2]);
}

fn draw_parent_pane(f: &mut Frame, app: &App, rect: Rect) {
    let tab    = app.tab();
    let parent = tab.cwd.parent().unwrap_or(&tab.cwd);
    let entries = list_dir(parent, tab.show_hidden);
    let h = rect.height as usize;

    let items: Vec<ListItem> = entries.iter().take(h).map(|e| {
        let k    = file_kind(e);
        let fg   = kind_color(&k, &app.theme);
        let ic   = &get_icon(e, &k, &app.icon_set, &app.user_icons);
        let name = e.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let is_cur = *e == tab.cwd;
        let(fg, bg) = if is_cur { (app.theme.cursor_fg, app.theme.cursor_bg) } else { (fg, app.theme.panel_bg) };
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {} ", ic), st_bg(fg, bg)),
            Span::styled(name, st_bg(fg, bg)),
        ]))
    }).collect();

    let block = Block::default().style(st_bg(app.theme.text, app.theme.base));
    f.render_widget(List::new(items).block(block), rect);
}

fn draw_files_pane(f: &mut Frame, app: &mut App, rect: Rect) {
    let tab     = app.tab();
    let visible = tab.visible().to_vec();
    let h       = rect.height as usize;

    // Sync scroll
    {
        let tab = app.tab_mut();
        let cur = tab.state.selected().unwrap_or(0);
        if cur < tab.scroll { tab.scroll = cur; }
        if cur >= tab.scroll + h { tab.scroll = cur + 1 - h; }
    }

    let tab    = app.tab();
    let scroll = tab.scroll;

    let items: Vec<ListItem> = visible.iter().enumerate()
        .skip(scroll).take(h)
        .map(|(i, e)| {
            let k    = file_kind(e);
            let fg   = kind_color(&k, &app.theme);
            let ic   = &get_icon(e, &k, &app.icon_set, &app.user_icons);
            let name = e.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let size = file_size_str(e);
            let is_cur = tab.state.selected() == Some(i);
            let is_sel = tab.selected.contains(e);
            let (fg, bg) = if is_cur { (app.theme.cursor_fg, app.theme.cursor_bg) }
                           else if is_sel { (app.theme.selected_fg, app.theme.selected_bg) }
                           else { (fg, app.theme.base) };
            let max_name = (rect.width.saturating_sub(9)) as usize;
            let name_clipped: String = name.chars().take(max_name).collect();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", ic), st_bg(fg, bg)),
                Span::styled(name_clipped, st_bg(fg, bg)),
                Span::styled(format!(" {:>6}", size), st_bg(if is_cur { app.theme.cursor_fg } else { app.theme.popup_dim_fg }, bg)),
            ]))
        }).collect();

    let title = {
        let name = tab.cwd.file_name().and_then(|n| n.to_str()).unwrap_or("/");
        format!(" \u{f07b} {} ", name)
    };
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(st(app.theme.border_fg))
        .title(Span::styled(title, bold(app.theme.title_fg)))
        .style(st_bg(app.theme.text, app.theme.panel_bg));

    let mut state = app.tab().state.clone();
    f.render_stateful_widget(List::new(items).block(block), rect, &mut state);
}

fn draw_preview_pane(f: &mut Frame, app: &mut App, rect: Rect) {
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(rect);

    // Record actual preview size for image caching
    app.last_preview_size = (inner[1].width, inner[1].height);

    let tab = app.tab();
    let current = match tab.current() { Some(p) => p.clone(), None => {
        let b = Block::default().style(st_bg(app.theme.text, app.theme.base));
        f.render_widget(b, rect);
        return;
    }};

    let k    = file_kind(&current);
    let c    = kind_color(&k, &app.theme);
    let name = current.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let ext  = current.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();

    // Header
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!(" {} ", &get_icon(&current, &k, &app.icon_set, &app.user_icons)), st_bg(c, app.theme.base)),
            Span::styled(name, bold_bg(c, app.theme.base)),
        ]),
        Line::from(vec![
            Span::styled(format!("  {}   {}", file_size_str(&current), format_mtime(&current, &app.cfg.date_format)), st_bg(app.theme.popup_dim_fg, app.theme.panel_bg)),
        ]),
    ]).style(st_bg(app.theme.text, app.theme.base));
    f.render_widget(header, inner[0]);

    let content_rect = inner[1];
    let ch = content_rect.height as usize;

    // Image preview
    if IMAGE_EXT.contains(&ext.as_str()) {
        let needs_load = app.img_path.as_deref() != Some(&current);
        if needs_load {
            app.img_path  = Some(current.clone());
            app.img_state = None;
            if let Some(picker) = app.img_picker.as_mut() {
                if let Ok(img) = image::open(&current) {
                    app.img_state = Some(picker.new_resize_protocol(img));
                }
            }
        }
        if let Some(state) = app.img_state.as_mut() {
            StatefulImage::new().render(content_rect, f.buffer_mut(), state);
        } else {
            let p = Paragraph::new(Span::styled(
                "\u{f03e}  (terminal does not support graphics protocol)",
                st_bg(app.theme.popup_dim_fg, app.theme.panel_bg),
            ));
            f.render_widget(p, content_rect);
        }
        return;
    }

    // Video preview — extract thumbnail via ffmpeg on a background thread.
    // Re-extract if video changed OR pane width changed (better resolution fit).
    let pane_changed = app.last_preview_size.0 != content_rect.width;
    if VIDEO_EXT.contains(&ext.as_str()) {
        let needs_thumb = app.thumb_src.as_deref() != Some(&current) || pane_changed;
        if needs_thumb {
            app.thumb_rx   = None;
            app.thumb_meta = None;
            if let Some(old) = app.thumb_tmp.take() { let _ = fs::remove_file(old); }
            app.thumb_src  = Some(current.clone());
            app.img_state  = None;
            app.img_path   = None;

            if which("ffmpeg") {
                let tmp = std::env::temp_dir()
                    .join(format!("fd-files-thumb-{}.jpg", std::process::id()));
                let src  = current.clone();
                let tmp2 = tmp.clone();
                // Scale thumbnail to fit within the pane's pixel dimensions.
                // Use conservative cell sizes (8×16) so it never overflows.
                // force_original_aspect_ratio=decrease ensures it fits in the box.
                let pw = ((content_rect.width  as u32) * 8) & !1; // round to even
                let ph = ((content_rect.height as u32) * 16) & !1;
                let scale = format!(
                    "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
                    pw.max(80), ph.max(80), pw.max(80), ph.max(80)
                );
                let (tx, rx) = mpsc::channel();
                app.thumb_rx = Some(rx);
                std::thread::spawn(move || {
                    let ok = Command::new("ffmpeg")
                        .args(["-y", "-loglevel", "error",
                               "-ss", "00:00:03",
                               "-i", &src.to_string_lossy(),
                               "-vframes", "1",
                               "-vf", &scale,
                               tmp2.to_str().unwrap_or("")])
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false);
                    let _ = tx.send(if ok && tmp2.exists() { Some(tmp2) } else { None });
                });
            }

            // Run ffprobe once and cache — do it here, not in draw loop
            if which("ffprobe") {
                app.thumb_meta = Command::new("ffprobe")
                    .args(["-v", "quiet", "-print_format", "json",
                           "-show_format", "-show_streams",
                           &current.to_string_lossy()])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|json| {
                        let duration = json.split("\"duration\":").nth(1)
                            .and_then(|s| s.split('"').nth(1)).unwrap_or("?");
                        let codec = json.split("\"codec_name\":").nth(1)
                            .and_then(|s| s.split('"').nth(1)).unwrap_or("?");
                        let width = json.split("\"width\":").nth(1)
                            .and_then(|s| s.split([',','}']).next())
                            .map(|s| s.trim()).unwrap_or("?");
                        let height = json.split("\"height\":").nth(1)
                            .and_then(|s| s.split([',','}']).next())
                            .map(|s| s.trim()).unwrap_or("?");
                        let secs: f64 = duration.parse().unwrap_or(0.0);
                        let dur_str = if secs > 0.0 {
                            format!("{:02}:{:02}:{:02}",
                                secs as u64 / 3600,
                                (secs as u64 % 3600) / 60,
                                secs as u64 % 60)
                        } else { duration.to_string() };
                        format!("  codec:    {}\n  size:     {}×{}\n  duration: {}", codec, width, height, dur_str)
                    });
            }
        }

        if let Some(state) = app.img_state.as_mut() {
            // Constrain to 80% of pane width to ensure the image stays within
            // the preview pane regardless of terminal cell pixel size.
            let img_w = (content_rect.width * 4 / 5).max(1);
            let pad   = content_rect.width.saturating_sub(img_w) / 2;
            let cols  = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(pad),
                    Constraint::Length(img_w),
                    Constraint::Min(0),
                ])
                .split(content_rect);
            StatefulImage::new().render(cols[1], f.buffer_mut(), state);
        } else {
            let status_line = if app.thumb_rx.is_some() {
                "  \u{f03d}  extracting thumbnail\u{2026}"
            } else if which("ffmpeg") {
                "  \u{f03d}  (thumbnail unavailable)"
            } else {
                "  \u{f03d}  install ffmpeg for thumbnails"
            };
            let mut lines: Vec<Line> = vec![
                Line::from(Span::styled(status_line, bold(app.theme.color_video))),
                Line::from(Span::raw("")),
            ];
            if let Some(meta) = &app.thumb_meta {
                lines.extend(meta.lines().map(|l|
                    Line::from(Span::styled(l.to_string(), st(app.theme.popup_dim_fg)))
                ));
            } else if !which("ffprobe") {
                lines.push(Line::from(Span::styled(
                    "  install ffprobe for metadata",
                    st(app.theme.popup_dim_fg),
                )));
            }
            f.render_widget(Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.panel_bg)), content_rect);
        }
        return;
    }
    if current.is_dir() {
        let entries = list_dir(&current, app.cfg.show_hidden);
        let items: Vec<ListItem> = entries.iter().take(ch).map(|e| {
            let ek = file_kind(e);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", &get_icon(e, &ek, &app.icon_set, &app.user_icons)), st(kind_color(&ek, &app.theme))),
                Span::styled(e.file_name().unwrap_or_default().to_string_lossy().to_string(), st(kind_color(&ek, &app.theme))),
            ]))
        }).collect();
        let block = Block::default().style(st_bg(app.theme.text, app.theme.base));
        f.render_widget(List::new(items).block(block), content_rect);
        return;
    }

    // Text preview
    if let Ok(content) = fs::read_to_string(&current) {
        let lines: Vec<Line> = content.lines().take(ch).map(|l| {
            let clipped: String = l.chars().take(content_rect.width as usize).collect();
            Line::from(Span::styled(clipped, st(app.theme.popup_dim_fg)))
        }).collect();
        let p = Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.base));
        f.render_widget(p, content_rect);
    } else {
        let p = Paragraph::new("(binary file)").style(st_bg(app.theme.popup_dim_fg, app.theme.panel_bg));
        f.render_widget(p, content_rect);
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, rect: Rect) {
    let tab = app.tab();
    let home = dirs_home().to_string_lossy().to_string();
    let cwd  = tab.cwd.to_string_lossy().replace(&home, "~");
    let cur  = tab.state.selected().map(|i| i+1).unwrap_or(0);
    let total = tab.visible().len();

    let mut spans = vec![
        Span::styled(format!("  {}  ", cwd), bold_bg(app.theme.status_path_fg, app.theme.status_path_bg)),
    ];
    if !app.status_msg.is_empty() {
        let (fg, bg) = if app.status_err { (app.theme.status_err_fg, app.theme.status_err_bg) } else { (app.theme.status_msg_fg, app.theme.status_msg_bg) };
        spans.push(Span::styled(format!("  {}  ", app.status_msg), bold_bg(fg, bg)));
    }
    if !app.yank.is_empty() {
        spans.push(Span::styled(format!("  \u{f0c5} {}  ", app.yank.len()), bold_bg(app.theme.status_yank_fg, app.theme.status_yank_bg)));
    }
    if !tab.selected.is_empty() {
        spans.push(Span::styled(format!("  \u{f14a} {}  ", tab.selected.len()), bold_bg(app.theme.status_sel_fg, app.theme.status_sel_bg)));
    }
    // Right-align: ? hint then count
    let hint_str = " ?:help ";
    let count_str = format!("  {}/{}", cur, total);
    let used: usize = spans.iter().map(|s| s.content.len()).sum();
    let right_len = hint_str.len() + count_str.len();
    let pad = (rect.width as usize).saturating_sub(used + right_len);
    spans.push(Span::styled(" ".repeat(pad), st_bg(app.theme.status_bar_fg, app.theme.status_bar_bg)));
    spans.push(Span::styled(hint_str, bold_bg(app.theme.status_hint_fg, app.theme.status_bar_bg)));
    spans.push(Span::styled(count_str, st_bg(app.theme.status_bar_fg, app.theme.status_bar_bg)));

    let bar = Paragraph::new(Line::from(spans)).style(st_bg(app.theme.status_bar_fg, app.theme.status_bar_bg));
    f.render_widget(bar, rect);
}

fn draw_help_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let c = &app.cfg;
    let pairs: &[(&str, &str)] = &[
        ("↕ / ↔",              "navigate / enter dir"),
        ("BS",                  "go up"),
        ("↩",                   "open file"),
        (&c.key_select,         "select"),
        (&c.key_select_all,     "select all"),
        (&c.key_copy,           "copy selected"),
        (&c.key_cut,            "cut selected"),
        (&c.key_paste,          "paste"),
        (&c.key_delete,         "delete selected"),
        (&c.key_rename,         "rename"),
        (&c.key_new_file,       "new file"),
        (&c.key_new_dir,        "new directory"),
        (&c.key_search,         "fuzzy search"),
        (&c.key_toggle_hidden,  "toggle hidden files"),
        (&c.key_new_tab,        "new tab"),
        (&c.key_close_tab,      "close tab"),
        (&c.key_switch_tab,     "switch tab"),
        (&c.key_quit,           "quit"),
        (":",                   "settings"),
        ("?",                   "this help"),
    ];

    let w = (rect.width * 2 / 3).max(52).min(rect.width.saturating_sub(4));
    let h = (pairs.len() as u16 + 4).min(rect.height.saturating_sub(2));
    let x = (rect.width  - w) / 2;
    let y = (rect.height - h) / 2;
    let popup = Rect { x, y, width: w, height: h };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.popup_border_fg))
        .title(Span::styled("  ? Help  — any key to close  ", bold(app.theme.popup_border_fg)))
        .style(st_bg(app.theme.popup_text_fg, app.theme.popup_bg));
    f.render_widget(block, popup);

    let inner = Rect {
        x: popup.x + 2, y: popup.y + 1,
        width: popup.width.saturating_sub(4),
        height: popup.height.saturating_sub(2),
    };

    let col_w = (inner.width / 2) as usize;
    let rows_available = inner.height as usize;
    let lines: Vec<Line> = pairs.iter().take(rows_available).map(|(key, desc)| {
        let key_col  = format!("{:<12}", key);
        let desc_col = format!("{}", desc);
        // Truncate if needed
        let total = col_w.saturating_sub(1);
        let desc_trunc = if desc_col.len() > total { &desc_col[..total] } else { &desc_col };
        Line::from(vec![
            Span::styled(key_col,  bold(app.theme.popup_border_fg)),
            Span::styled(format!(" {}", desc_trunc), st(app.theme.popup_text_fg)),
        ])
    }).collect();

    f.render_widget(
        Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.base)),
        inner,
    );
}

fn draw_settings(f: &mut Frame, app: &App, rect: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.popup_border_fg))
        .title(Span::styled(
            "  \u{f013} Settings  [\u{2190}\u{2192} sections  \u{2191}\u{2193} navigate  Enter edit  S save  Esc close]  ",
            bold_bg(app.theme.tab_inactive_fg, app.theme.panel_bg),
        ))
        .style(st_bg(app.theme.text, app.theme.panel_bg));
    f.render_widget(block, rect);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .margin(1)
        .split(rect);

    // Section tabs
    let sections = [
        (SettingsSection::Behaviour,  "Behaviour"),
        (SettingsSection::Appearance, "Appearance"),
        (SettingsSection::Openers,    "Openers"),
        (SettingsSection::Keybinds,   "Keybinds"),
    ];
    let titles: Vec<Line> = sections.iter().map(|(sec, label)| {
        let s = format!("  {}  ", label);
        if *sec == app.settings.section { Line::from(Span::styled(s, bold_bg(app.theme.settings_cursor_fg, app.theme.settings_cursor_bg))) }
        else { Line::from(Span::styled(s, st_bg(app.theme.tab_inactive_fg, app.theme.tab_inactive_bg))) }
    }).collect();
    let tabs = Tabs::new(titles)
        .select(match app.settings.section {
            SettingsSection::Behaviour  => 0, SettingsSection::Appearance => 1,
            SettingsSection::Openers    => 2, SettingsSection::Keybinds   => 3,
        })
        .style(st_bg(app.theme.tab_inactive_fg, app.theme.tab_inactive_bg))
        .divider(Span::raw(""));
    f.render_widget(tabs, inner[0]);

    // Items
    let items = SettingsState::section_items(&app.settings.section);
    let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, (key, label))| {
        let val = SettingsState::get_value(key, &app.cfg);
        let is_cur = i == app.settings.cursor;
        let val_display = if app.settings.editing && is_cur {
            format!("{}\u{2588}", app.settings.edit_buf)
        } else { val };
        let (lbg, vbg) = if is_cur { (app.theme.surface0, app.theme.border_fg) } else { (app.theme.panel_bg, app.theme.panel_bg) };
        let (lfg, vfg) = if is_cur { (app.theme.settings_cursor_fg, app.theme.settings_cursor_bg) } else { (app.theme.settings_key_fg, app.theme.settings_val_fg) };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:30}", label), st_bg(lfg, lbg)),
            Span::styled(format!("  {}  ", val_display), st_bg(vfg, vbg)),
        ]))
    }).collect();

    let mut state = ListState::default();
    state.select(Some(app.settings.cursor));
    f.render_stateful_widget(List::new(list_items).style(st_bg(app.theme.text, app.theme.base)), inner[1], &mut state);

    // Unsaved indicator
    if app.settings.dirty && !app.settings.dropdown {
        let hint = Paragraph::new("  Unsaved changes — press S to save")
            .style(st_bg(app.theme.warn_fg, app.theme.warn_bg));
        f.render_widget(hint, inner[2]);
    }

    // ── Dropdown overlay ───────────────────────────────────────────────────────
    if app.settings.dropdown {
        let items  = SettingsState::section_items(&app.settings.section);
        let (k, _) = items[app.settings.cursor];
        let opts   = SettingsState::dropdown_options(k, &app.user_themes, &app.user_icons).unwrap_or_default();

        let dw = 36u16;
        let dh = (opts.len() as u16 + 2).min(rect.height - 4);
        // Position dropdown below the currently selected row
        let row_y = inner[1].y + app.settings.cursor as u16;
        let dy = (row_y + 1).min(rect.bottom().saturating_sub(dh));
        // Align to the value column (label is ~34 chars wide)
        let dx = inner[1].x + 34;
        let dx = dx.min(rect.right().saturating_sub(dw));
        let popup = Rect { x: dx, y: dy, width: dw, height: dh };
        f.render_widget(Clear, popup);

        let dd_items: Vec<ListItem> = opts.iter().enumerate().map(|(i, opt)| {
            let is_cur = i == app.settings.dd_cursor;
            let (fg, bg) = if is_cur {
                (app.theme.settings_cursor_fg, app.theme.settings_cursor_bg)
            } else {
                (app.theme.popup_text_fg, app.theme.popup_bg)
            };
            ListItem::new(Line::from(vec![
                Span::styled(if is_cur { " \u{f0da} " } else { "   " }, st_bg(fg, bg)),
                Span::styled(opt.clone(), st_bg(fg, bg)),
            ]))
        }).collect();

        let dd_block = Block::default()
            .borders(Borders::ALL)
            .border_style(bold(app.theme.popup_border_fg))
            .title(Span::styled(" \u{2191}\u{2193} select  Enter confirm  Esc cancel ", st(app.theme.popup_dim_fg)))
            .style(st_bg(app.theme.popup_text_fg, app.theme.popup_bg));

        let mut dd_state = ListState::default();
        dd_state.select(Some(app.settings.dd_cursor));
        f.render_stateful_widget(
            List::new(dd_items).block(dd_block),
            popup,
            &mut dd_state,
        );
    }
}

fn draw_input_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let prompt = match &app.mode {
        InputMode::Rename(_) => "\u{f040} Rename",
        InputMode::NewFile   => "\u{f15b} New File",
        InputMode::NewDir    => "\u{f07b} New Directory",
        _ => "",
    };
    let w = (rect.width / 2).max(40);
    let h = 3u16;
    let x = (rect.width - w) / 2;
    let y = rect.height / 2;
    let popup = Rect { x, y, width: w, height: h };
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.popup_border_fg))
        .style(st_bg(app.theme.popup_text_fg, app.theme.popup_bg));
    let text = format!(" {}: {}\u{2588}", prompt, app.input_buf);
    let p = Paragraph::new(text).block(block).style(bold(app.theme.text));
    f.render_widget(p, popup);
}

fn draw_run_args_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let base_cmd = match &app.mode {
        InputMode::RunArgs { args, .. } => args.join(" "),
        _ => return,
    };

    let w = (rect.width * 2 / 3).max(50).min(rect.width.saturating_sub(4));
    let h = 5u16;
    let x = (rect.width  - w) / 2;
    let y = (rect.height - h) / 2;
    let popup = Rect { x, y, width: w, height: h };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.popup_border_fg))
        .title(Span::styled("  \u{f489} Run  ", bold(app.theme.popup_border_fg)))
        .style(st_bg(app.theme.popup_text_fg, app.theme.popup_bg));
    f.render_widget(block, popup);

    let inner = Rect {
        x: popup.x + 1, y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // prefix input line
            Constraint::Length(1), // base command (greyed, not editable)
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Top row: what the user types — goes BEFORE the command
    // Empty = run as-is.  "mangohud" = mangohud ./game
    let prefix_display = format!(" {}\u{2588}", app.input_buf);
    f.render_widget(
        Paragraph::new(prefix_display).style(bold(app.theme.popup_text_fg)),
        rows[0],
    );

    // Middle row: the fixed base command, dimmed
    let max_base = inner.width.saturating_sub(2) as usize;
    let base_display = if base_cmd.len() > max_base {
        format!(" …{}", &base_cmd[base_cmd.len() - max_base..])
    } else {
        format!(" {}", base_cmd)
    };
    f.render_widget(
        Paragraph::new(base_display).style(st(app.theme.popup_dim_fg)),
        rows[1],
    );

    // Hint
    f.render_widget(
        Paragraph::new("  Enter — run    Esc — cancel  ").style(st(app.theme.popup_dim_fg)),
        rows[2],
    );
}

fn draw_confirm_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let msg = "  Delete selected items? [y/N]  ";
    let w = msg.len() as u16;
    let h = 1u16;
    let x = (rect.width - w) / 2;
    let y = rect.height / 2;
    let popup = Rect { x, y, width: w, height: h };
    f.render_widget(Clear, popup);
    let p = Paragraph::new(msg).style(bold_bg(app.theme.status_err_fg, app.theme.status_err_bg));
    f.render_widget(p, popup);
}

fn draw_progress_overlay(f: &mut Frame, app: &App, rect: Rect) {
    // ── Popup dimensions ─────────────────────────────────────────────────────
    let w  = (rect.width * 3 / 4).max(50).min(rect.width.saturating_sub(4));
    let h  = 7u16;
    let x  = (rect.width  - w) / 2;
    let y  = (rect.height - h) / 2;
    let popup = Rect { x, y, width: w, height: h };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.popup_border_fg))
        .title(Span::styled(
            format!("  {}…  ", app.progress_label),
            bold(app.theme.popup_border_fg),
        ));
    f.render_widget(block, popup);

    // Inner area (inside the border)
    let inner = Rect {
        x: popup.x + 1, y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };
    let bar_width = inner.width.saturating_sub(2) as u64;

    // ── Progress bar ─────────────────────────────────────────────────────────
    let (filled, pct_str) = if app.progress_total > 0 {
        let ratio   = (app.progress_done as f64 / app.progress_total as f64).min(1.0);
        let filled  = (ratio * bar_width as f64) as u64;
        let pct     = (ratio * 100.0) as u32;
        (filled, format!(" {}% ", pct))
    } else {
        // Delete operations have no byte total — show a spinner-style fill
        // that grows while the operation runs.
        (bar_width / 2, " … ".to_string())
    };

    let bar_line = Line::from(vec![
        Span::styled("[",                                st(app.theme.popup_dim_fg)),
        Span::styled("█".repeat(filled as usize),       bold(app.theme.progress_filled_fg)),
        Span::styled("░".repeat((bar_width - filled) as usize), st(app.theme.progress_empty_fg)),
        Span::styled("]",                                st(app.theme.popup_dim_fg)),
    ]);

    // ── Size counters ─────────────────────────────────────────────────────────
    let size_str = if app.progress_total > 0 {
        format!(
            "  {}  /  {}  {}",
            human_size(app.progress_done),
            human_size(app.progress_total),
            pct_str,
        )
    } else {
        format!("  {}  deleted", human_size(app.progress_done))
    };

    // ── Current filename ──────────────────────────────────────────────────────
    let max_name = inner.width.saturating_sub(4) as usize;
    let name_str = if app.progress_current.len() > max_name {
        format!("  …{}", &app.progress_current[app.progress_current.len() - max_name..])
    } else {
        format!("  {}", app.progress_current)
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // bar
            Constraint::Length(1), // size
            Constraint::Length(1), // filename
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(bar_line),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(size_str).style(st(app.theme.popup_text_fg)),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(name_str).style(st(app.theme.popup_dim_fg)),
        rows[2],
    );

    // Hint at the bottom of the popup
    let hint = "  Esc — cancel  ";
    let hint_rect = Rect {
        x: popup.x + popup.width.saturating_sub(hint.len() as u16 + 1),
        y: popup.y + popup.height - 1,
        width: hint.len() as u16,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(hint).style(st(app.theme.popup_dim_fg)),
        hint_rect,
    );
}

fn draw_fuzzy_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let ow = (rect.width * 7 / 8).max(40).min(rect.width);
    let oh = (rect.height * 4 / 5).max(10).min(rect.height);
    let ox = (rect.width  - ow) / 2;
    let oy = (rect.height - oh) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(popup);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.popup_border_fg))
        .title(Span::styled(
            "  \u{f422} Fuzzy Find  [Esc cancel  \u{2191}\u{2193} navigate  Enter jump]  ",
            bold(app.theme.tab_inactive_fg),
        ))
        .style(st_bg(app.theme.text, app.theme.base));

    // Live count — always shows current number even while streaming
    let count_str = if app.fuzzy_loading {
        format!("  {} matches, indexing\u{2026}", app.fuzzy_results.len())
    } else {
        format!("  {} matches", app.fuzzy_results.len())
    };
    let search_text = Line::from(vec![
        Span::styled("  ", st(app.theme.popup_dim_fg)),
        Span::styled(&app.fuzzy_query, bold(app.theme.text)),
        Span::styled("\u{2588}", bold(app.theme.popup_border_fg)),
        Span::styled(&count_str, st(app.theme.popup_dim_fg)),
    ]);
    f.render_widget(Paragraph::new(search_text).block(search_block), inner[0]);

    // Subtract 1 for bottom border to keep cursor from hiding underneath it
    let list_h = (inner[1].height as usize).saturating_sub(1);
    let cursor = app.fuzzy_cursor;
    // Keep cursor visible — scroll so cursor is always within the visible window
    let scroll = if cursor >= list_h { cursor + 1 - list_h } else { 0 };

    let items: Vec<ListItem> = app.fuzzy_results.iter().enumerate()
        .skip(scroll).take(list_h)
        .map(|(i, path)| {
            let k    = file_kind(path);
            let c    = kind_color(&k, &app.theme);
            let ic   = &get_icon(path, &k, &app.icon_set, &app.user_icons);
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let rel  = path.strip_prefix(&app.tab().cwd)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            let is_cur = i == cursor;  // i is the absolute index, cursor is absolute — correct
            let ns = if is_cur { bold_bg(app.theme.cursor_fg, app.theme.cursor_bg) } else { st(c) };
            let ps = if is_cur { st_bg(app.theme.popup_dim_fg, app.theme.cursor_bg) } else { st(app.theme.popup_dim_fg) };
            let prefix = if is_cur { " \u{f0da} " } else { "   " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, if is_cur { bold(app.theme.popup_border_fg) } else { st(app.theme.border_fg) }),
                Span::styled(format!("{} ", ic), ns),
                Span::styled(name, ns),
                Span::styled(format!("  {}", rel), ps),
            ]))
        }).collect();

    let list_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(st(app.theme.border_fg))
        .style(st_bg(app.theme.text, app.theme.panel_bg));
    f.render_widget(List::new(items).block(list_block), inner[1]);
}

// ─── Key handling ─────────────────────────────────────────────────────────────
fn handle_key(app: &mut App, key: KeyCode, mods: KeyModifiers) -> bool {
    if app.mode == InputMode::Settings {
        return handle_settings_key(app, key, mods);
    }
    match &app.mode {
        InputMode::Normal | InputMode::Settings => {}
        InputMode::FuzzySearch => { handle_fuzzy_key(app, key, mods); return false; }
        InputMode::Rename(_)|InputMode::NewFile|InputMode::NewDir => { handle_input_key(app, key); return false; }
        InputMode::Confirm => {
            app.mode = InputMode::Normal;
            if matches!(key, KeyCode::Char('y')|KeyCode::Char('Y')) { app.delete_files(); }
            return false;
        }
        InputMode::Help => {
            app.mode = InputMode::Normal;
            return false;
        }
        InputMode::Progress => {
            // Esc cancels the running operation.
            if key == KeyCode::Esc {
                if let Some(flag) = &app.progress_cancel {
                    flag.store(true, Ordering::Relaxed);
                }
                app.msg("Cancelling…", false);
            }
            return false;
        }
        InputMode::RunArgs { .. } => {
            match key {
                KeyCode::Esc => {
                    app.mode = InputMode::Normal;
                    app.input_buf.clear();
                }
                KeyCode::Enter => {
                    let (base_args, cwd) = match mem::replace(&mut app.mode, InputMode::Normal) {
                        InputMode::RunArgs { args, cwd } => (args, cwd),
                        _ => unreachable!(),
                    };
                    let prefix = app.input_buf.trim().to_string();
                    app.input_buf.clear();
                    // Build final argv: prefix tokens (if any) then base args.
                    // e.g. prefix="mangohud" + base=["./game"] → ["mangohud","./game"]
                    let final_args: Vec<String> = if prefix.is_empty() {
                        base_args
                    } else {
                        let mut v: Vec<String> = prefix.split_whitespace().map(|s| s.to_string()).collect();
                        v.extend(base_args);
                        v
                    };
                    let term = app.cfg.opener_terminal.clone();
                    let arg_refs: Vec<&str> = final_args.iter().map(|s| s.as_str()).collect();
                    app.spawn_in_terminal(&term, &arg_refs, &cwd);
                }
                KeyCode::Backspace => { app.input_buf.pop(); }
                KeyCode::Char(c)   => app.input_buf.push(c),
                _ => {}
            }
            return false;
        }
    }
    // Clone the keys we need to avoid borrow issues
    let cfg = app.cfg.clone();
    match key {
        KeyCode::Up        => app.tab_mut().move_cursor(-1),
        KeyCode::Down      => app.tab_mut().move_cursor(1),
        KeyCode::Left  | KeyCode::Backspace => app.tab_mut().leave(),
        KeyCode::Right | KeyCode::Enter     => app.open_current(),
        KeyCode::PageUp    => app.tab_mut().move_cursor(-10),
        KeyCode::PageDown  => app.tab_mut().move_cursor(10),
        KeyCode::Home      => { app.tab_mut().state.select(Some(0)); }
        KeyCode::End       => { let n=app.tab().visible().len(); if n>0 { app.tab_mut().state.select(Some(n-1)); } }
        KeyCode::Esc       => return true,
        _ => {
            // ── Configurable keybinds ────────────────────────────────────────
            if key_matches(key, mods, &cfg.key_quit) {
                return true;
            } else if key_matches(key, mods, &cfg.key_switch_tab) {
                app.tab_idx = (app.tab_idx + 1) % app.tabs.len();
            } else if key_matches(key, mods, &cfg.key_select) {
                app.tab_mut().toggle_select();
            } else if key_matches(key, mods, &cfg.key_select_all) {
                app.tab_mut().select_all();
            } else if key == KeyCode::Char('r') && mods.contains(KeyModifiers::CONTROL) {
                app.tab_mut().deselect_all();
            } else if key_matches(key, mods, &cfg.key_copy) {
                if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
                else { app.yank_files(false); }
            } else if key_matches(key, mods, &cfg.key_cut) {
                if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
                else { app.yank_files(true); }
            } else if key_matches(key, mods, &cfg.key_paste) {
                app.paste_files();
            } else if key_matches(key, mods, &cfg.key_delete) {
                if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
                else { app.mode = InputMode::Confirm; }
            } else if key_matches(key, mods, &cfg.key_rename) {
                if let Some(p) = app.tab().current().cloned() {
                    let name = p.file_name().and_then(|n|n.to_str()).unwrap_or("").to_string();
                    app.input_buf = name.clone(); app.mode = InputMode::Rename(name);
                }
            } else if key_matches(key, mods, &cfg.key_new_file) {
                app.input_buf.clear(); app.mode = InputMode::NewFile;
            } else if key_matches(key, mods, &cfg.key_new_dir) {
                app.input_buf.clear(); app.mode = InputMode::NewDir;
            } else if key_matches(key, mods, &cfg.key_search) {
                app.open_fuzzy();
            } else if key_matches(key, mods, &cfg.key_toggle_hidden) {
                let h = !app.tab().show_hidden;
                app.tab_mut().show_hidden = h; app.tab_mut().refresh();
                app.msg(if h {"Hidden files shown"} else {"Hidden files hidden"}, false);
            } else if key_matches(key, mods, &cfg.key_new_tab) {
                app.new_tab();
            } else if key_matches(key, mods, &cfg.key_close_tab) {
                app.close_tab();
            } else if key == KeyCode::Char(':') {
                app.mode = InputMode::Settings;
            } else if key == KeyCode::Char('?') {
                app.mode = InputMode::Help;
            }
        }
    }
    false
}

/// Called whenever any setting is changed — applies everything that can take
/// effect immediately without a restart.
fn apply_settings_live(app: &mut App) {
    // Theme and icon set
    app.theme    = Theme::resolve(&app.cfg.theme, &app.user_themes);
    app.icon_set = ResolvedIconSet::resolve(&app.cfg.icon_set, &app.user_icons);

    // show_hidden — apply to every open tab
    let sh = app.cfg.show_hidden;
    for tab in &mut app.tabs {
        tab.show_hidden = sh;
        tab.refresh();
    }

    // col_parent / col_files — used directly from cfg at draw time, nothing to do.
    // date_format            — used directly from cfg at draw time, nothing to do.
    // opener_* / key_*       — used directly from cfg at call time, nothing to do.

    // If opener_terminal was cleared, re-detect.
    if app.cfg.opener_terminal.is_empty() {
        app.cfg.opener_terminal = App::detect_terminal();
    }
}

fn handle_settings_key(app: &mut App, key: KeyCode, _mods: KeyModifiers) -> bool {
    // ── Dropdown mode ──────────────────────────────────────────────────────────
    if app.settings.dropdown {
        let items  = SettingsState::section_items(&app.settings.section);
        let (k, _) = items[app.settings.cursor];
        let opts   = SettingsState::dropdown_options(k, &app.user_themes, &app.user_icons).unwrap_or_default();
        match key {
            KeyCode::Esc => { app.settings.dropdown = false; }
            KeyCode::Up  => { if app.settings.dd_cursor > 0 { app.settings.dd_cursor -= 1; } }
            KeyCode::Down => { if app.settings.dd_cursor + 1 < opts.len() { app.settings.dd_cursor += 1; } }
            KeyCode::Enter => {
                let chosen = opts[app.settings.dd_cursor].clone();
                SettingsState::set_value(k, &chosen, &mut app.cfg);
                app.settings.dropdown = false;
                app.settings.dirty    = true;
                apply_settings_live(app);
            }
            _ => {}
        }
        return false;
    }

    // ── Text edit mode ─────────────────────────────────────────────────────────
    if app.settings.editing {
        match key {
            KeyCode::Esc   => { app.settings.editing=false; app.settings.edit_buf.clear(); }
            KeyCode::Enter => {
                let items = SettingsState::section_items(&app.settings.section);
                let (k,_) = items[app.settings.cursor]; let v = app.settings.edit_buf.clone();
                SettingsState::set_value(k, &v, &mut app.cfg);
                app.settings.editing=false; app.settings.edit_buf=String::new(); app.settings.dirty=true;
                // Apply changes that take effect immediately without a restart.
                apply_settings_live(app);
            }
            KeyCode::Backspace => { app.settings.edit_buf.pop(); }
            KeyCode::Char(c)   => { app.settings.edit_buf.push(c); }
            _ => {}
        }
        return false;
    }

    // ── Normal navigation ──────────────────────────────────────────────────────
    match key {
        KeyCode::Esc  => { app.mode = InputMode::Normal; }
        KeyCode::Up   => { if app.settings.cursor > 0 { app.settings.cursor -= 1; } }
        KeyCode::Down => {
            let m = SettingsState::section_items(&app.settings.section).len().saturating_sub(1);
            if app.settings.cursor < m { app.settings.cursor += 1; }
        }
        KeyCode::Left => {
            app.settings.section = match app.settings.section {
                SettingsSection::Behaviour=>SettingsSection::Keybinds, SettingsSection::Appearance=>SettingsSection::Behaviour,
                SettingsSection::Openers=>SettingsSection::Appearance, SettingsSection::Keybinds=>SettingsSection::Openers,
            }; app.settings.cursor=0;
        }
        KeyCode::Right => {
            app.settings.section = match app.settings.section {
                SettingsSection::Behaviour=>SettingsSection::Appearance, SettingsSection::Appearance=>SettingsSection::Openers,
                SettingsSection::Openers=>SettingsSection::Keybinds, SettingsSection::Keybinds=>SettingsSection::Behaviour,
            }; app.settings.cursor=0;
        }
        KeyCode::Enter => {
            let items = SettingsState::section_items(&app.settings.section);
            let (k, _) = items[app.settings.cursor];
            if SettingsState::dropdown_options(k, &app.user_themes, &app.user_icons).is_some() {
                // Open dropdown — pre-select current value
                let cur_val = SettingsState::get_value(k, &app.cfg);
                let opts    = SettingsState::dropdown_options(k, &app.user_themes, &app.user_icons).unwrap();
                app.settings.dd_cursor = opts.iter().position(|o| o == &cur_val).unwrap_or(0);
                app.settings.dropdown  = true;
            } else {
                app.settings.edit_buf = SettingsState::get_value(k, &app.cfg);
                app.settings.editing  = true;
            }
        }
        KeyCode::Char('s')|KeyCode::Char('S') => {
            match app.cfg.save() {
                Ok(_)  => {
                    app.settings.dirty = false;
                    // Reload user themes and icon sets in case files were edited externally
                    app.user_themes = UserThemeEntry::load_all();
                    app.user_icons  = UserIconEntry::load_all();
                    apply_settings_live(app);
                    app.msg("Settings saved", false);
                }
                Err(e) => { app.msg(&format!("Save error: {}",e),true); }
            }
        }
        _ => {}
    }
    false
}

fn handle_fuzzy_key(app: &mut App, key: KeyCode, _mods: KeyModifiers) {
    match key {
        KeyCode::Esc   => { app.mode=InputMode::Normal; app.fuzzy_query.clear(); app.fuzzy_results.clear(); app.fuzzy_cursor=0; app.fuzzy_loading=false; app.fuzzy_rx=None; }
        KeyCode::Enter => app.fuzzy_accept(),
        KeyCode::Up    => { if app.fuzzy_cursor>0{app.fuzzy_cursor-=1;} }
        KeyCode::Down  => { if app.fuzzy_cursor+1<app.fuzzy_results.len(){app.fuzzy_cursor+=1;} }
        KeyCode::Backspace => { app.fuzzy_query.pop(); app.fuzzy_update_results(); }
        KeyCode::Char(c)   => { app.fuzzy_query.push(c); app.fuzzy_update_results(); }
        _ => {}
    }
}

fn handle_input_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc   => { app.mode=InputMode::Normal; app.input_buf.clear(); }
        KeyCode::Enter => {
            let val  = app.input_buf.clone();
            let mode = std::mem::replace(&mut app.mode, InputMode::Normal);
            app.input_buf.clear();
            if val.is_empty() { return; }
            match mode {
                InputMode::Rename(orig) if val != orig => {
                    let src=app.tab().cwd.join(&orig); let dst=app.tab().cwd.join(&val);
                    match fs::rename(&src,&dst) { Ok(_)=>{app.tab_mut().refresh();app.msg(&format!("Renamed \u{2192} {}",val),false);} Err(e)=>app.msg(&e.to_string(),true), }
                }
                InputMode::NewFile => {
                    let t=app.tab().cwd.join(&val);
                    match fs::File::create(&t) { Ok(_)=>{app.tab_mut().refresh();app.msg(&format!("Created {}",val),false);} Err(e)=>app.msg(&e.to_string(),true), }
                }
                InputMode::NewDir => {
                    let t=app.tab().cwd.join(&val);
                    match fs::create_dir_all(&t) { Ok(_)=>{app.tab_mut().refresh();app.msg(&format!("Created dir {}",val),false);} Err(e)=>app.msg(&e.to_string(),true), }
                }
                _ => {}
            }
        }
        KeyCode::Backspace => { app.input_buf.pop(); }
        KeyCode::Char(c)   => app.input_buf.push(c),
        _ => {}
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────────
fn main() -> Result<()> {
    let cfg   = Config::load();
    let start = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(dirs_home);
    let start = if start.is_dir() { start } else { start.parent().map(|p| p.to_path_buf()).unwrap_or_else(dirs_home) };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend  = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    let mut app  = App::new(start, cfg);

    loop {
        term.draw(|f| ui(f, &mut app))?;
        app.tick();

        if let Some(path) = app.nvim_path.take() {
            disable_raw_mode()?;
            execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
            term.show_cursor()?;
            let _ = Command::new(&app.cfg.opener_editor).arg(&path).status();
            enable_raw_mode()?;
            execute!(term.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
            term.hide_cursor()?;
            term.clear()?;
            continue;
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(k) = event::read()? {
                if handle_key(&mut app, k.code, k.modifiers) { break; }
            }
        }
    }

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    // Clean up any video thumbnail temp file
    if let Some(tmp) = app.thumb_tmp.take() { let _ = fs::remove_file(tmp); }
    Ok(())
}
