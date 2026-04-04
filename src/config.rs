// config.rs — Theme, IconData, Chrome, Config, SettingsState, style helpers
use std::{fs, path::{Path, PathBuf}, process::Command};
use anyhow::Result;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use crate::types::FileKind;

// ─── Style helpers ─────────────────────────────────────────────────────────────
pub fn st(fg: Color) -> Style { Style::default().fg(fg) }
pub fn st_bg(fg: Color, bg: Color) -> Style { Style::default().fg(fg).bg(bg) }
pub fn bold(fg: Color) -> Style { Style::default().fg(fg).add_modifier(Modifier::BOLD) }
pub fn bold_bg(fg: Color, bg: Color) -> Style {
    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
}

#[derive(Deserialize, Serialize, Default)]
pub struct ThemeFile {
    #[serde(default)]
    pub palette: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub roles:   std::collections::HashMap<String, String>,
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
pub fn xdg_data_home() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() { return PathBuf::from(xdg); }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".local").join("share")
}

pub fn parse_hex(s: &str) -> Color {
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

    #[allow(dead_code)]
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

pub fn kind_color(k: &FileKind, t: &Theme) -> Color {
    match k {
        FileKind::Dir     => t.kind_dir,
        FileKind::Image   => t.kind_image,
        FileKind::Video   => t.kind_video,
        FileKind::Audio   => t.kind_audio,
        FileKind::Archive => t.kind_archive,
        FileKind::Jar     => t.kind_jar,
        FileKind::Html    => t.kind_code,
        FileKind::Doc     => t.kind_doc,
        FileKind::Code    => t.kind_code,
        FileKind::Exec    => t.kind_exec,
        FileKind::Symlink => t.kind_symlink,
        FileKind::Other   => t.kind_other,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ICON SYSTEM
// ────────────────────────────────────────────────────────────────────────────
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
    #[serde(default)] by_name:    std::collections::HashMap<String, String>,
    #[serde(default)] by_ext:     std::collections::HashMap<String, String>,
    #[serde(default)] named_dirs: std::collections::HashMap<String, String>,
    #[serde(default)] chrome:     ChromeJson,
}

#[derive(Deserialize, Default)]
pub struct ChromeJson {
    pub tab_sep:         Option<String>,
    pub clock:           Option<String>,
    pub calendar:        Option<String>,
    pub cursor_arrow:    Option<String>,
    pub yank_icon:       Option<String>,
    pub sel_icon:        Option<String>,
    pub dir_icon:        Option<String>,
    pub no_image:        Option<String>,
    pub progress_fill:   Option<String>,
    pub done_icon:       Option<String>,
    pub eta_icon:        Option<String>,
    pub arrow_icon:      Option<String>,
    pub help_icon:       Option<String>,
    pub nav_icon:        Option<String>,
    pub ops_icon:        Option<String>,
    pub tab_sec_icon:    Option<String>,
    pub rename_icon:     Option<String>,
    pub newfile_icon:    Option<String>,
    pub newdir_icon:     Option<String>,
    pub search_icon:     Option<String>,
    pub terminal_icon:   Option<String>,
    pub settings_icon:   Option<String>,
    pub search_sec_icon: Option<String>,
    pub cursor_block:    Option<String>,
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

    #[allow(dead_code)]
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
            FileKind::Html    => "🌐",
            FileKind::Code    => self.code.as_deref().unwrap_or(""),
            FileKind::Exec    => self.exec.as_deref().unwrap_or(""),
            FileKind::Other   => self.other.as_deref().unwrap_or(""),
            _                 => "",
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// STYLE HELPERS
// ────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub show_hidden:       bool,
    pub date_format:       String,
    pub col_parent:        u16,
    pub col_files:         u16,
    pub theme:             String,
    pub icon_set:          String,
    pub opener_browser:    String,
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
    pub show_clock:        bool,
    pub show_file_mtime:   bool,
    pub language:          String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_hidden: true, date_format: "%d/%m/%Y %H:%M".into(),
            col_parent: 20, col_files: 37,
            theme: "catppuccin-macchiato".into(), icon_set: "nerdfont".into(),
            opener_browser: Self::detect_browser(),
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
            language: "English (UK)".into(),
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

    /// Detect the best available browser on the system.
    pub fn detect_browser() -> String {
        let candidates = ["xdg-open", "librewolf", "firefox", "zen-browser",
                          "chromium", "google-chrome-stable", "microsoft-edge-stable"];
        for b in candidates {
            if Command::new("which").arg(b)
                .output().map(|o| o.status.success()).unwrap_or(false)
            {
                return b.to_string();
            }
        }
        "xdg-open".to_string()
    }


    /// Detect the best available browser on the system.



    pub fn load() -> Self {
        let p = Self::config_path();
        if p.exists() {
            match fs::read_to_string(&p).map(|d| serde_json::from_str::<Self>(&d)) {
                Ok(Ok(cfg)) => return cfg,
                Ok(Err(e))  => eprintln!("VoidDream: config parse error: {e}"),
                Err(e)      => eprintln!("VoidDream: config read error: {e}"),
            }
        }
        let c = Self::default();
        let _ = c.save();
        c
    }
    pub fn save(&self) -> Result<()> {
        let p = Self::config_path();
        if let Some(par) = p.parent() { fs::create_dir_all(par)?; }
        fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum SettingsSection { Behaviour, Appearance, Openers, Keybinds, About }

pub struct SettingsState {
    pub section:    SettingsSection,
    pub cursor:     usize,
    pub editing:    bool,
    pub edit_buf:   String,
    pub dirty:      bool,
    pub dropdown:   bool,   // true when showing a dropdown picker
    pub dd_cursor:  usize,  // selected index within the dropdown
}
impl SettingsState {
    pub fn new() -> Self {
        Self { section: SettingsSection::Behaviour, cursor: 0, editing: false, edit_buf: String::new(), dirty: false, dropdown: false, dd_cursor: 0 }
    }
    pub fn section_items(s: &SettingsSection) -> Vec<(&'static str, &'static str)> {
        match s {
            SettingsSection::Behaviour  => vec![
                ("language",         "Language"),
                ("show_hidden","Show hidden files"), ("date_format","Date format"),
                ("show_clock","Show clock in tab bar"),
                ("show_file_mtime","Show file date/time in file list"),
            ],
            SettingsSection::Appearance => vec![
                ("col_parent","Parent pane width (%)"), ("col_files","Files pane width (%)"),
                ("theme","Theme"), ("icon_set","Icon theme"),
            ],
            SettingsSection::Openers    => vec![
                ("opener_browser","Browser (HTML)"), ("opener_image","Image"), ("opener_video","Video"), ("opener_audio","Audio"),
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
                ("fixed_drives",     "Drive / USB / phone manager  [D]"),
                ("fixed_drive_m",    "  Mount selected device  [m]"),
                ("fixed_drive_u",    "  Unmount selected device  [u]"),
                ("fixed_drive_r",    "  Refresh device list  [r]"),
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
                ("fixed_open_with",  "Open with…"),
                ("fixed_settings",   "Open settings"),
                ("fixed_help",       "Show help"),
                ("fixed_quit2",      "Quit (alt)"),
            ],
            SettingsSection::About => vec![
                ("fixed_about_app",     "Application"),
                ("fixed_about_ver",     "Version"),
                ("fixed_about_author",  "Author"),
                ("fixed_about_license", "License"),
                ("fixed_about_repo",    "Repository"),
            ],
        }
    }
    pub fn dropdown_options(key: &str) -> Option<Vec<String>> {
        match key {
            "language"    => Some(vec![
                "English (UK)".into(), "Română".into(),  "Français".into(), "Deutsch".into(),
                "Español".into(),      "Italiano".into(), "Português".into(),"Русский".into(),
                "日本語".into(),         "中文".into(),     "한국어".into(),    "العربية".into(),
            ]),
            "theme"       => Some(Theme::installed_names()),
            "icon_set"    => Some(vec!["nerdfont","emoji","minimal","none"].iter().map(|s| s.to_string()).collect()),
            "show_hidden"      => Some(vec!["true".to_string(), "false".to_string()]),
            "show_clock"       => Some(vec!["true".to_string(), "false".to_string()]),
            "show_file_mtime"  => Some(vec!["true".to_string(), "false".to_string()]),
            _ => None,
        }
    }
    pub fn get_value(key: &str, cfg: &Config) -> String {
        match key {
            "show_hidden"       => cfg.show_hidden.to_string(),
            "show_clock"        => cfg.show_clock.to_string(),
            "show_file_mtime"   => cfg.show_file_mtime.to_string(),
            "date_format"       => cfg.date_format.clone(),
            "col_parent"        => cfg.col_parent.to_string(),
            "col_files"         => cfg.col_files.to_string(),
            "language"          => cfg.language.clone(),
            "theme"             => cfg.theme.clone(),
            "icon_set"          => cfg.icon_set.clone(),
            "opener_browser"    => cfg.opener_browser.clone(),
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
            "fixed_drives"      => "D".into(),
            "fixed_drive_m"     => "m".into(),
            "fixed_drive_u"     => "u".into(),
            "fixed_drive_r"     => "r".into(),
            "key_new_tab"       => cfg.key_new_tab.clone(),
            "key_close_tab"     => cfg.key_close_tab.clone(),
            "key_cycle_tab"     => cfg.key_cycle_tab.clone(),
            "key_select"        => cfg.key_select.clone(),
            "key_select_all"    => cfg.key_select_all.clone(),
            "fixed_arc_rar"   => "unrar (system)".into(),
            "fixed_arc_zip"   => "native Rust".into(),
            "fixed_arc_tgz"   => "native Rust".into(),
            "fixed_arc_tbz2"  => "native Rust".into(),
            "fixed_arc_txz"   => "native Rust".into(),
            "fixed_arc_tzst"  => "native Rust".into(),
            "fixed_arc_tar"   => "native Rust".into(),
            "fixed_arc_gz"    => "native Rust".into(),
            "fixed_arc_bz2"   => "native Rust".into(),
            "fixed_arc_xz"    => "native Rust".into(),
            "fixed_arc_zst"   => "native Rust".into(),
            "fixed_arc_7z"    => "native Rust".into(),
            "fixed_nav"        => "\u{2191} / \u{2193}  (fixed)".into(),
            "fixed_open"       => "\u{2192} / Enter  (fixed)".into(),
            "fixed_up"         => "\u{2190} / Backspace  (fixed)".into(),
            "fixed_pgupdown"   => "Page Up / Page Down  (fixed)".into(),
            "fixed_homeend"    => "Home / End  (fixed)".into(),
            "fixed_deselect"   => "Ctrl+r  (fixed)".into(),
            "fixed_sel_all2"   => "A  (fixed)".into(),
            "fixed_open_with"  => "k  (fixed)".into(),
            "fixed_settings"   => ":  (fixed)".into(),
            "fixed_help"       => "?  (fixed)".into(),
            "fixed_quit2"      => "Esc  (fixed)".into(),
            "fixed_about_app"     => "VoidDream".into(),
            "fixed_about_ver"     => "0.1.6".into(),
            "fixed_about_author"  => "FemBoyGamerTechGuy".into(),
            "fixed_about_license" => "GPL-3.0-or-later".into(),
            "fixed_about_repo"    => "github.com/FemBoyGamerTechGuy/VoidDream".into(),
            _ => String::new(),
        }
    }
    pub fn set_value(key: &str, val: &str, cfg: &mut Config) {
        match key {
            "show_hidden"       => cfg.show_hidden = val == "true",
            "show_clock"        => cfg.show_clock = val == "true",
            "show_file_mtime"   => cfg.show_file_mtime = val == "true",
            "date_format"       => cfg.date_format = val.into(),
            "col_parent"        => { if let Ok(n) = val.parse::<u16>() { cfg.col_parent = n.clamp(10,40); } }
            "col_files"         => { if let Ok(n) = val.parse::<u16>() { cfg.col_files  = n.clamp(20,60); } }
            "language"          => cfg.language = val.into(),
            "theme"             => cfg.theme    = val.into(),
            "icon_set"          => cfg.icon_set = val.into(),
            "opener_browser"    => cfg.opener_browser  = val.into(),
            "opener_image"      => cfg.opener_image    = val.into(),
            "opener_video"      => cfg.opener_video    = val.into(),
            "opener_audio"      => cfg.opener_audio    = val.into(),
            "opener_doc"        => cfg.opener_doc      = val.into(),
            "opener_editor"     => cfg.opener_editor   = val.into(),
            "opener_jar"        => cfg.opener_jar      = val.into(),
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
            "key_cycle_tab"     => cfg.key_cycle_tab     = val.into(),
            "key_select"        => cfg.key_select        = val.into(),
            "key_select_all"    => cfg.key_select_all    = val.into(),
            _ => {}
        }
    }
}

// ─── Tab ─────────────────────────────────────────────────────────────────────
