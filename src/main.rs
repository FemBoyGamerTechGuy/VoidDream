use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    prelude::StatefulWidget,
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
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
    env,
};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// THEME SYSTEM
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// Theme files live in:
//   /usr/share/VoidDream/themes/<name>.json  — system
//   ~/.local/share/VoidDream/themes/<name>.json  — user (overrides system)
//
// A theme file has two sections:
//
//   "palette": { "base": "#1e1e2e", "mantle": "#181825", ... }   — named colors
//   "roles":   { "bg_primary": "base", "cursor_bg": "mauve", ... } — semantic roles
//                                                                     values are either a palette key
//                                                                     or a raw "#RRGGBB" hex string
//
// Every role the app uses is listed below in ThemeRoles.  Missing roles fall
// back to a built-in neutral gray so the app always renders correctly even with
// an incomplete theme file.
//
// PALETTE section is optional — if you omit it you can put raw hex directly in
// every role value.  Palette is just a named-color shorthand.
//
// Example minimal theme:
// {
//   "palette": { "bg": "#1e1e2e", "fg": "#cdd6f4", "accent": "#cba6f7" },
//   "roles": {
//     "bg_primary":   "bg",     "bg_panel":    "bg",
//     "bg_popup":     "bg",     "bg_statusbar":"bg",
//     "bg_tabbar":    "bg",     "bg_selected": "accent",
//     "fg_primary":   "fg",     "fg_dim":      "fg",
//     "fg_muted":     "fg",     "fg_cursor":   "bg",
//     "accent":       "accent", "accent2":     "accent",
//     "border":       "accent", "border_dim":  "fg",
//     "warn":         "#f38ba8","ok":          "#a6e3a1",
//     "kind_dir":     "accent", "kind_image":  "#f5c2e7",
//     "kind_video":   "accent", "kind_audio":  "accent",
//     "kind_archive": "#f38ba8","kind_jar":    "#94e2d5",
//     "kind_doc":     "#f9e2af","kind_code":   "#a6e3a1",
//     "kind_exec":    "#94e2d5","kind_symlink":"#94e2d5",
//     "kind_other":   "fg"
//   }
// }
//

/// Raw JSON shape of a theme file.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct ThemeFile {
    #[serde(default)]
    palette: std::collections::HashMap<String, String>,
    #[serde(default)]
    roles:   std::collections::HashMap<String, String>,
}

impl ThemeFile {
    /// Resolve a role value: if it's a palette key, return the hex; else treat as raw hex.
    fn resolve_color(&self, key: &str) -> Option<Color> {
        let raw = self.roles.get(key)?;
        let hex = self.palette.get(raw.as_str()).unwrap_or(raw);
        Some(parse_hex(hex))
    }
    fn rc(&self, key: &str, fallback: Color) -> Color {
        self.resolve_color(key).unwrap_or(fallback)
    }
}

/// Returns the XDG data home dir: $XDG_DATA_HOME if set, else ~/.local/share
fn xdg_data_home() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() { return PathBuf::from(xdg); }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".local").join("share")
}

fn parse_hex(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) { return Color::Rgb(r, g, b); }
    }
    Color::Reset
}

/// Runtime theme — every semantic color role the UI uses.
/// All fields are resolved Color values; no strings at runtime.
#[derive(Clone)]
pub struct Theme {
    // Backgrounds
    pub bg_primary:   Color,  // main pane background
    pub bg_panel:     Color,  // secondary / side panels
    pub bg_popup:     Color,  // overlays, dialogs, help
    pub bg_statusbar: Color,  // bottom status bar
    pub bg_tabbar:    Color,  // top tab bar
    pub bg_selected:  Color,  // highlighted / active tab background
    pub bg_cursor:    Color,  // file list cursor background
    pub bg_sel_entry: Color,  // multi-selected entry background

    // Foregrounds
    pub fg_primary:   Color,  // main text
    pub fg_dim:       Color,  // secondary text / labels
    pub fg_muted:     Color,  // timestamps, sizes, faint info
    pub fg_cursor:    Color,  // text on cursor row
    pub fg_active_tab:Color,  // active tab label text

    // Accents / semantic
    pub accent:       Color,  // primary accent (cursor fg, active tab, highlights)
    pub accent2:      Color,  // secondary accent (selection markers, yank count)
    pub border:       Color,  // active / focused border
    pub border_dim:   Color,  // inactive border
    pub warn:         Color,  // errors, deletes
    pub ok:           Color,  // success, paste confirmation

    // File-kind colors
    pub kind_dir:     Color,
    pub kind_image:   Color,
    pub kind_video:   Color,
    pub kind_audio:   Color,
    pub kind_archive: Color,
    pub kind_jar:     Color,
    pub kind_doc:     Color,
    pub kind_code:    Color,
    pub kind_exec:    Color,
    pub kind_symlink: Color,
    pub kind_other:   Color,
}

impl Theme {
    /// Neutral fallback — pure gray scale, works on any terminal.
    /// Hardcoded fallback used when no theme JSON is found.
    fn neutral() -> Self {
        // catppuccin-macchiato palette
        let base     = Color::Rgb(36,  39,  58);   // #24273a
        let surface0 = Color::Rgb(54,  58,  79);   // #363a4f
        let surface1 = Color::Rgb(73,  77, 100);   // #494d64
        let overlay0 = Color::Rgb(110, 115, 141);  // #6e738d
        let text     = Color::Rgb(202, 211, 245);  // #cad3f5
        let subtext  = Color::Rgb(165, 173, 203);  // #a5adcb
        let mauve    = Color::Rgb(198, 160, 246);  // #c6a0f6
        let blue     = Color::Rgb(138, 173, 244);  // #8aadf4
        let teal     = Color::Rgb(139, 213, 202);  // #8bd5ca
        let sapphire = Color::Rgb(125, 196, 228);  // #7dc4e4
        let green    = Color::Rgb(166, 218, 149);  // #a6da95
        let red      = Color::Rgb(237, 135, 150);  // #ed8796
        let yellow   = Color::Rgb(238, 212, 159);  // #eed49f
        let pink     = Color::Rgb(245, 189, 230);  // #f5bde6
        Self {
            bg_primary:   base,     bg_panel:      surface0, bg_popup:     surface1,
            bg_statusbar: Color::Rgb(30, 32, 48),  // #1e2030 mantle
            bg_tabbar:    Color::Rgb(24, 25, 38),  // #181926 crust
            bg_selected:  mauve,    bg_cursor:     mauve,    bg_sel_entry: surface0,
            fg_primary:   text,     fg_dim:        subtext,  fg_muted:     overlay0,
            fg_cursor:    base,     fg_active_tab: base,
            accent:       mauve,    accent2:       yellow,
            border:       blue,     border_dim:    surface1,
            warn:         red,      ok:            green,
            kind_dir:     blue,     kind_image:    pink,
            kind_video:   mauve,    kind_audio:    mauve,
            kind_archive: red,      kind_jar:      teal,
            kind_doc:     yellow,   kind_code:     green,
            kind_exec:    teal,     kind_symlink:  sapphire,
            kind_other:   text,
        }
    }

    /// Build a Theme from a parsed ThemeFile, falling back to neutral for missing roles.
    fn from_file(tf: &ThemeFile) -> Self {
        let n = Self::neutral();
        Self {
            bg_primary:    tf.rc("bg_primary",    n.bg_primary),
            bg_panel:      tf.rc("bg_panel",       n.bg_panel),
            bg_popup:      tf.rc("bg_popup",       n.bg_popup),
            bg_statusbar:  tf.rc("bg_statusbar",   n.bg_statusbar),
            bg_tabbar:     tf.rc("bg_tabbar",      n.bg_tabbar),
            bg_selected:   tf.rc("bg_selected",    n.bg_selected),
            bg_cursor:     tf.rc("bg_cursor",      n.bg_cursor),
            bg_sel_entry:  tf.rc("bg_sel_entry",   n.bg_sel_entry),
            fg_primary:    tf.rc("fg_primary",     n.fg_primary),
            fg_dim:        tf.rc("fg_dim",         n.fg_dim),
            fg_muted:      tf.rc("fg_muted",       n.fg_muted),
            fg_cursor:     tf.rc("fg_cursor",      n.fg_cursor),
            fg_active_tab: tf.rc("fg_active_tab",  n.fg_active_tab),
            accent:        tf.rc("accent",         n.accent),
            accent2:       tf.rc("accent2",        n.accent2),
            border:        tf.rc("border",         n.border),
            border_dim:    tf.rc("border_dim",     n.border_dim),
            warn:          tf.rc("warn",           n.warn),
            ok:            tf.rc("ok",             n.ok),
            kind_dir:      tf.rc("kind_dir",       n.kind_dir),
            kind_image:    tf.rc("kind_image",     n.kind_image),
            kind_video:    tf.rc("kind_video",     n.kind_video),
            kind_audio:    tf.rc("kind_audio",     n.kind_audio),
            kind_archive:  tf.rc("kind_archive",   n.kind_archive),
            kind_jar:      tf.rc("kind_jar",       n.kind_jar),
            kind_doc:      tf.rc("kind_doc",       n.kind_doc),
            kind_code:     tf.rc("kind_code",      n.kind_code),
            kind_exec:     tf.rc("kind_exec",      n.kind_exec),
            kind_symlink:  tf.rc("kind_symlink",   n.kind_symlink),
            kind_other:    tf.rc("kind_other",     n.kind_other),
        }
    }

    /// Scan theme dirs and return all available theme names.
    pub fn theme_dirs() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/usr/share/VoidDream/themes"),
            xdg_data_home().join("VoidDream").join("themes"),
        ]
    }

    pub fn installed_names() -> Vec<String> {
        let mut seen = std::collections::HashMap::<String,()>::new();
        for dir in Self::theme_dirs() {
            if !dir.exists() { continue; }
            if let Ok(rd) = fs::read_dir(&dir) {
                let mut names: Vec<String> = rd
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
                    .filter_map(|e| e.path().file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
                    .collect();
                names.sort();
                for n in names { seen.insert(n, ()); }
            }
        }
        let mut v: Vec<String> = seen.into_keys().collect();
        v.sort();
        v
    }

    /// Load a theme by name, scanning system then user dirs (user wins).
    pub fn load(name: &str) -> Self {
        let filename = format!("{}.json", name);
        let mut result: Option<ThemeFile> = None;
        for dir in Self::theme_dirs() {
            let path = dir.join(&filename);
            if let Ok(text) = fs::read_to_string(&path) {
                let text = text.trim_start_matches('\u{feff}');
                if let Ok(tf) = serde_json::from_str::<ThemeFile>(text) {
                    result = Some(tf);
                }
            }
        }
        match result {
            Some(tf) => Self::from_file(&tf),
            None     => Self::neutral(),
        }
    }
}

fn kind_color(k: &FileKind, t: &Theme) -> Color {
    match k {
        FileKind::Dir     => t.kind_dir,
        FileKind::Image   => t.kind_image,
        FileKind::Video   => t.kind_video,
        FileKind::Audio   => t.kind_audio,
        FileKind::Archive => t.kind_archive,
        FileKind::Jar     => t.kind_jar,
        FileKind::Doc     => t.kind_doc,
        FileKind::Code    => t.kind_code,
        FileKind::Exec    => t.kind_exec,
        FileKind::Symlink => t.kind_symlink,
        FileKind::Other   => t.kind_other,
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ICON SYSTEM
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// Icon files live in:
//   /usr/share/VoidDream/icons/<name>.json
//   ~/.local/share/VoidDream/icons/<name>.json   (overrides system)
//
// Full JSON schema:
// {
//   "dir":     "",  "symlink": "󰌷",  "image": "",  "video": "",
//   "audio":   "",  "archive": "",  "jar":   "",  "doc":   "",
//   "code":    "",  "exec":    "",  "other": "",
//
//   "by_name": { "dockerfile": "", ".gitignore": "", ... },
//   "by_ext":  { "rs": "", "py": "", "js": "", ... },
//   "named_dirs": { ".config": "", "downloads": "", ... },
//
//   "chrome": {
//     "tab_sep":        "",   -- separator between tabs
//     "clock":          "",   -- clock icon
//     "calendar":       "",   -- calendar/date icon
//     "cursor_arrow":   "",   -- selection cursor arrow
//     "yank_icon":      "",   -- yank count indicator
//     "sel_icon":       "",   -- selection count indicator
//     "dir_icon":       "",   -- files-pane border title folder icon
//     "no_image":       "",   -- shown when terminal can't display images
//     "progress_fill":  "█",  -- progress bar fill char
//     "done_icon":      "",   -- extraction done icon
//     "eta_icon":       "",   -- ETA icon in progress
//     "arrow_icon":     "",   -- forward arrow in progress
//     "help_icon":      "?",  -- help overlay title icon
//     "nav_icon":       "",   -- help nav section icon
//     "ops_icon":       "",   -- help file ops section icon
//     "tab_icon":       "",   -- help tabs section icon
//     "rename_icon":    "",   -- rename mode pill icon
//     "newfile_icon":   "",   -- new file mode pill icon
//     "newdir_icon":    "",   -- new directory mode pill icon
//     "search_icon":    "",   -- fuzzy search title icon
//     "terminal_icon":  "",   -- terminal/shell indicator
//     "settings_icon":  "",   -- settings section icon
//     "search_sec_icon":"",   -- settings search section icon
//     "cursor_block":   "█"   -- text cursor block character
//   }
// }
//

/// The Chrome struct holds every UI glyph the TUI uses outside of file icons.
/// This lets icon sets swap between NerdFont glyphs, emoji, ASCII, or anything.
#[derive(Clone, Debug)]
pub struct Chrome {
    pub tab_sep:        String,  // separator between tabs (powerline arrow etc)
    pub clock:          String,  // clock icon
    pub calendar:       String,  // date/calendar icon
    pub cursor_arrow:   String,  // list cursor arrow
    pub yank_icon:      String,  // yank buffer count icon
    pub sel_icon:       String,  // multi-selection count icon
    pub dir_icon:       String,  // folder icon in pane title
    pub no_image:       String,  // "can't show image" placeholder
    pub progress_fill:  String,  // progress bar fill character
    pub done_icon:      String,  // extraction done
    pub eta_icon:       String,  // clock in ETA display
    pub arrow_icon:     String,  // forward arrow in ETA display
    pub help_icon:      String,  // help overlay title
    pub nav_icon:       String,  // help nav section header
    pub ops_icon:       String,  // help file ops section header
    pub tab_sec_icon:   String,  // help tabs section header
    pub rename_icon:    String,  // rename mode pill
    pub newfile_icon:   String,  // new file mode pill
    pub newdir_icon:    String,  // new directory mode pill
    pub search_icon:    String,  // fuzzy search overlay title
    pub terminal_icon:  String,  // terminal/exec indicator in run-args
    pub settings_icon:  String,  // settings section decoration
    pub search_sec_icon:String,  // settings fuzzy section icon
    pub cursor_block:   String,  // text cursor character
}

impl Default for Chrome {
    /// ASCII-safe fallback — works on any terminal without special fonts.
    fn default() -> Self {
        Self {
            tab_sep:         "|".into(),
            clock:           "[T]".into(),
            calendar:        "[D]".into(),
            cursor_arrow:    ">".into(),
            yank_icon:       "[Y]".into(),
            sel_icon:        "[S]".into(),
            dir_icon:        "/".into(),
            no_image:        "(no image)".into(),
            progress_fill:   "#".into(),
            done_icon:       "[OK]".into(),
            eta_icon:        "[T]".into(),
            arrow_icon:      "->".into(),
            help_icon:       "?".into(),
            nav_icon:        "[nav]".into(),
            ops_icon:        "[ops]".into(),
            tab_sec_icon:    "[tab]".into(),
            rename_icon:     "[R]".into(),
            newfile_icon:    "[F]".into(),
            newdir_icon:     "[D]".into(),
            search_icon:     "[/]".into(),
            terminal_icon:   ">_".into(),
            settings_icon:   "[*]".into(),
            search_sec_icon: "[/]".into(),
            cursor_block:    "\u{2588}".into(),
        }
    }
}

/// Runtime icon set — all data driven from a JSON file.
#[derive(Clone, Debug, Default)]
pub struct IconData {
    // kind fallbacks
    pub dir:     Option<String>,
    pub symlink: Option<String>,
    pub image:   Option<String>,
    pub video:   Option<String>,
    pub audio:   Option<String>,
    pub archive: Option<String>,
    pub jar:     Option<String>,
    pub doc:     Option<String>,
    pub code:    Option<String>,
    pub exec:    Option<String>,
    pub other:   Option<String>,
    // lookup tables
    pub by_name:    std::collections::HashMap<String, String>,
    pub by_ext:     std::collections::HashMap<String, String>,
    pub named_dirs: std::collections::HashMap<String, String>,
    // UI chrome
    pub chrome: Chrome,
}

/// Raw JSON shape — all fields optional so partial files are fine.
#[derive(Deserialize, Default)]
struct IconJson {
    dir:     Option<String>,
    symlink: Option<String>,
    image:   Option<String>,
    video:   Option<String>,
    audio:   Option<String>,
    archive: Option<String>,
    jar:     Option<String>,
    doc:     Option<String>,
    code:    Option<String>,
    exec:    Option<String>,
    other:   Option<String>,
    #[serde(default)] by_name:    std::collections::HashMap<String, String>,
    #[serde(default)] by_ext:     std::collections::HashMap<String, String>,
    #[serde(default)] named_dirs: std::collections::HashMap<String, String>,
    #[serde(default)] chrome:     ChromeJson,
}

#[derive(Deserialize, Default)]
struct ChromeJson {
    tab_sep:         Option<String>,
    clock:           Option<String>,
    calendar:        Option<String>,
    cursor_arrow:    Option<String>,
    yank_icon:       Option<String>,
    sel_icon:        Option<String>,
    dir_icon:        Option<String>,
    no_image:        Option<String>,
    progress_fill:   Option<String>,
    done_icon:       Option<String>,
    eta_icon:        Option<String>,
    arrow_icon:      Option<String>,
    help_icon:       Option<String>,
    nav_icon:        Option<String>,
    ops_icon:        Option<String>,
    tab_sec_icon:    Option<String>,
    rename_icon:     Option<String>,
    newfile_icon:    Option<String>,
    newdir_icon:     Option<String>,
    search_icon:     Option<String>,
    terminal_icon:   Option<String>,
    settings_icon:   Option<String>,
    search_sec_icon: Option<String>,
    cursor_block:    Option<String>,
}

impl ChromeJson {
    fn into_chrome(self) -> Chrome {
        let d = Chrome::default();
        Chrome {
            tab_sep:         self.tab_sep.unwrap_or(d.tab_sep),
            clock:           self.clock.unwrap_or(d.clock),
            calendar:        self.calendar.unwrap_or(d.calendar),
            cursor_arrow:    self.cursor_arrow.unwrap_or(d.cursor_arrow),
            yank_icon:       self.yank_icon.unwrap_or(d.yank_icon),
            sel_icon:        self.sel_icon.unwrap_or(d.sel_icon),
            dir_icon:        self.dir_icon.unwrap_or(d.dir_icon),
            no_image:        self.no_image.unwrap_or(d.no_image),
            progress_fill:   self.progress_fill.unwrap_or(d.progress_fill),
            done_icon:       self.done_icon.unwrap_or(d.done_icon),
            eta_icon:        self.eta_icon.unwrap_or(d.eta_icon),
            arrow_icon:      self.arrow_icon.unwrap_or(d.arrow_icon),
            help_icon:       self.help_icon.unwrap_or(d.help_icon),
            nav_icon:        self.nav_icon.unwrap_or(d.nav_icon),
            ops_icon:        self.ops_icon.unwrap_or(d.ops_icon),
            tab_sec_icon:    self.tab_sec_icon.unwrap_or(d.tab_sec_icon),
            rename_icon:     self.rename_icon.unwrap_or(d.rename_icon),
            newfile_icon:    self.newfile_icon.unwrap_or(d.newfile_icon),
            newdir_icon:     self.newdir_icon.unwrap_or(d.newdir_icon),
            search_icon:     self.search_icon.unwrap_or(d.search_icon),
            terminal_icon:   self.terminal_icon.unwrap_or(d.terminal_icon),
            settings_icon:   self.settings_icon.unwrap_or(d.settings_icon),
            search_sec_icon: self.search_sec_icon.unwrap_or(d.search_sec_icon),
            cursor_block:    self.cursor_block.unwrap_or(d.cursor_block),
        }
    }
}

impl IconData {
    fn icon_dirs() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/usr/share/VoidDream/icons"),
            xdg_data_home().join("VoidDream").join("icons"),
        ]
    }

    pub fn installed_names() -> Vec<String> {
        let mut seen = std::collections::HashMap::<String,()>::new();
        for dir in Self::icon_dirs() {
            if !dir.exists() { continue; }
            if let Ok(rd) = fs::read_dir(&dir) {
                let mut names: Vec<String> = rd
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
                    .filter_map(|e| e.path().file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
                    .collect();
                names.sort();
                for n in names { seen.insert(n, ()); }
            }
        }
        let mut v: Vec<String> = seen.into_keys().collect();
        v.sort();
        v
    }

    /// Load an icon set by name.  Later dirs (user) override earlier (system).
    pub fn load(name: &str) -> Self {
        let filename = format!("{}.json", name);
        let mut result: Option<IconJson> = None;
        for dir in Self::icon_dirs() {
            let path = dir.join(&filename);
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(ij) = serde_json::from_str::<IconJson>(&text) {
                    result = Some(ij);
                }
            }
        }
        match result {
            None => IconData::default(),
            Some(ij) => IconData {
                dir:     ij.dir,
                symlink: ij.symlink,
                image:   ij.image,
                video:   ij.video,
                audio:   ij.audio,
                archive: ij.archive,
                jar:     ij.jar,
                doc:     ij.doc,
                code:    ij.code,
                exec:    ij.exec,
                other:   ij.other,
                by_name:    ij.by_name,
                by_ext:     ij.by_ext,
                named_dirs: ij.named_dirs,
                chrome:     ij.chrome.into_chrome(),
            },
        }
    }

    /// Resolve icon for a given file path + kind.
    /// Priority: by_name > named_dirs (for dirs) > by_ext > kind fallback
    pub(crate) fn file_icon<'a>(&'a self, path: &Path, kind: &FileKind) -> &'a str {
        let name = path.file_name().and_then(|n| n.to_str())
            .map(|s| s.to_lowercase()).unwrap_or_default();
        let ext  = path.extension().and_then(|e| e.to_str())
            .map(|s| s.to_lowercase()).unwrap_or_default();

        // 1. Exact filename match
        if let Some(ic) = self.by_name.get(name.as_str()) { return ic.as_str(); }

        // 2. Directory → named_dirs lookup
        if *kind == FileKind::Dir {
            if let Some(ic) = self.named_dirs.get(name.as_str()) { return ic.as_str(); }
            return self.dir.as_deref().unwrap_or("");
        }

        // 3. Symlink
        if *kind == FileKind::Symlink {
            return self.symlink.as_deref().unwrap_or("");
        }

        // 4. Extension match
        if !ext.is_empty() {
            if let Some(ic) = self.by_ext.get(ext.as_str()) { return ic.as_str(); }
        }

        // 5. Kind fallback
        match kind {
            FileKind::Image   => self.image.as_deref().unwrap_or(""),
            FileKind::Video   => self.video.as_deref().unwrap_or(""),
            FileKind::Audio   => self.audio.as_deref().unwrap_or(""),
            FileKind::Archive => self.archive.as_deref().unwrap_or(""),
            FileKind::Jar     => self.jar.as_deref().unwrap_or(""),
            FileKind::Doc     => self.doc.as_deref().unwrap_or(""),
            FileKind::Code    => self.code.as_deref().unwrap_or(""),
            FileKind::Exec    => self.exec.as_deref().unwrap_or(""),
            FileKind::Other   => self.other.as_deref().unwrap_or(""),
            _                 => "",
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// STYLE HELPERS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
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
    pub opener_jar:        String,
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
    pub key_cycle_tab:     String,
    pub key_select:        String,
    pub key_select_all:    String,
    pub show_clock:        bool,   // live HH:MM:SS clock in tab bar
    pub show_file_mtime:   bool,   // date/time column in file panes
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_hidden: true, date_format: "%d/%m/%Y %H:%M".into(),
            col_parent: 20, col_files: 37,
            theme: "catppuccin-macchiato".into(), icon_set: "nerdfont".into(),
            opener_image: "mirage".into(), opener_video: "mpv".into(),
            opener_audio: "mpv".into(), opener_doc: "libreoffice".into(),
            opener_editor: "nvim".into(),
            opener_jar: "java -jar".into(),
            opener_terminal: "kitty".into(),
            key_copy: "c".into(), key_cut: "u".into(), key_paste: "p".into(),
            key_delete: "d".into(), key_rename: "r".into(),
            key_new_file: "f".into(), key_new_dir: "m".into(),
            key_search: "/".into(), key_toggle_hidden: ".".into(),
            key_quit: "q".into(), key_new_tab: "t".into(),
            key_close_tab: "x".into(), key_cycle_tab: "Tab".into(),
            key_select: "Space".into(), key_select_all: "Ctrl+a".into(),
            show_clock: true,
            show_file_mtime: true,
        }
    }
}

impl Config {
    fn config_path() -> PathBuf {
        let cfg_home = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() { PathBuf::from(xdg) }
            else { PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into())).join(".config") }
        } else {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into())).join(".config")
        };
        cfg_home.join("VoidDream").join("config.json")
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

// ─── File types ───────────────────────────────────────────────────────────────
const IMAGE_EXT:   &[&str] = &["png","jpg","jpeg","gif","bmp","webp","svg","ico","tiff","avif"];
const VIDEO_EXT:   &[&str] = &["mp4","mkv","avi","mov","webm","flv","wmv","m4v","mpg","mpeg"];
const AUDIO_EXT:   &[&str] = &["mp3","flac","ogg","wav","aac","m4a","opus","wma"];
const ARCHIVE_EXT: &[&str] = &["zip","tar","gz","bz2","xz","7z","rar","zst","tgz","tbz2"];
const JAR_EXT:     &[&str] = &["jar","war","ear"];
const DOC_EXT:     &[&str] = &["pdf","doc","docx","odt","xls","xlsx","ods","ppt","pptx","odp"];
const CODE_EXT:    &[&str] = &[
    "py","js","ts","rs","go","c","cpp","h","java","rb","php",
    "sh","bash","zsh","fish","lua","vim","toml","yaml","yml",
    "json","xml","html","css","scss","md","rst","txt","conf",
    "ini","cfg","env","lock",
];

#[derive(Clone, PartialEq)]
pub(crate) enum FileKind { Dir, Image, Video, Audio, Archive, Jar, Doc, Code, Exec, Symlink, Other }

fn file_kind(path: &Path) -> FileKind {
    if path.is_symlink() { return FileKind::Symlink; }
    if path.is_dir()     { return FileKind::Dir; }
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
    let ext = ext.as_str();
    if IMAGE_EXT.contains(&ext)   { return FileKind::Image; }
    if VIDEO_EXT.contains(&ext)   { return FileKind::Video; }
    if AUDIO_EXT.contains(&ext)   { return FileKind::Audio; }
    if ARCHIVE_EXT.contains(&ext) { return FileKind::Archive; }
    if JAR_EXT.contains(&ext)     { return FileKind::Jar; }
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
/// Returns (time_str, date_str) split from the mtime, e.g. ("20:54", "07/03/2026")
fn format_mtime_split(path: &Path) -> (String, String) {
    use std::time::SystemTime;
    if let Ok(meta) = path.metadata() {
        if let Ok(modified) = meta.modified() {
            let secs = modified.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
            let (y, mo, d, h, mi) = secs_to_datetime(secs);
            let time = format!("{:02}:{:02}", h, mi);
            let date = format!("{:02}/{:02}/{:04}", d, mo, y);
            return (time, date);
        }
    }
    ("?".into(), "?".into())
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

fn current_time_str() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (_, _, _, h, mi) = secs_to_datetime(secs);
    let sc = secs % 60;
    format!("{:02}:{:02}:{:02}", h, mi, sc)
}

fn current_date_str() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, mo, d, _, _) = secs_to_datetime(secs);
    format!("{:02}/{:02}/{:04}", d, mo, y)
}

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

fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() { copy_dir(&entry.path(), &dst.join(entry.file_name()))?; }
        else { fs::copy(entry.path(), dst.join(entry.file_name()))?; }
    }
    Ok(())
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
                ("show_clock","Show clock in tab bar"),
                ("show_file_mtime","Show file date/time in file list"),
            ],
            SettingsSection::Appearance => vec![
                ("col_parent","Parent pane width (%)"), ("col_files","Files pane width (%)"),
                ("theme","Theme"), ("icon_set","Icon theme"),
            ],
            SettingsSection::Openers    => vec![
                ("opener_image","Image"), ("opener_video","Video"), ("opener_audio","Audio"),
                ("opener_doc","Documents"), ("opener_editor","Editor"),
                ("opener_jar","Java (.jar)"), ("opener_terminal","Terminal"),
                // Archive extraction is built-in per format
                ("fixed_arc_rar",  "Archive .rar"),
                ("fixed_arc_zip",  "Archive .zip"),
                ("fixed_arc_tgz",  "Archive .tar.gz / .tgz"),
                ("fixed_arc_tbz2", "Archive .tar.bz2 / .tbz2"),
                ("fixed_arc_txz",  "Archive .tar.xz"),
                ("fixed_arc_tzst", "Archive .tar.zst"),
                ("fixed_arc_tar",  "Archive .tar"),
                ("fixed_arc_gz",   "Archive .gz"),
                ("fixed_arc_bz2",  "Archive .bz2"),
                ("fixed_arc_xz",   "Archive .xz"),
                ("fixed_arc_zst",  "Archive .zst"),
                ("fixed_arc_7z",   "Archive .7z"),
            ],
            SettingsSection::Keybinds   => vec![
                // Configurable
                ("key_select",       "Select / deselect"),
                ("key_select_all",   "Select all"),
                ("key_copy",         "Copy"),
                ("key_cut",          "Cut"),
                ("key_paste",        "Paste"),
                ("key_delete",       "Delete"),
                ("key_rename",       "Rename"),
                ("key_new_file",     "New file"),
                ("key_new_dir",      "New directory"),
                ("key_search",       "Fuzzy search"),
                ("key_toggle_hidden","Toggle hidden files"),
                ("key_new_tab",      "New tab"),
                ("key_close_tab",    "Close tab"),
                ("key_cycle_tab",    "Cycle tabs"),
                ("key_quit",         "Quit"),
                // Fixed (not configurable)
                ("fixed_nav",        "Navigate"),
                ("fixed_open",       "Open / enter dir"),
                ("fixed_up",         "Go up"),
                ("fixed_pgupdown",   "Jump 10 entries"),
                ("fixed_homeend",    "First / last entry"),
                ("fixed_deselect",   "Deselect all"),
                ("fixed_sel_all2",   "Select all (alt)"),
                ("fixed_settings",   "Open settings"),
                ("fixed_help",       "Show help"),
                ("fixed_quit2",      "Quit (alt)"),
            ],
        }
    }
    fn dropdown_options(key: &str) -> Option<Vec<String>> {
        match key {
            "theme"       => Some(Theme::installed_names()),
            "icon_set"    => Some(vec!["nerdfont","emoji","minimal","none"].iter().map(|s| s.to_string()).collect()),
            "show_hidden"      => Some(vec!["true".to_string(), "false".to_string()]),
            "show_clock"       => Some(vec!["true".to_string(), "false".to_string()]),
            "show_file_mtime"  => Some(vec!["true".to_string(), "false".to_string()]),
            _ => None,
        }
    }
    fn get_value(key: &str, cfg: &Config) -> String {
        match key {
            "show_hidden"       => cfg.show_hidden.to_string(),
            "show_clock"        => cfg.show_clock.to_string(),
            "show_file_mtime"   => cfg.show_file_mtime.to_string(),
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
            "opener_jar"        => cfg.opener_jar.clone(),
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
            "key_cycle_tab"     => cfg.key_cycle_tab.clone(),
            "key_select"        => cfg.key_select.clone(),
            "key_select_all"    => cfg.key_select_all.clone(),
            "fixed_arc_rar"   => "unrar x -o+  (built-in)".into(),
            "fixed_arc_zip"   => "unzip -o  (built-in)".into(),
            "fixed_arc_tgz"   => "tar -xzf  (built-in)".into(),
            "fixed_arc_tbz2"  => "tar -xjf  (built-in)".into(),
            "fixed_arc_txz"   => "tar -xJf  (built-in)".into(),
            "fixed_arc_tzst"  => "tar --zstd -xf  (built-in)".into(),
            "fixed_arc_tar"   => "tar -xf  (built-in)".into(),
            "fixed_arc_gz"    => "gunzip -kf  (built-in)".into(),
            "fixed_arc_bz2"   => "bunzip2 -kf  (built-in)".into(),
            "fixed_arc_xz"    => "xz -dkf  (built-in)".into(),
            "fixed_arc_zst"   => "zstd -dkf  (built-in)".into(),
            "fixed_arc_7z"    => "7z x -y  (built-in)".into(),
            "fixed_nav"        => "\u{2191} / \u{2193}  (fixed)".into(),
            "fixed_open"       => "\u{2192} / Enter  (fixed)".into(),
            "fixed_up"         => "\u{2190} / Backspace  (fixed)".into(),
            "fixed_pgupdown"   => "Page Up / Page Down  (fixed)".into(),
            "fixed_homeend"    => "Home / End  (fixed)".into(),
            "fixed_deselect"   => "Ctrl+r  (fixed)".into(),
            "fixed_sel_all2"   => "A  (fixed)".into(),
            "fixed_settings"   => ":  (fixed)".into(),
            "fixed_help"       => "?  (fixed)".into(),
            "fixed_quit2"      => "Esc  (fixed)".into(),
            _ => String::new(),
        }
    }
    fn set_value(key: &str, val: &str, cfg: &mut Config) {
        match key {
            "show_hidden"       => cfg.show_hidden = val == "true",
            "show_clock"        => cfg.show_clock = val == "true",
            "show_file_mtime"   => cfg.show_file_mtime = val == "true",
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
            "key_cycle_tab"     => cfg.key_cycle_tab     = val.into(),
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

// ─── Extraction progress ─────────────────────────────────────────────────────
#[derive(Clone)]
struct ExtractionProgress {
    filename:   String,
    current:    u64,
    total:      u64,
    done:       bool,
    error:      Option<String>,
    start_time: Instant,
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
    Help,
    Extracting,
    RunArgs(PathBuf, bool), // (path, prepend_mode)
}

// ─── App ─────────────────────────────────────────────────────────────────────
struct App {
    cfg:         Config,
    theme:       Theme,
    icons:       IconData,
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
    // Video thumbnail preview
    vid_thumb_path:  Option<PathBuf>,   // source video path
    vid_thumb_file:  Option<PathBuf>,   // temp PNG on disk
    vid_thumb_state: Option<StatefulProtocol>,
    vid_thumb_rx:    Option<mpsc::Receiver<PathBuf>>, // signals thumb is ready
    // Live clock string, updated every tick
    clock_str:    String,
    // Archive extraction progress
    extract_progress: Option<ExtractionProgress>,
    extract_rx:       Option<mpsc::Receiver<ExtractionProgress>>,
}
impl App {
    fn new(start: PathBuf, cfg: Config) -> Self {
        let sh = cfg.show_hidden;
        let icons = IconData::load(&cfg.icon_set);
        let theme = Theme::load(&cfg.theme);
        Self {
            theme, icons,
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
            img_path: None,
            img_state: None,
            vid_thumb_path: None,
            vid_thumb_file: None,
            vid_thumb_state: None,
            vid_thumb_rx: None,
            clock_str: current_time_str(),
            extract_progress: None,
            extract_rx: None,
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
        self.clock_str = current_time_str();
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
        // Drain extraction progress channel
        if self.extract_rx.is_some() {
            let mut last: Option<ExtractionProgress> = None;
            let mut done = false;
            if let Some(rx) = &self.extract_rx {
                loop {
                    match rx.try_recv() {
                        Ok(p) => { done = p.done || p.error.is_some(); last = Some(p); }
                        Err(_) => break,
                    }
                }
            }
            if let Some(p) = last {
                if done {
                    if let Some(ref e) = p.error.clone() {
                        self.msg(&format!("Extract error: {}", e), true);
                    } else {
                        self.msg(&format!("Extracted {}", p.filename), false);
                        self.tab_mut().refresh();
                    }
                    self.extract_rx       = None;
                    self.extract_progress = None;
                    self.mode             = InputMode::Normal;
                } else {
                    let st = self.extract_progress.as_ref()
                        .map(|e| e.start_time)
                        .unwrap_or_else(Instant::now);
                    self.extract_progress = Some(ExtractionProgress { start_time: st, ..p });
                }
            }
        }

        // Clear image state if we've navigated away from an image
        let is_image = self.tab().current()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| IMAGE_EXT.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);
        if !is_image && self.img_state.is_some() {
            self.img_path  = None;
            self.img_state = None;
        }

        // Clear video thumbnail state if we've navigated away from a video
        let is_video = self.tab().current()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| VIDEO_EXT.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);
        if !is_video {
            if self.vid_thumb_state.is_some() || self.vid_thumb_rx.is_some() {
                // Clean up temp file
                if let Some(f) = self.vid_thumb_file.take() { let _ = fs::remove_file(f); }
                self.vid_thumb_path  = None;
                self.vid_thumb_state = None;
                self.vid_thumb_rx    = None;
            }
        }

        // Drain video thumbnail channel — load image once ffmpeg signals done
        if self.vid_thumb_rx.is_some() {
            let done = if let Some(rx) = &self.vid_thumb_rx {
                match rx.try_recv() {
                    Ok(thumb_path) => Some(thumb_path),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // ffmpeg failed — show nothing, clean up
                        if let Some(f) = self.vid_thumb_file.take() { let _ = fs::remove_file(f); }
                        self.vid_thumb_rx = None;
                        None
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                }
            } else { None };

            if let Some(thumb_path) = done {
                self.vid_thumb_rx = None;
                if let Some(picker) = self.img_picker.as_mut() {
                    if let Ok(img) = image::open(&thumb_path) {
                        self.vid_thumb_state = Some(picker.new_resize_protocol(img));
                    }
                }
            }
        }
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
        let dst = self.tab().cwd.clone(); let mut errors = vec![];
        for src in &self.yank {
            let target = dst.join(src.file_name().unwrap_or_default());
            let res = if self.yank_cut {
                fs::rename(src, &target).or_else(|_| {
                    if src.is_dir() { copy_dir(src, &target).and_then(|_| fs::remove_dir_all(src)) }
                    else { fs::copy(src, &target).map(|_|()).and_then(|_| fs::remove_file(src)) }
                })
            } else if src.is_dir() { copy_dir(src, &target) }
            else { fs::copy(src, &target).map(|_|()) };
            if let Err(e) = res { errors.push(e.to_string()); }
        }
        if self.yank_cut { self.yank.clear(); self.yank_cut = false; }
        self.tab_mut().refresh();
        if errors.is_empty() { self.msg("Pasted successfully", false); }
        else { self.msg(&format!("Error: {}", errors[0]), true); }
    }
    fn delete_files(&mut self) {
        let targets: Vec<PathBuf> = if !self.tab().selected.is_empty() {
            self.tab().selected.iter().cloned().collect()
        } else if let Some(p) = self.tab().current().cloned() { vec![p] }
        else { return; };
        let mut errors = vec![];
        for p in &targets {
            let res = if p.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) };
            if let Err(e) = res { errors.push(e.to_string()); }
        }
        self.tab_mut().selected.clear(); self.tab_mut().refresh();
        if errors.is_empty() { self.msg(&format!("Deleted {} item(s)", targets.len()), false); }
        else { self.msg(&format!("Error: {}", errors[0]), true); }
    }
    fn open_current(&mut self) {
        let path = match self.tab().current().cloned() { Some(p) => p, None => return };
        if path.is_dir() { self.tab_mut().enter(); return; }
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
        let ext = ext.as_str(); let cfg = self.cfg.clone();
        if IMAGE_EXT.contains(&ext)        { let _ = Command::new(&cfg.opener_image).arg(&path).spawn(); }
        else if VIDEO_EXT.contains(&ext)   { let _ = Command::new(&cfg.opener_video).arg(&path).spawn(); }
        else if AUDIO_EXT.contains(&ext)   { let _ = Command::new(&cfg.opener_audio).arg(&path).spawn(); }
        else if DOC_EXT.contains(&ext)     { let _ = Command::new(&cfg.opener_doc).arg(&path).spawn(); }
        else if JAR_EXT.contains(&ext)     {
            // New terminal window — stdout/stderr visible to user, TUI never touched
            let term = cfg.opener_terminal.clone();
            // Quote the path so spaces in filenames/dirs work correctly
            let path_escaped = path.to_string_lossy().replace("'", "'\''");
            let jar_cmd = format!("{} '{}'", cfg.opener_jar, path_escaped);
            // Pause after exit so the user can read any output before the window closes
            let full_cmd = format!("{}; echo; echo '-- Press Enter to close --'; read _", jar_cmd);
            let _ = Command::new(&term)
                .args(["--", "sh", "-c", &full_cmd])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .stdin(std::process::Stdio::null())
                .spawn();
            self.msg(&format!("Launching {} in terminal", path.file_name().and_then(|n| n.to_str()).unwrap_or("")), false);
        }
        else if ARCHIVE_EXT.contains(&ext) { self.extract_archive(&path.clone()); }
        else {
            // Shell scripts and executables — run in a new terminal window
            let is_script = matches!(ext, "sh"|"bash"|"zsh"|"fish");
            let is_exec = {
                #[cfg(unix)] {
                    use std::os::unix::fs::PermissionsExt;
                    path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
                }
                #[cfg(not(unix))] { false }
            };
            if is_script || is_exec {
                // Open args bar — Enter to launch, Tab toggles prepend/append mode
                self.input_buf.clear();
                self.mode = InputMode::RunArgs(path, false);
            } else {
                self.nvim_path = Some(path);
            }
        }
    }
    fn extract_archive(&mut self, path: &Path) {
        let dst      = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let src_s    = path.to_string_lossy().into_owned();
        let dst_s    = dst.to_string_lossy().into_owned();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let ext      = path.extension()
            .and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
        let name_lower = path.file_name()
            .and_then(|n| n.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();

        // Get total uncompressed size for progress (best-effort)
        let total_bytes: u64 = Self::archive_total_size(&src_s, &ext, &name_lower);

        let (tx, rx) = mpsc::channel::<ExtractionProgress>();
        self.extract_rx       = Some(rx);
        self.extract_progress = Some(ExtractionProgress {
            filename: filename.clone(), current: 0, total: total_bytes,
            done: false, error: None, start_time: Instant::now(),
        });
        self.mode = InputMode::Extracting;

        let fname = filename.clone();
        std::thread::spawn(move || {
            // Choose command + args based on format; pipe stdout for progress parsing
            let result = Self::run_extraction_with_progress(&src_s, &dst_s, &ext, &name_lower, total_bytes, &tx, &fname);
            let _ = tx.send(ExtractionProgress {
                filename: fname, current: total_bytes.max(1), total: total_bytes.max(1),
                done: true, error: result.err().map(|e| e.to_string()),
                start_time: Instant::now(),
            });
        });
    }

    fn archive_total_size(src_s: &str, ext: &str, name_lower: &str) -> u64 {
        // Best-effort: ask each tool for the total uncompressed size
        let out = if ext == "zip" {
            Command::new("unzip").args(["-l", src_s]).output().ok()
        } else if ext == "rar" {
            Command::new("unrar").args(["l", src_s]).output().ok()
        } else if name_lower.ends_with(".tar.gz") || name_lower.ends_with(".tgz")
               || name_lower.ends_with(".tar.bz2") || name_lower.ends_with(".tbz2")
               || name_lower.ends_with(".tar.xz")  || name_lower.ends_with(".tar.zst")
               || ext == "tar" {
            Command::new("tar").args(["--list", "--verbose", "-f", src_s]).output().ok()
        } else if ext == "7z" {
            Command::new("7z").args(["l", src_s]).output().ok()
        } else {
            None
        };
        out.and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            // Parse last number on last non-empty line (most tools put total there)
            text.lines().rev()
                .find(|l| !l.trim().is_empty())
                .and_then(|l| l.split_whitespace().filter_map(|w| w.parse::<u64>().ok()).last())
        }).unwrap_or(0)
    }

    fn run_extraction_with_progress(
        src_s: &str, dst_s: &str, ext: &str, name_lower: &str,
        total: u64, tx: &mpsc::Sender<ExtractionProgress>, fname: &str,
    ) -> std::io::Result<()> {
        use std::io::{BufRead, BufReader};

        // Build command with stdout piped so we can read progress lines
        let mut cmd = if ext == "rar" {
            let mut c = Command::new("unrar");
            c.args(["x", "-o+", src_s, &format!("{}/", dst_s)]);
            c
        } else if ext == "zip" {
            let mut c = Command::new("unzip");
            c.args(["-o", src_s, "-d", dst_s]);
            c
        } else if name_lower.ends_with(".tar.gz") || name_lower.ends_with(".tgz") {
            let mut c = Command::new("tar");
            c.args(["-xzvf", src_s, "-C", dst_s]);
            c
        } else if name_lower.ends_with(".tar.bz2") || name_lower.ends_with(".tbz2") {
            let mut c = Command::new("tar");
            c.args(["-xjvf", src_s, "-C", dst_s]);
            c
        } else if name_lower.ends_with(".tar.xz") {
            let mut c = Command::new("tar");
            c.args(["-xJvf", src_s, "-C", dst_s]);
            c
        } else if name_lower.ends_with(".tar.zst") {
            let mut c = Command::new("tar");
            c.args(["--zstd", "-xvf", src_s, "-C", dst_s]);
            c
        } else if ext == "tar" {
            let mut c = Command::new("tar");
            c.args(["-xvf", src_s, "-C", dst_s]);
            c
        } else if ext == "7z" {
            let mut c = Command::new("7z");
            c.args(["x", src_s, &format!("-o{}", dst_s), "-y"]);
            c
        } else if ext == "gz" {
            let mut c = Command::new("gunzip");
            c.args(["-kf", src_s]);
            c
        } else if ext == "bz2" {
            let mut c = Command::new("bunzip2");
            c.args(["-kf", src_s]);
            c
        } else if ext == "xz" {
            let mut c = Command::new("xz");
            c.args(["-dkf", src_s]);
            c
        } else if ext == "zst" {
            let out_name = std::path::Path::new(src_s)
                .file_stem().and_then(|s| s.to_str()).unwrap_or("out");
            let mut c = Command::new("zstd");
            c.args(["-dkf", src_s, "-o", &format!("{}/{}", dst_s, out_name)]);
            c
        } else {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,
                format!("No extractor for .{}", ext)));
        };

        cmd.stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped())
           .stdin(std::process::Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        let mut extracted: u64 = 0;
        // Each output line = one file extracted; accumulate size estimates
        for line in reader.lines() {
            let line = match line { Ok(l) => l, Err(_) => break };
            // Try to parse a file size from the line (works for tar -v, unzip, 7z, unrar)
            // Format varies: tar prints the path, unzip "  Length  ...", 7z "  Size  ..."
            // We count files as progress units when total_bytes is unknown,
            // and try to add the size when we can parse it.
            let size_in_line: u64 = line.split_whitespace()
                .filter_map(|w| w.parse::<u64>().ok())
                .next()
                .unwrap_or(0);

            if total == 0 {
                // Unknown total — count files
                extracted += 1;
            } else {
                extracted = (extracted + size_in_line.max(1)).min(total);
            }

            let _ = tx.send(ExtractionProgress {
                filename:   fname.to_string(),
                current:    extracted,
                total,
                done:       false,
                error:      None,
                start_time: Instant::now(),
            });
        }

        child.wait()?;
        Ok(())
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
    /// Spawn a background thread that runs ffmpeg to extract a thumbnail frame.
    /// Sends the output path on the channel when done; App::tick() picks it up.
    fn spawn_video_thumb(&mut self, video: PathBuf) {
        // Already generating or already have thumb for this file
        if self.vid_thumb_path.as_deref() == Some(&video) { return; }

        // Clean up previous thumb file if any
        if let Some(f) = self.vid_thumb_file.take() { let _ = fs::remove_file(&f); }
        self.vid_thumb_state = None;
        self.vid_thumb_path  = Some(video.clone());

        // Write thumb to a temp file
        let tmp = env::temp_dir().join(format!(
            "voiddream_thumb_{}.png",
            video.file_name().and_then(|n| n.to_str()).unwrap_or("v")
        ));
        self.vid_thumb_file = Some(tmp.clone());

        let (tx, rx) = mpsc::channel();
        self.vid_thumb_rx = Some(rx);

        std::thread::spawn(move || {
            // ffmpeg -y -i <video> -ss 00:00:03 -vframes 1 -vf scale=640:-1 <thumb>
            let status = Command::new("ffmpeg")
                .args([
                    "-y", "-i", &video.to_string_lossy(),
                    "-ss", "00:00:03",
                    "-vframes", "1",
                    "-vf", "scale=640:-1",
                    &tmp.to_string_lossy(),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if status.map(|s| s.success()).unwrap_or(false) && tmp.exists() {
                let _ = tx.send(tmp);
            }
            // If ffmpeg fails, tx drops → Disconnected signals failure in tick()
        });
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
    let sz = f.area();

    if app.mode == InputMode::Settings {
        draw_settings(f, app, sz);
        return;
    }

    // Layout: tab bar (1) | body (flex) | status (1) | help (1)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(sz);

    draw_tab_bar(f, app, rows[0]);
    draw_body(f, app, rows[1]);
    draw_status_bar(f, app, rows[2]);
    draw_help_bar(f, app, rows[3]);

    // Overlays drawn last
    match &app.mode {
        InputMode::FuzzySearch => draw_fuzzy_overlay(f, app, sz),
        InputMode::Rename(_) | InputMode::NewFile | InputMode::NewDir => draw_input_overlay(f, app, sz),
        InputMode::Confirm => draw_confirm_overlay(f, app, sz),
        InputMode::Help => draw_help_overlay(f, app, sz),
        InputMode::Extracting => draw_extract_overlay(f, app, sz),
        InputMode::RunArgs(..) => draw_runargs_overlay(f, app, sz),
        _ => {}
    }
}

fn draw_tab_bar(f: &mut Frame, app: &App, rect: Rect) {
    // Fill the whole bar with surface0 first
    let bg_block = Block::default().style(st_bg(app.theme.fg_dim, app.theme.bg_panel));
    f.render_widget(bg_block, rect);

    // Right side: clock + date (if enabled)
    let clock_widget_w: u16 = if app.cfg.show_clock {
        // " HH:MM:SS  DD/MM/YYYY " = ~22 chars
        let clock_label = format!(" {} {}   {} {} ", app.icons.chrome.clock, app.clock_str, app.icons.chrome.calendar, current_date_str());
        let w = clock_label.chars().count() as u16;
        let cx = rect.x + rect.width.saturating_sub(w);
        let clock_rect = Rect { x: cx, y: rect.y, width: w, height: 1 };
        f.render_widget(
            Paragraph::new(Span::styled(clock_label, bold_bg(app.theme.accent, app.theme.bg_panel))),
            clock_rect,
        );
        w
    } else { 0 };

    let available = rect.width.saturating_sub(clock_widget_w);
    let mut x = rect.x;
    for (i, tab) in app.tabs.iter().enumerate() {
        let name = tab.cwd.file_name().and_then(|n| n.to_str()).unwrap_or("/");
        let label = format!(" {} {} ", i + 1, name);
        let is_active = i == app.tab_idx;

        let (fg, bg) = if is_active {
            (app.theme.bg_primary, app.theme.accent)
        } else {
            (app.theme.fg_dim, app.theme.bg_panel)
        };

        let w = label.chars().count() as u16;
        if x + w > rect.x + available { break; }

        let tab_rect = Rect { x, y: rect.y, width: w, height: 1 };
        f.render_widget(
            Paragraph::new(Span::styled(&label, if is_active { bold_bg(fg, bg) } else { st_bg(fg, bg) })),
            tab_rect,
        );
        x += w;

        // Separator after each tab
        if x < rect.x + available {
            let sep_rect = Rect { x, y: rect.y, width: 1, height: 1 };
            f.render_widget(
                Paragraph::new(Span::styled(&app.icons.chrome.tab_sep, st_bg(bg, app.theme.bg_panel))),
                sep_rect,
            );
            x += 1;
        }
    }
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
        let ic   = app.icons.file_icon(e, &k);
        let name = e.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let is_cur = *e == tab.cwd;
        let(fg, bg) = if is_cur { (app.theme.bg_primary, app.theme.border) } else { (fg, app.theme.bg_primary) };
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {} ", ic), st_bg(fg, bg)),
            Span::styled(name, st_bg(fg, bg)),
        ]))
    }).collect();

    let block = Block::default().style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
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
            let ic   = app.icons.file_icon(e, &k);
            let name = e.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let size = file_size_str(e);
            let is_cur = tab.state.selected() == Some(i);
            let is_sel = tab.selected.contains(e);
            let (fg, bg) = if is_cur { (app.theme.bg_primary, app.theme.accent) }
                           else if is_sel { (app.theme.accent, app.theme.bg_panel) }
                           else { (fg, app.theme.bg_primary) };
            let mtime_str = if app.cfg.show_file_mtime {
                let (t, d) = format_mtime_split(e);
                format!("  {} {}", t, d)
            } else { String::new() };
            let mtime_w = mtime_str.chars().count() as u16;
            let max_name = (rect.width.saturating_sub(14 + mtime_w)) as usize;
            let name_clipped: String = name.chars().take(max_name).collect();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", ic), st_bg(fg, bg)),
                Span::styled(name_clipped, st_bg(fg, bg)),
                Span::styled(format!(" {:>6}", size), st_bg(if is_cur { app.theme.bg_primary } else { app.theme.fg_muted }, bg)),
                Span::styled(mtime_str, st_bg(app.theme.fg_muted, bg)),
            ]))
        }).collect();

    let title = {
        let name = tab.cwd.file_name().and_then(|n| n.to_str()).unwrap_or("/");
        format!(" {} {} ", app.icons.chrome.dir_icon, name)
    };
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(st(app.theme.bg_popup))
        .title(Span::styled(title, bold(app.theme.border)))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));

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
        let b = Block::default().style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(b, rect);
        return;
    }};

    let k    = file_kind(&current);
    let c    = kind_color(&k, &app.theme);
    let name = current.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let ext  = current.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
    let size = file_size_str(&current);

    // Header: line1 = icon + name, line2 = size
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!(" {} ", app.icons.file_icon(&current, &k)), st_bg(c, app.theme.bg_primary)),
            Span::styled(name, bold_bg(c, app.theme.bg_primary)),
        ]),
        Line::from(vec![
            Span::styled(format!("  {} ", size), st_bg(app.theme.fg_muted, app.theme.bg_primary)),
        ]),
    ]).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
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
                &app.icons.chrome.no_image,
                st_bg(app.theme.fg_muted, app.theme.bg_primary),
            ));
            f.render_widget(p, content_rect);
        }
        return;
    }

    // Directory listing
    if current.is_dir() {
        let entries = list_dir(&current, app.cfg.show_hidden);
        let items: Vec<ListItem> = entries.iter().take(ch).map(|e| {
            let ek = file_kind(e);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", app.icons.file_icon(e, &ek)), st(kind_color(&ek, &app.theme))),
                Span::styled(e.file_name().unwrap_or_default().to_string_lossy().to_string(), st(kind_color(&ek, &app.theme))),
            ]))
        }).collect();
        let block = Block::default().style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(List::new(items).block(block), content_rect);
        return;
    }

    // Video preview — generate thumbnail via ffmpeg in a background thread
    if VIDEO_EXT.contains(&ext.as_str()) {
        app.spawn_video_thumb(current.clone());
        if let Some(state) = app.vid_thumb_state.as_mut() {
            let shifted = Rect {
                x: content_rect.x + 4,
                y: content_rect.y,
                width: content_rect.width,
                height: content_rect.height,
            };
            StatefulImage::new().render(shifted, f.buffer_mut(), state);
        } else {
            let size_str = human_size(current.metadata().map(|m| m.len()).unwrap_or(0));
            let generating = app.vid_thumb_rx.is_some();
            let lines = vec![
                Line::from(Span::styled(
                    if generating { "  Generating thumbnail…" } else { "  (ffmpeg not available)" },
                    st(app.theme.fg_muted),
                )),
                Line::from(Span::styled(format!("  Size:   {}", size_str), st(app.theme.fg_dim))),
                Line::from(Span::styled(format!("  Format: .{}", ext), st(app.theme.fg_dim))),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    format!("  Press Enter to open with {}", app.cfg.opener_video),
                    st(app.theme.fg_muted),
                )),
            ];
            let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
            f.render_widget(p, content_rect);
        }
        return;
    }

    // Audio — show metadata only, no preview
    if AUDIO_EXT.contains(&ext.as_str()) {
        let size_str = human_size(current.metadata().map(|m| m.len()).unwrap_or(0));
        let lines = vec![
            Line::from(Span::styled("  Audio file", bold(app.theme.fg_dim))),
            Line::from(Span::styled(format!("  Size:   {}", size_str), st(app.theme.fg_dim))),
            Line::from(Span::styled(format!("  Format: .{}", ext), st(app.theme.fg_dim))),
            Line::from(Span::raw("")),
            Line::from(Span::styled(
                format!("  Press Enter to open with {}", app.cfg.opener_audio),
                st(app.theme.fg_muted),
            )),
        ];
        let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(p, content_rect);
        return;
    }

    // Text preview — skip large files to avoid blocking the UI
    const PREVIEW_SIZE_LIMIT: u64 = 512 * 1024 * 1024; // 512 MB
    const PREVIEW_READ_BYTES: u64 = 32 * 1024;         // read at most 32 KB

    let file_size = current.metadata().map(|m| m.len()).unwrap_or(0);

    if file_size > PREVIEW_SIZE_LIMIT {
        let lines = vec![
            Line::from(Span::styled("  (large file — preview skipped)", st(app.theme.fg_muted))),
            Line::from(Span::styled(format!("  Size: {}", human_size(file_size)), st(app.theme.fg_dim))),
            Line::from(Span::styled("  Press Enter to open in your editor.", st(app.theme.fg_muted))),
        ];
        let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(p, content_rect);
        return;
    }

    use std::io::Read;
    let preview_text = std::fs::File::open(&current).ok().and_then(|mut f| {
        let mut buf = vec![0u8; PREVIEW_READ_BYTES as usize];
        let n = f.read(&mut buf).ok()?;
        buf.truncate(n);
        if buf.contains(&0u8) { return None; }
        String::from_utf8(buf).ok()
    });

    if let Some(content) = preview_text {
        let lines: Vec<Line> = content.lines().take(ch).map(|l| {
            let clipped: String = l.chars().take(content_rect.width as usize).collect();
            Line::from(Span::styled(clipped, st(app.theme.fg_dim)))
        }).collect();
        let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(p, content_rect);
    } else {
        let lines = vec![
            Line::from(Span::styled("  (binary file)", st(app.theme.fg_muted))),
            Line::from(Span::styled(format!("  Size: {}", human_size(file_size)), st(app.theme.fg_dim))),
        ];
        let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
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
        Span::styled(format!("  {}  ", cwd), bold_bg(app.theme.bg_primary, app.theme.border)),
    ];
    if !app.status_msg.is_empty() {
        let (fg, bg) = if app.status_err { (app.theme.bg_primary, app.theme.warn) } else { (app.theme.bg_primary, app.theme.ok) };
        spans.push(Span::styled(format!("  {}  ", app.status_msg), bold_bg(fg, bg)));
    }
    if !app.yank.is_empty() {
        spans.push(Span::styled(format!("  {} {}  ", app.icons.chrome.yank_icon, app.yank.len()), bold_bg(app.theme.bg_primary, app.theme.accent2)));
    }
    if !tab.selected.is_empty() {
        spans.push(Span::styled(format!("  {} {}  ", app.icons.chrome.sel_icon, tab.selected.len()), bold_bg(app.theme.bg_primary, app.theme.accent)));
    }
    // Right-align count
    let count_str = format!("  {}/{}  ", cur, total);
    let used: usize = spans.iter().map(|s| s.content.len()).sum();
    let pad = (rect.width as usize).saturating_sub(used + count_str.len());
    spans.push(Span::styled(" ".repeat(pad), st_bg(app.theme.fg_dim, app.theme.bg_primary)));
    spans.push(Span::styled(count_str, bold_bg(app.theme.bg_primary, app.theme.bg_popup)));

    let bar = Paragraph::new(Line::from(spans)).style(st_bg(app.theme.fg_dim, app.theme.bg_primary));
    f.render_widget(bar, rect);
}

fn draw_help_bar(f: &mut Frame, app: &App, rect: Rect) {
    let bar = Paragraph::new(Line::from(vec![
        Span::styled(" ?", bold(app.theme.accent)),
        Span::styled(":help ", st(app.theme.fg_muted)),
    ])).style(st_bg(app.theme.fg_muted, app.theme.bg_primary));
    f.render_widget(bar, rect);
}

fn draw_extract_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let ow = (rect.width * 2 / 3).max(50).min(rect.width);
    let oh = 9u16;
    let ox = (rect.width.saturating_sub(ow)) / 2;
    let oy = (rect.height.saturating_sub(oh)) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    let prog = match &app.extract_progress {
        Some(p) => p.clone(),
        None    => return,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .title(Span::styled(
            format!("  \u{f410} Extracting {}  [Esc cancel]  ", prog.filename),
            bold_bg(app.theme.fg_dim, app.theme.bg_primary),
        ))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // Layout: spacer / bar / stats / spacer
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    // ── Progress bar ──────────────────────────────────────────────────────────
    let bar_w = rows[1].width as usize;
    let (pct, filled) = if prog.total > 0 {
        let p = (prog.current * 100 / prog.total).min(100);
        let f = (prog.current * bar_w as u64 / prog.total).min(bar_w as u64) as usize;
        (p, f)
    } else {
        // Unknown total — animate a bouncing block
        let pos = prog.current as usize % (bar_w * 2);
        let pos = if pos < bar_w { pos } else { bar_w * 2 - pos };
        (0, pos.min(bar_w / 5 + 1))
    };

    let filled_str: String = app.icons.chrome.progress_fill.repeat(filled);
    let empty_str:  String = std::iter::repeat('\u{2591}').take(bar_w.saturating_sub(filled)).collect();

    let bar_line = Line::from(vec![
        Span::styled(filled_str, bold(app.theme.accent)),
        Span::styled(empty_str, st(app.theme.bg_popup)),
    ]);
    f.render_widget(Paragraph::new(bar_line), rows[1]);

    // ── Stats line ────────────────────────────────────────────────────────────
    let stats = if prog.total > 0 {
        format!("  {}%   {} / {}",
            pct,
            human_size_u64(prog.current),
            human_size_u64(prog.total),
        )
    } else {
        format!("  {} extracted  (total size unknown)", human_size_u64(prog.current))
    };
    f.render_widget(
        Paragraph::new(Span::styled(stats, st(app.theme.fg_dim))),
        rows[2],
    );

    // ── ETA line — real elapsed + estimated remaining ─────────────────────────
    let elapsed_secs = prog.start_time.elapsed().as_secs();
    let eta_str = if prog.done || pct >= 100 {
        format!("  {}  Done in {}s", app.icons.chrome.done_icon, elapsed_secs)
    } else if prog.total > 0 && prog.current > 0 {
        let rate = prog.current as f64 / elapsed_secs.max(1) as f64;
        let remaining = (prog.total.saturating_sub(prog.current)) as f64 / rate;
        let eta_s = remaining as u64;
        if eta_s < 60 {
            format!("  {}  {}s elapsed  {}  ~{}s left", app.icons.chrome.eta_icon, elapsed_secs, app.icons.chrome.arrow_icon, eta_s)
        } else {
            format!("  {}  {}s elapsed  {}  ~{}m {}s left", app.icons.chrome.eta_icon, elapsed_secs, app.icons.chrome.arrow_icon, eta_s / 60, eta_s % 60)
        }
    } else {
        format!("  {}  {}s elapsed…", app.icons.chrome.eta_icon, elapsed_secs)
    };
    f.render_widget(
        Paragraph::new(Span::styled(eta_str, st(app.theme.fg_muted))),
        rows[3],
    );
}

fn human_size_u64(b: u64) -> String {
    if b >= 1_073_741_824 { format!("{:.1} GB", b as f64 / 1_073_741_824.0) }
    else if b >= 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) }
    else if b >= 1024      { format!("{:.0} KB", b as f64 / 1024.0) }
    else                   { format!("{} B", b) }
}

fn draw_help_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let ow = (rect.width * 3 / 4).max(60).min(rect.width);
    let oh = (rect.height * 4 / 5).max(20).min(rect.height);
    let ox = (rect.width  - ow) / 2;
    let oy = (rect.height - oh) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .title(Span::styled(
            format!("  {} Keybinds  [Esc / ? to close]  ", app.icons.chrome.help_icon),
            bold_bg(app.theme.fg_dim, app.theme.bg_primary),
        ))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
    f.render_widget(block, popup);

    let inner = Rect {
        x: popup.x + 2,
        y: popup.y + 2,
        width: popup.width.saturating_sub(4),
        height: popup.height.saturating_sub(3),
    };

    let cfg = &app.cfg;
    let col_w = (inner.width / 2) as usize;
    let key_col_w = col_w.min(22);

    // Build sections dynamically from live cfg values
    let nav_section = vec![
        ("\u{2191} / \u{2193}".to_string(),  "Move cursor up / down".to_string()),
        ("\u{2192} / Enter".to_string(),     "Open file or enter directory".to_string()),
        ("\u{2190} / Backspace".to_string(), "Go up to parent directory".to_string()),
        ("Page Up / Down".to_string(),       "Jump 10 entries".to_string()),
        ("Home / End".to_string(),           "First / last entry".to_string()),
    ];
    let file_section = vec![
        (cfg.key_select.clone(),             "Select / deselect file".to_string()),
        ("Ctrl+a  /  A".to_string(),         "Select all".to_string()),
        ("Ctrl+r".to_string(),               "Deselect all".to_string()),
        (cfg.key_copy.clone(),               "Copy selected".to_string()),
        (cfg.key_cut.clone(),                "Cut selected".to_string()),
        (cfg.key_paste.clone(),              "Paste".to_string()),
        (cfg.key_delete.clone(),             "Delete selected (with confirm)".to_string()),
        (cfg.key_rename.clone(),             "Rename".to_string()),
        (cfg.key_new_file.clone(),           "New file".to_string()),
        (cfg.key_new_dir.clone(),            "New directory".to_string()),
    ];
    let search_section = vec![
        (cfg.key_search.clone(),             "Fuzzy find (recursive search)".to_string()),
    ];
    let tab_section = vec![
        (cfg.key_new_tab.clone(),            "Open new tab".to_string()),
        (cfg.key_close_tab.clone(),          "Close current tab".to_string()),
        (cfg.key_cycle_tab.clone(),           "Cycle to next tab".to_string()),
    ];
    let app_section = vec![
        (cfg.key_toggle_hidden.clone(),      "Toggle hidden files".to_string()),
        (":".to_string(),                    "Open settings".to_string()),
        ("?".to_string(),                    "Show this help".to_string()),
        (format!("{}  /  Esc", cfg.key_quit), "Quit".to_string()),
    ];

    let sections: &[(&str, &Vec<(String, String)>)] = &[
        (&format!("{}  Navigation", app.icons.chrome.nav_icon),     &nav_section),
        (&format!("{}  File Operations", app.icons.chrome.ops_icon),&file_section),
        (&format!("{}  Search", app.icons.chrome.search_sec_icon), &search_section),
        (&format!("{}  Tabs", app.icons.chrome.tab_sec_icon),           &tab_section),
        (&format!("{}  App", app.icons.chrome.settings_icon), &app_section),
    ];

    let mut lines: Vec<Line> = vec![];
    for (section_title, binds) in sections {
        lines.push(Line::from(Span::styled(format!(" {}", section_title), bold(app.theme.accent))));
        for (key, action) in *binds {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<width$}", key, width = key_col_w), bold(app.theme.border)),
                Span::styled(action.clone(), st(app.theme.fg_dim)),
            ]));
        }
        lines.push(Line::from(Span::raw("")));
    }

    let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
    f.render_widget(p, inner);
}

fn draw_settings(f: &mut Frame, app: &App, rect: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .title(Span::styled(
            format!("  {} Settings  [←→ sections  ↑↓ navigate  Enter edit  S save  Esc close]  ", app.icons.chrome.settings_icon),
            bold_bg(app.theme.fg_dim, app.theme.bg_primary),
        ))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
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
        if *sec == app.settings.section { Line::from(Span::styled(s, bold_bg(app.theme.bg_primary, app.theme.accent))) }
        else { Line::from(Span::styled(s, st_bg(app.theme.fg_dim, app.theme.bg_panel))) }
    }).collect();
    let tabs = Tabs::new(titles)
        .select(match app.settings.section {
            SettingsSection::Behaviour  => 0, SettingsSection::Appearance => 1,
            SettingsSection::Openers    => 2, SettingsSection::Keybinds   => 3,
        })
        .style(st_bg(app.theme.fg_dim, app.theme.bg_panel))
        .divider(Span::raw(""));
    f.render_widget(tabs, inner[0]);

    // Items
    let items = SettingsState::section_items(&app.settings.section);
    let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, (key, label))| {
        let val = SettingsState::get_value(key, &app.cfg);
        let is_cur = i == app.settings.cursor;
        let is_fixed = key.starts_with("fixed_");
        let val_display = if app.settings.editing && is_cur {
            format!("{}{}", app.settings.edit_buf, app.icons.chrome.cursor_block)
        } else { val };
        let (lbg, vbg) = if is_cur { (app.theme.bg_panel, app.theme.bg_popup) } else { (app.theme.bg_primary, app.theme.bg_primary) };
        let (lfg, vfg) = if is_fixed {
            (app.theme.fg_muted, app.theme.fg_muted)
        } else if is_cur {
            (app.theme.fg_primary, app.theme.accent)
        } else {
            (app.theme.fg_dim, app.theme.fg_primary)
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:30}", label), st_bg(lfg, lbg)),
            Span::styled(format!("  {}  ", val_display), st_bg(vfg, vbg)),
        ]))
    }).collect();

    let mut state = ListState::default();
    state.select(Some(app.settings.cursor));
    f.render_stateful_widget(List::new(list_items).style(st_bg(app.theme.fg_primary, app.theme.bg_primary)), inner[1], &mut state);

    // Unsaved indicator
    if app.settings.dirty && !app.settings.dropdown {
        let hint = Paragraph::new("  Unsaved changes — press S to save")
            .style(st_bg(app.theme.bg_primary, app.theme.accent2));
        f.render_widget(hint, inner[2]);
    }

    // ── Dropdown overlay ───────────────────────────────────────────────────────
    if app.settings.dropdown {
        let items  = SettingsState::section_items(&app.settings.section);
        let (k, _) = items[app.settings.cursor];
        let opts   = SettingsState::dropdown_options(k).unwrap_or_default();

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
                (app.theme.bg_primary, app.theme.accent)
            } else {
                (app.theme.fg_primary, app.theme.bg_popup)
            };
            let arrow = if is_cur { format!(" {} ", app.icons.chrome.cursor_arrow) } else { "   ".to_string() };
            ListItem::new(Line::from(vec![
                Span::styled(arrow, st_bg(fg, bg)),
                Span::styled(opt.clone(), st_bg(fg, bg)),
            ]))
        }).collect();

        let dd_block = Block::default()
            .borders(Borders::ALL)
            .border_style(bold(app.theme.accent))
            .title(Span::styled(" \u{2191}\u{2193} select  Enter confirm  Esc cancel ", st(app.theme.fg_muted)))
            .style(st_bg(app.theme.fg_primary, app.theme.bg_popup));

        let mut dd_state = ListState::default();
        dd_state.select(Some(app.settings.dd_cursor));
        f.render_stateful_widget(
            List::new(dd_items).block(dd_block),
            popup,
            &mut dd_state,
        );
    }
}

fn draw_runargs_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let (path, prepend) = match &app.mode {
        InputMode::RunArgs(p, pre) => (p.clone(), *pre),
        _ => return,
    };
    // Only show the filename — path is irrelevant to the user here
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");

    let ow = (rect.width / 2).max(44).min(rect.width);
    let oh = 5u16;
    let ox = (rect.width.saturating_sub(ow)) / 2;
    let oy = (rect.height.saturating_sub(oh)) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    // Outer bordered box
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_panel));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title row: icon + name + mode pill
            Constraint::Length(1), // spacer
            Constraint::Length(1), // input row
        ])
        .split(inner);

    // ── Row 0: icon  filename  [MODE] ────────────────────────────────────────
    let (pill_txt, pill_sty) = if prepend {
        (" PREPEND ", bold_bg(app.theme.bg_primary, app.theme.ok))
    } else {
        (" APPEND  ", bold_bg(app.theme.bg_primary, app.theme.accent))
    };
    // Truncate filename if needed to fit pill
    let pill_w   = pill_txt.len() as u16 + 3; // pill + spaces
    let name_max = (rows[0].width.saturating_sub(pill_w + 4)) as usize;
    let fname_shown: String = if fname.chars().count() > name_max {
        fname.chars().take(name_max.saturating_sub(1)).collect::<String>() + "\u{2026}"
    } else {
        fname.to_string()
    };

    let title_line = Line::from(vec![
        Span::styled(format!(" {} {}", app.icons.chrome.terminal_icon, fname_shown), bold_bg(app.theme.fg_primary, app.theme.bg_panel)),
        Span::styled(
            " ".repeat(rows[0].width.saturating_sub(
                fname_shown.chars().count() as u16 + 4 + pill_w
            ) as usize + 1),
            st_bg(app.theme.bg_panel, app.theme.bg_panel),
        ),
        Span::styled(pill_txt, pill_sty),
        Span::styled(" ", st_bg(app.theme.bg_panel, app.theme.bg_panel)),
    ]);
    f.render_widget(Paragraph::new(title_line), rows[0]);

    // ── Row 2: args input ─────────────────────────────────────────────────────
    let prompt = Span::styled(" args: ", st_bg(app.theme.fg_muted, app.theme.bg_panel));
    let input  = Span::styled(
        format!("{}{}", app.input_buf, app.icons.chrome.cursor_block),
        bold_bg(app.theme.fg_primary, app.theme.bg_panel),
    );
    f.render_widget(Paragraph::new(Line::from(vec![prompt, input])), rows[2]);
}

fn draw_input_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let prompt = match &app.mode {
        InputMode::Rename(_) => &format!("{} Rename", app.icons.chrome.rename_icon),
        InputMode::NewFile   => &format!("{} New File", app.icons.chrome.newfile_icon),
        InputMode::NewDir    => &format!("{} New Directory", app.icons.chrome.newdir_icon),
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
        .border_style(bold(app.theme.accent))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
    let text = format!(" {}: {}{}", prompt, app.input_buf, app.icons.chrome.cursor_block);
    let p = Paragraph::new(text).block(block).style(bold(app.theme.fg_primary));
    f.render_widget(p, popup);
}

fn draw_confirm_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let msg = "  Delete selected items? [y/N]  ";
    let w = msg.len() as u16;
    let h = 1u16;
    let x = (rect.width - w) / 2;
    let y = rect.height / 2;
    let popup = Rect { x, y, width: w, height: h };
    f.render_widget(Clear, popup);
    let p = Paragraph::new(msg).style(bold_bg(app.theme.bg_primary, app.theme.warn));
    f.render_widget(p, popup);
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
        .border_style(bold(app.theme.accent))
        .title(Span::styled(
            format!("  {} Fuzzy Find  [Esc cancel  ↑↓ navigate  Enter jump]  ", app.icons.chrome.search_icon),
            bold(app.theme.fg_dim),
        ))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));

    // Live count — always shows current number even while streaming
    let count_str = if app.fuzzy_loading {
        format!("  {} matches, indexing\u{2026}", app.fuzzy_results.len())
    } else {
        format!("  {} matches", app.fuzzy_results.len())
    };
    let search_text = Line::from(vec![
        Span::styled("  ", st(app.theme.fg_muted)),
        Span::styled(&app.fuzzy_query, bold(app.theme.fg_primary)),
        Span::styled(&app.icons.chrome.cursor_block, bold(app.theme.accent)),
        Span::styled(&count_str, st(app.theme.fg_muted)),
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
            let ic   = app.icons.file_icon(path, &k);
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let rel  = path.strip_prefix(&app.tab().cwd)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            let is_cur = i == cursor;  // i is the absolute index, cursor is absolute — correct
            let ns = if is_cur { bold_bg(app.theme.bg_primary, app.theme.accent) } else { st(c) };
            let ps = if is_cur { st_bg(app.theme.fg_muted, app.theme.accent) } else { st(app.theme.fg_muted) };
            let prefix = if is_cur { format!(" {} ", app.icons.chrome.cursor_arrow) } else { "   ".to_string() };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, if is_cur { bold(app.theme.accent) } else { st(app.theme.bg_popup) }),
                Span::styled(format!("{} ", ic), ns),
                Span::styled(name, ns),
                Span::styled(format!("  {}", rel), ps),
            ]))
        }).collect();

    let list_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(st(app.theme.bg_popup))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
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
        InputMode::RunArgs(..) => { handle_runargs_key(app, key); return false; }
        InputMode::Confirm => {
            app.mode = InputMode::Normal;
            if matches!(key, KeyCode::Char('y')|KeyCode::Char('Y')) { app.delete_files(); }
            return false;
        }
        InputMode::Help => {
            if matches!(key, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')) {
                app.mode = InputMode::Normal;
            }
            return false;
        }
        InputMode::Extracting => {
            // Esc cancels (best-effort — process may still finish in bg)
            if key == KeyCode::Esc {
                app.extract_rx       = None;
                app.extract_progress = None;
                app.mode             = InputMode::Normal;
            }
            return false;
        }
    }
    let cfg = &app.cfg;
    let _ch = match key { KeyCode::Char(c) => Some(c), _ => None };

    match key {
        KeyCode::Up        => app.tab_mut().move_cursor(-1),
        KeyCode::Down      => app.tab_mut().move_cursor(1),
        KeyCode::Left  | KeyCode::Backspace => app.tab_mut().leave(),
        KeyCode::Right | KeyCode::Enter     => app.open_current(),
        KeyCode::PageUp    => app.tab_mut().move_cursor(-10),
        KeyCode::PageDown  => app.tab_mut().move_cursor(10),
        KeyCode::Home      => { app.tab_mut().state.select(Some(0)); }
        KeyCode::End       => { let n=app.tab().visible().len(); if n>0 { app.tab_mut().state.select(Some(n-1)); } }
        KeyCode::Tab if app.cfg.key_cycle_tab == "Tab" => {
            app.tab_idx = (app.tab_idx + 1) % app.tabs.len();
        }
        KeyCode::Char(' ') if cfg.key_select == "Space" => app.tab_mut().toggle_select(),
        KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) && cfg.key_select_all == "Ctrl+a" => app.tab_mut().select_all(),
        KeyCode::Char('A') => app.tab_mut().select_all(),
        KeyCode::Char('r') if mods.contains(KeyModifiers::CONTROL) => app.tab_mut().deselect_all(),
        KeyCode::Char(c) => {
            let s = c.to_string();
            if s == cfg.key_copy {
                if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
                else { app.yank_files(false); }
            } else if s == cfg.key_cut {
                if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
                else { app.yank_files(true); }
            } else if s == cfg.key_paste {
                app.paste_files();
            } else if s == cfg.key_delete {
                if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
                else { app.mode = InputMode::Confirm; }
            } else if s == cfg.key_rename {
                if let Some(p) = app.tab().current().cloned() {
                    let name = p.file_name().and_then(|n|n.to_str()).unwrap_or("").to_string();
                    app.input_buf = name.clone(); app.mode = InputMode::Rename(name);
                }
            } else if s == cfg.key_new_file {
                app.input_buf.clear(); app.mode = InputMode::NewFile;
            } else if s == cfg.key_new_dir {
                app.input_buf.clear(); app.mode = InputMode::NewDir;
            } else if s == cfg.key_search {
                app.open_fuzzy();
            } else if s == cfg.key_toggle_hidden {
                let h = !app.tab().show_hidden;
                app.tab_mut().show_hidden = h; app.tab_mut().refresh();
                app.msg(if h {"Hidden files shown"} else {"Hidden files hidden"}, false);
            } else if s == cfg.key_cycle_tab {
                app.tab_idx = (app.tab_idx + 1) % app.tabs.len();
            } else if s == cfg.key_new_tab {
                app.new_tab();
            } else if s == cfg.key_close_tab {
                app.close_tab();
            } else if s == cfg.key_quit {
                return true;
            } else if c == ':' {
                app.mode = InputMode::Settings;
            } else if c == '?' {
                app.mode = InputMode::Help;
            }
        }
        KeyCode::Esc => return true,
        _ => {}
    }
    false
}

fn handle_settings_key(app: &mut App, key: KeyCode, _mods: KeyModifiers) -> bool {
    // ── Dropdown mode ──────────────────────────────────────────────────────────
    if app.settings.dropdown {
        let items  = SettingsState::section_items(&app.settings.section);
        let (k, _) = items[app.settings.cursor];
        let opts   = SettingsState::dropdown_options(k).unwrap_or_default();
        match key {
            KeyCode::Esc => { app.settings.dropdown = false; }
            KeyCode::Up  => { if app.settings.dd_cursor > 0 { app.settings.dd_cursor -= 1; } }
            KeyCode::Down => { if app.settings.dd_cursor + 1 < opts.len() { app.settings.dd_cursor += 1; } }
            KeyCode::Enter => {
                let chosen = opts[app.settings.dd_cursor].clone();
                SettingsState::set_value(k, &chosen, &mut app.cfg);
                app.settings.dropdown = false;
                app.settings.dirty    = true;
                // Apply theme/icon live immediately
                app.theme = Theme::load(&app.cfg.theme);
                app.icons = IconData::load(&app.cfg.icon_set);
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
            if k.starts_with("fixed_") {
                // Fixed keys are informational — not configurable
            } else if SettingsState::dropdown_options(k).is_some() {
                // Open dropdown — pre-select current value
                let cur_val = SettingsState::get_value(k, &app.cfg);
                let opts    = SettingsState::dropdown_options(k).unwrap();
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
                    // Reload user themes in case themes.json was edited externally
                                        app.theme = Theme::load(&app.cfg.theme);
                    app.icons = IconData::load(&app.cfg.icon_set);
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

fn handle_runargs_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.mode = InputMode::Normal;
            app.input_buf.clear();
        }
        KeyCode::Tab => {
            // Toggle between append (args after exe) and prepend (args before exe)
            if let InputMode::RunArgs(ref _p, ref mut prepend) = app.mode {
                *prepend = !*prepend;
            }
        }
        KeyCode::Enter => {
            let args_str = app.input_buf.clone();
            let (path, prepend) = match std::mem::replace(&mut app.mode, InputMode::Normal) {
                InputMode::RunArgs(p, pre) => (p, pre),
                _ => return,
            };
            app.input_buf.clear();

            let cfg  = app.cfg.clone();
            let term = cfg.opener_terminal.clone();
            let ext  = path.extension().and_then(|e| e.to_str())
                .map(|s| s.to_lowercase()).unwrap_or_default();
            let is_script = matches!(ext.as_str(), "sh"|"bash"|"zsh"|"fish");
            let is_exec = {
                #[cfg(unix)] {
                    use std::os::unix::fs::PermissionsExt;
                    path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
                }
                #[cfg(not(unix))] { false }
            };
            let path_escaped = path.to_string_lossy().replace("'", "'\\''");

            // Build the base command (interpreter + path, or just path)
            let base_cmd = if is_script && !is_exec {
                match ext.as_str() {
                    "fish" => format!("fish '{}'", path_escaped),
                    "zsh"  => format!("zsh '{}'",  path_escaped),
                    "bash" => format!("bash '{}'", path_escaped),
                    _      => format!("sh '{}'",   path_escaped),
                }
            } else {
                format!("'{}'", path_escaped)
            };

            // Combine with user args
            let run_cmd = if args_str.is_empty() {
                base_cmd
            } else if prepend {
                format!("{} {}", args_str, base_cmd)
            } else {
                format!("{} {}", base_cmd, args_str)
            };

            let full_cmd = format!("{}; echo; echo '-- Press Enter to close --'; read _", run_cmd);
            let work_dir = path.parent().unwrap_or(std::path::Path::new("/")).to_path_buf();
            let _ = Command::new(&term)
                .args(["--", "sh", "-c", &full_cmd])
                .current_dir(&work_dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .stdin(std::process::Stdio::null())
                .spawn();
            app.msg(&format!("Running {} in terminal",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("")), false);
        }
        KeyCode::Backspace => { app.input_buf.pop(); }
        KeyCode::Char(c)   => app.input_buf.push(c),
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
    // ── Auto-create data directories on first run ──────────────────────────
    // System dirs — silently ignored if not root (package installer handles these)
    let _ = fs::create_dir_all("/usr/share/VoidDream/themes");
    let _ = fs::create_dir_all("/usr/share/VoidDream/icons");

    // User dirs — always created, no special permissions needed
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(&home).join(".local").join("share").join("VoidDream");
        let _ = fs::create_dir_all(base.join("themes"));
        let _ = fs::create_dir_all(base.join("icons"));
        let _ = fs::create_dir_all(PathBuf::from(&home).join(".config").join("VoidDream"));
    }

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
    // Restore terminal default background color on exit
    let _ = write!(term.backend_mut(), "\x1b]111\x07");
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    Ok(())
}
