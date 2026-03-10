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
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
    env,
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserThemeColors {
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
}

/// A resolved user theme: name (from filename) + parsed colors.
#[derive(Clone, Debug)]
pub struct UserThemeEntry {
    pub name:   String,
    pub colors: UserThemeColors,
}

impl UserThemeEntry {
    /// Returns ~/.local/share/fd-files/themes/
    pub fn themes_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        PathBuf::from(home).join(".local").join("share").join("fd-files").join("themes")
    }

    /// Scan the themes directory and load every *.json file.
    /// The theme name is the filename stem (e.g. "Fire Aura.json" → "Fire Aura").
    /// Files that fail to parse are silently skipped.
    pub fn load_all() -> Vec<Self> {
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

    /// Convert to a runtime Theme.
    pub fn to_theme(&self) -> Theme {
        let c = &self.colors;
        Theme {
            base:     Self::parse_hex(&c.base),
            surface0: Self::parse_hex(&c.surface0),
            surface1: Self::parse_hex(&c.surface1),
            overlay0: Self::parse_hex(&c.overlay0),
            text:     Self::parse_hex(&c.text),
            subtext:  Self::parse_hex(&c.subtext),
            mauve:    Self::parse_hex(&c.mauve),
            blue:     Self::parse_hex(&c.blue),
            teal:     Self::parse_hex(&c.teal),
            green:    Self::parse_hex(&c.green),
            red:      Self::parse_hex(&c.red),
            yellow:   Self::parse_hex(&c.yellow),
            pink:     Self::parse_hex(&c.pink),
        }
    }
}

// ─── Theme ────────────────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct Theme {
    base:     Color, surface0: Color, surface1: Color,
    overlay0: Color, text:     Color, subtext:  Color,
    mauve:    Color, blue:     Color, teal:     Color,
    green:    Color, red:      Color, yellow:   Color,
    pink:     Color,
}

impl Theme {
    /// Resolve a theme by name, checking user themes first, then built-ins.
    fn resolve(name: &str, user_themes: &[UserThemeEntry]) -> Self {
        if let Some(ut) = user_themes.iter().find(|t| t.name == name) {
            return ut.to_theme();
        }
        Self::by_name(name)
    }

    /// All theme names (built-in + user), deduplicated (user wins on collision).
    fn all_names_merged(user_themes: &[UserThemeEntry]) -> Vec<String> {
        let mut names: Vec<String> = Self::builtin_names()
            .iter()
            .filter(|&&n| !user_themes.iter().any(|u| u.name == n))
            .map(|&s| s.to_string())
            .collect();
        for ut in user_themes {
            names.push(ut.name.clone());
        }
        names
    }

    fn by_name(name: &str) -> Self {
        match name {
            "catppuccin-latte" => Self {
                base:     Color::Rgb(239,241,245), surface0: Color::Rgb(204,208,218),
                surface1: Color::Rgb(188,192,204), overlay0: Color::Rgb(156,160,176),
                text:     Color::Rgb(76,79,105),   subtext:  Color::Rgb(92,95,119),
                mauve:    Color::Rgb(136,57,239),  blue:     Color::Rgb(30,102,245),
                teal:     Color::Rgb(23,146,153),  green:    Color::Rgb(64,160,43),
                red:      Color::Rgb(210,15,57),   yellow:   Color::Rgb(223,142,29),
                pink:     Color::Rgb(234,118,203),
            },
            "catppuccin-frappe" => Self {
                base:     Color::Rgb(48,52,70),    surface0: Color::Rgb(65,69,89),
                surface1: Color::Rgb(81,87,109),   overlay0: Color::Rgb(115,121,148),
                text:     Color::Rgb(198,208,245), subtext:  Color::Rgb(181,191,226),
                mauve:    Color::Rgb(202,158,230), blue:     Color::Rgb(140,170,238),
                teal:     Color::Rgb(129,200,190), green:    Color::Rgb(166,209,137),
                red:      Color::Rgb(231,130,132), yellow:   Color::Rgb(229,200,144),
                pink:     Color::Rgb(244,184,228),
            },
            "catppuccin-mocha" => Self {
                base:     Color::Rgb(30,30,46),    surface0: Color::Rgb(49,50,68),
                surface1: Color::Rgb(69,71,90),    overlay0: Color::Rgb(108,112,134),
                text:     Color::Rgb(205,214,244), subtext:  Color::Rgb(166,173,200),
                mauve:    Color::Rgb(203,166,247), blue:     Color::Rgb(137,180,250),
                teal:     Color::Rgb(148,226,213), green:    Color::Rgb(166,227,161),
                red:      Color::Rgb(243,139,168), yellow:   Color::Rgb(249,226,175),
                pink:     Color::Rgb(245,194,231),
            },
            "tokyo-night" => Self {
                base:     Color::Rgb(26,27,38),    surface0: Color::Rgb(36,40,59),
                surface1: Color::Rgb(52,59,88),    overlay0: Color::Rgb(86,95,137),
                text:     Color::Rgb(192,202,245), subtext:  Color::Rgb(169,177,214),
                mauve:    Color::Rgb(187,154,247), blue:     Color::Rgb(122,162,247),
                teal:     Color::Rgb(42,195,222),  green:    Color::Rgb(158,206,106),
                red:      Color::Rgb(247,118,142), yellow:   Color::Rgb(224,175,104),
                pink:     Color::Rgb(255,121,198),
            },
            "tokyo-night-storm" => Self {
                base:     Color::Rgb(35,38,52),    surface0: Color::Rgb(42,46,62),
                surface1: Color::Rgb(57,62,80),    overlay0: Color::Rgb(86,95,137),
                text:     Color::Rgb(192,202,245), subtext:  Color::Rgb(169,177,214),
                mauve:    Color::Rgb(187,154,247), blue:     Color::Rgb(122,162,247),
                teal:     Color::Rgb(42,195,222),  green:    Color::Rgb(158,206,106),
                red:      Color::Rgb(247,118,142), yellow:   Color::Rgb(224,175,104),
                pink:     Color::Rgb(255,121,198),
            },
            "tokyo-night-light" => Self {
                base:     Color::Rgb(213,214,219), surface0: Color::Rgb(195,197,210),
                surface1: Color::Rgb(180,182,198), overlay0: Color::Rgb(136,139,163),
                text:     Color::Rgb(52,59,88),    subtext:  Color::Rgb(86,95,137),
                mauve:    Color::Rgb(122,78,203),  blue:     Color::Rgb(52,101,196),
                teal:     Color::Rgb(0,150,170),   green:    Color::Rgb(74,153,46),
                red:      Color::Rgb(194,48,74),   yellow:   Color::Rgb(143,110,37),
                pink:     Color::Rgb(175,65,148),
            },
            "gruvbox-dark" => Self {
                base:     Color::Rgb(40,40,40),    surface0: Color::Rgb(60,56,54),
                surface1: Color::Rgb(80,73,69),    overlay0: Color::Rgb(124,111,100),
                text:     Color::Rgb(235,219,178), subtext:  Color::Rgb(213,196,161),
                mauve:    Color::Rgb(211,134,155), blue:     Color::Rgb(131,165,152),
                teal:     Color::Rgb(142,192,124), green:    Color::Rgb(184,187,38),
                red:      Color::Rgb(251,73,52),   yellow:   Color::Rgb(250,189,47),
                pink:     Color::Rgb(211,134,155),
            },
            "gruvbox-light" => Self {
                base:     Color::Rgb(251,241,199), surface0: Color::Rgb(235,219,178),
                surface1: Color::Rgb(213,196,161), overlay0: Color::Rgb(189,174,147),
                text:     Color::Rgb(60,56,54),    subtext:  Color::Rgb(80,73,69),
                mauve:    Color::Rgb(143,63,113),  blue:     Color::Rgb(69,133,136),
                teal:     Color::Rgb(121,116,14),  green:    Color::Rgb(121,116,14),
                red:      Color::Rgb(204,36,29),   yellow:   Color::Rgb(215,153,33),
                pink:     Color::Rgb(143,63,113),
            },
            "nord" => Self {
                base:     Color::Rgb(46,52,64),    surface0: Color::Rgb(59,66,82),
                surface1: Color::Rgb(67,76,94),    overlay0: Color::Rgb(76,86,106),
                text:     Color::Rgb(236,239,244), subtext:  Color::Rgb(229,233,240),
                mauve:    Color::Rgb(180,142,173), blue:     Color::Rgb(136,192,208),
                teal:     Color::Rgb(143,188,187), green:    Color::Rgb(163,190,140),
                red:      Color::Rgb(191,97,106),  yellow:   Color::Rgb(235,203,139),
                pink:     Color::Rgb(180,142,173),
            },
            "dracula" => Self {
                base:     Color::Rgb(40,42,54),    surface0: Color::Rgb(50,52,65),
                surface1: Color::Rgb(68,71,90),    overlay0: Color::Rgb(98,114,164),
                text:     Color::Rgb(248,248,242), subtext:  Color::Rgb(191,192,197),
                mauve:    Color::Rgb(189,147,249), blue:     Color::Rgb(139,233,253),
                teal:     Color::Rgb(80,250,123),  green:    Color::Rgb(80,250,123),
                red:      Color::Rgb(255,85,85),   yellow:   Color::Rgb(241,250,140),
                pink:     Color::Rgb(255,121,198),
            },
            "rose-pine" => Self {
                base:     Color::Rgb(25,23,36),    surface0: Color::Rgb(31,29,46),
                surface1: Color::Rgb(38,35,58),    overlay0: Color::Rgb(110,106,134),
                text:     Color::Rgb(224,222,244), subtext:  Color::Rgb(144,140,170),
                mauve:    Color::Rgb(196,167,231), blue:     Color::Rgb(156,207,216),
                teal:     Color::Rgb(156,207,216), green:    Color::Rgb(156,207,216),
                red:      Color::Rgb(235,111,146), yellow:   Color::Rgb(246,193,119),
                pink:     Color::Rgb(235,111,146),
            },
            "rose-pine-moon" => Self {
                base:     Color::Rgb(35,33,54),    surface0: Color::Rgb(42,39,63),
                surface1: Color::Rgb(57,53,82),    overlay0: Color::Rgb(110,106,134),
                text:     Color::Rgb(224,222,244), subtext:  Color::Rgb(144,140,170),
                mauve:    Color::Rgb(196,167,231), blue:     Color::Rgb(156,207,216),
                teal:     Color::Rgb(156,207,216), green:    Color::Rgb(156,207,216),
                red:      Color::Rgb(235,111,146), yellow:   Color::Rgb(246,193,119),
                pink:     Color::Rgb(235,111,146),
            },
            "rose-pine-dawn" => Self {
                base:     Color::Rgb(250,244,237), surface0: Color::Rgb(242,233,222),
                surface1: Color::Rgb(233,220,204), overlay0: Color::Rgb(152,147,165),
                text:     Color::Rgb(87,82,121),   subtext:  Color::Rgb(121,117,147),
                mauve:    Color::Rgb(144,122,169), blue:     Color::Rgb(86,148,159),
                teal:     Color::Rgb(86,148,159),  green:    Color::Rgb(86,148,159),
                red:      Color::Rgb(180,99,122),  yellow:   Color::Rgb(234,157,52),
                pink:     Color::Rgb(180,99,122),
            },
            "onedark" => Self {
                base:     Color::Rgb(40,44,52),    surface0: Color::Rgb(49,53,63),
                surface1: Color::Rgb(57,62,73),    overlay0: Color::Rgb(92,99,112),
                text:     Color::Rgb(171,178,191), subtext:  Color::Rgb(152,159,172),
                mauve:    Color::Rgb(198,120,221), blue:     Color::Rgb(97,175,239),
                teal:     Color::Rgb(86,182,194),  green:    Color::Rgb(152,195,121),
                red:      Color::Rgb(224,108,117), yellow:   Color::Rgb(229,192,123),
                pink:     Color::Rgb(198,120,221),
            },
            "solarized-dark" => Self {
                base:     Color::Rgb(0,43,54),     surface0: Color::Rgb(7,54,66),
                surface1: Color::Rgb(88,110,117),  overlay0: Color::Rgb(101,123,131),
                text:     Color::Rgb(253,246,227), subtext:  Color::Rgb(238,232,213),
                mauve:    Color::Rgb(108,113,196), blue:     Color::Rgb(38,139,210),
                teal:     Color::Rgb(42,161,152),  green:    Color::Rgb(133,153,0),
                red:      Color::Rgb(220,50,47),   yellow:   Color::Rgb(181,137,0),
                pink:     Color::Rgb(211,54,130),
            },
            "solarized-light" => Self {
                base:     Color::Rgb(253,246,227), surface0: Color::Rgb(238,232,213),
                surface1: Color::Rgb(214,210,196), overlay0: Color::Rgb(147,161,161),
                text:     Color::Rgb(7,54,66),     subtext:  Color::Rgb(88,110,117),
                mauve:    Color::Rgb(108,113,196), blue:     Color::Rgb(38,139,210),
                teal:     Color::Rgb(42,161,152),  green:    Color::Rgb(133,153,0),
                red:      Color::Rgb(220,50,47),   yellow:   Color::Rgb(181,137,0),
                pink:     Color::Rgb(211,54,130),
            },
            "material-ocean" => Self {
                base:     Color::Rgb(15,17,26),    surface0: Color::Rgb(27,30,44),
                surface1: Color::Rgb(36,40,59),    overlay0: Color::Rgb(84,91,130),
                text:     Color::Rgb(192,202,245), subtext:  Color::Rgb(137,148,196),
                mauve:    Color::Rgb(199,146,234), blue:     Color::Rgb(130,170,255),
                teal:     Color::Rgb(137,221,255), green:    Color::Rgb(195,232,141),
                red:      Color::Rgb(255,85,114),  yellow:   Color::Rgb(255,203,107),
                pink:     Color::Rgb(199,146,234),
            },
            "everforest-dark" => Self {
                base:     Color::Rgb(35,38,33),    surface0: Color::Rgb(45,50,42),
                surface1: Color::Rgb(57,62,51),    overlay0: Color::Rgb(131,139,117),
                text:     Color::Rgb(211,198,170), subtext:  Color::Rgb(157,151,132),
                mauve:    Color::Rgb(214,153,182), blue:     Color::Rgb(127,187,179),
                teal:     Color::Rgb(131,192,170), green:    Color::Rgb(167,192,128),
                red:      Color::Rgb(230,126,128), yellow:   Color::Rgb(219,188,127),
                pink:     Color::Rgb(214,153,182),
            },
            "kanagawa" => Self {
                base:     Color::Rgb(22,22,29),    surface0: Color::Rgb(31,31,40),
                surface1: Color::Rgb(42,42,54),    overlay0: Color::Rgb(84,84,109),
                text:     Color::Rgb(220,215,186), subtext:  Color::Rgb(150,147,125),
                mauve:    Color::Rgb(152,110,175), blue:     Color::Rgb(125,167,216),
                teal:     Color::Rgb(106,153,153), green:    Color::Rgb(118,148,106),
                red:      Color::Rgb(196,95,106),  yellow:   Color::Rgb(195,171,97),
                pink:     Color::Rgb(210,126,153),
            },
            "ayu-dark" => Self {
                base:     Color::Rgb(13,17,23),    surface0: Color::Rgb(20,25,33),
                surface1: Color::Rgb(30,37,48),    overlay0: Color::Rgb(72,82,99),
                text:     Color::Rgb(203,214,226), subtext:  Color::Rgb(131,148,168),
                mauve:    Color::Rgb(213,121,255), blue:     Color::Rgb(83,154,252),
                teal:     Color::Rgb(149,230,203), green:    Color::Rgb(186,230,126),
                red:      Color::Rgb(255,51,51),   yellow:   Color::Rgb(255,180,84),
                pink:     Color::Rgb(255,130,200),
            },
            // default: catppuccin-macchiato
            _ => Self {
                base:     Color::Rgb(30,32,48),    surface0: Color::Rgb(54,58,79),
                surface1: Color::Rgb(73,77,100),   overlay0: Color::Rgb(110,115,141),
                text:     Color::Rgb(202,211,245), subtext:  Color::Rgb(165,173,203),
                mauve:    Color::Rgb(198,160,246), blue:     Color::Rgb(138,173,244),
                teal:     Color::Rgb(139,213,202), green:    Color::Rgb(166,218,149),
                red:      Color::Rgb(237,135,150), yellow:   Color::Rgb(238,212,159),
                pink:     Color::Rgb(245,189,230),
            },
        }
    }

    fn builtin_names() -> &'static [&'static str] {
        &[
            "catppuccin-macchiato", "catppuccin-latte", "catppuccin-frappe", "catppuccin-mocha",
            "tokyo-night", "tokyo-night-storm", "tokyo-night-light",
            "gruvbox-dark", "gruvbox-light",
            "nord", "dracula",
            "rose-pine", "rose-pine-moon", "rose-pine-dawn",
            "onedark",
            "solarized-dark", "solarized-light",
            "material-ocean", "everforest-dark",
            "kanagawa", "ayu-dark",
        ]
    }
}

// ─── Icon sets ────────────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
enum IconSet { NerdFont, Emoji, Minimal, None }

impl IconSet {
    fn by_name(name: &str) -> Self {
        match name {
            "emoji"   => Self::Emoji,
            "minimal" => Self::Minimal,
            "none"    => Self::None,
            _         => Self::NerdFont,
        }
    }
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
            key_copy: "c".into(), key_cut: "u".into(), key_paste: "p".into(),
            key_delete: "d".into(), key_rename: "r".into(),
            key_new_file: "f".into(), key_new_dir: "m".into(),
            key_search: "/".into(), key_toggle_hidden: ".".into(),
            key_quit: "q".into(), key_new_tab: "Tab".into(),
            key_close_tab: "Tab+r".into(), key_select: "Space".into(),
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

// ─── File types ───────────────────────────────────────────────────────────────
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
        FileKind::Dir     => t.blue,  FileKind::Image   => t.pink,
        FileKind::Video   => t.mauve, FileKind::Audio   => t.mauve,
        FileKind::Archive => t.red,   FileKind::Doc     => t.yellow,
        FileKind::Code    => t.green, FileKind::Exec    => t.teal,
        FileKind::Symlink => t.teal,  FileKind::Other   => t.text,
    }
}

fn named_dir_icon(name: &str, icon_set: &IconSet) -> Option<&'static str> {
    match icon_set {
        IconSet::Emoji => match name {
            ".config"|".local"|".cache" => Some("⚙️ "),
            ".ssh"                      => Some("🔒"),
            ".git"|".github"            => Some("🐙"),
            "downloads"                 => Some("⬇️ "),
            "documents"                 => Some("📄"),
            "desktop"                   => Some("🖥️ "),
            "pictures"|"photos"|"images"=> Some("🖼️ "),
            "videos"                    => Some("🎬"),
            "music"|"audio"             => Some("🎵"),
            "games"                     => Some("🎮"),
            "projects"|"dev"|"code"|"src"=>Some("💻"),
            _                           => None,
        },
        IconSet::Minimal => Some("▸"),
        IconSet::None    => Some(""),
        IconSet::NerdFont => match name {
            ".config"                           => Some("\u{e5fc}"),
            ".local"                            => Some("\u{f015}"),
            ".cache"                            => Some("\u{f0c7}"),
            ".ssh"                              => Some("\u{f023}"),
            ".git" | ".github"                  => Some("\u{e702}"),
            "downloads"                         => Some("\u{f019}"),
            "documents"                         => Some("\u{f02d}"),
            "desktop"                           => Some("\u{f108}"),
            "pictures" | "photos" | "images"    => Some("\u{f03e}"),
            "videos"                            => Some("\u{f03d}"),
            "music" | "audio"                   => Some("\u{f001}"),
            "games"                             => Some("\u{f11b}"),
            "projects" | "dev" | "code" | "src" => Some("\u{e60c}"),
            "home"                              => Some("\u{f015}"),
            "tmp" | "temp"                      => Some("\u{f0c9}"),
            "bin"                               => Some("\u{f489}"),
            "lib" | "lib64"                     => Some("\u{f121}"),
            "etc"                               => Some("\u{f013}"),
            "usr"                               => Some("\u{f007}"),
            "var"                               => Some("\u{f1c0}"),
            "opt"                               => Some("\u{f187}"),
            "boot"                              => Some("\u{f0a0}"),
            "root"                              => Some("\u{f023}"),
            "node_modules"                      => Some("\u{e718}"),
            "target"                            => Some("\u{f140}"),
            "public"                            => Some("\u{f0ac}"),
            "fonts"                             => Some("\u{f031}"),
            "themes"                            => Some("\u{f53f}"),
            "icons"                             => Some("\u{f03e}"),
            "wallpapers" | "walls"              => Some("\u{f03e}"),
            "scripts"                           => Some("\u{f489}"),
            "dotfiles"                          => Some("\u{e615}"),
            _                                   => None,
        },
    }
}

fn file_icon(path: &Path, kind: &FileKind, icon_set: &IconSet) -> &'static str {
    match icon_set {
        IconSet::None    => "",
        IconSet::Minimal => match kind {
            FileKind::Dir     => "▸",
            FileKind::Symlink => "↪",
            FileKind::Image   => "i",
            FileKind::Video   => "v",
            FileKind::Audio   => "a",
            FileKind::Archive => "z",
            FileKind::Doc     => "d",
            FileKind::Code    => "c",
            FileKind::Exec    => "x",
            FileKind::Other   => "f",
        },
        IconSet::Emoji => match kind {
            FileKind::Dir     => "📁",
            FileKind::Symlink => "🔗",
            FileKind::Image   => "🖼 ",
            FileKind::Video   => "🎬",
            FileKind::Audio   => "🎵",
            FileKind::Archive => "📦",
            FileKind::Doc     => "📄",
            FileKind::Code    => "📝",
            FileKind::Exec    => "⚙ ",
            FileKind::Other   => "📄",
        },
        IconSet::NerdFont => {
            if *kind == FileKind::Dir {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                if let Some(ic) = named_dir_icon(&name, icon_set) { return ic; }
                return "\u{f07b}";
            }
            if *kind == FileKind::Symlink { return "\u{f0c1}"; }
            let ext  = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            match name.as_str() {
                ".bashrc"|".bash_profile"|".bash_history" => return "\u{f489}",
                ".zshrc"|".zshenv"|".zprofile"            => return "\u{f489}",
                ".gitconfig"|".gitignore"|".gitmodules"   => return "\u{e702}",
                "makefile"|"gnumakefile"                  => return "\u{f423}",
                "dockerfile"                              => return "\u{f308}",
                "cargo.toml"|"cargo.lock"                 => return "\u{e7a8}",
                "package.json"|"package-lock.json"        => return "\u{e718}",
                "readme.md"|"readme.txt"|"readme"         => return "\u{f48a}",
                _ => {}
            }
            match ext.as_str() {
                "png"|"jpg"|"jpeg"|"gif"|"bmp"|"webp"|"ico"|"svg"|"tiff"|"avif" => "\u{f03e}",
                "mp4"|"mkv"|"avi"|"mov"|"webm"|"flv"|"wmv"  => "\u{f03d}",
                "mp3"|"flac"|"ogg"|"wav"|"aac"|"m4a"|"opus" => "\u{f001}",
                "zip"|"tar"|"gz"|"bz2"|"xz"|"zst"|"tgz"|"7z"|"rar"|"tbz2" => "\u{f410}",
                "pdf"        => "\u{f1c1}", "doc"|"docx" => "\u{f1c2}",
                "xls"|"xlsx" => "\u{f1c3}", "ppt"|"pptx" => "\u{f1c4}",
                "rs"  => "\u{e7a8}", "py"  => "\u{e606}", "js"|"mjs" => "\u{e74e}",
                "ts"  => "\u{e628}", "go"  => "\u{e626}", "c"        => "\u{e61e}",
                "cpp"|"cc"|"cxx" => "\u{e61d}", "h"|"hpp" => "\u{f0fd}",
                "java" => "\u{e738}", "rb"  => "\u{e739}", "php" => "\u{e73d}",
                "sh"|"bash"|"zsh"|"fish" => "\u{f489}",
                "lua" => "\u{e620}", "vim" => "\u{e62b}", "toml" => "\u{f669}",
                "yaml"|"yml" => "\u{f481}", "json" => "\u{e60b}", "xml" => "\u{f72d}",
                "html"|"htm" => "\u{f13b}", "css" => "\u{e749}", "scss"|"sass" => "\u{e603}",
                "md"|"markdown" => "\u{f48a}", "txt"|"rst" => "\u{f15c}",
                "conf"|"ini"|"cfg" => "\u{f013}", "env" => "\u{f462}",
                "lock" => "\u{f023}", "log" => "\u{f18d}",
                "ttf"|"otf"|"woff"|"woff2" => "\u{f031}",
                _ if *kind == FileKind::Exec => "\u{f489}",
                _ => "\u{f15b}",
            }
        }
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
            ],
            SettingsSection::Appearance => vec![
                ("col_parent","Parent pane width (%)"), ("col_files","Files pane width (%)"),
                ("theme","Theme"), ("icon_set","Icon set (nerdfont/emoji/minimal/none)"),
            ],
            SettingsSection::Openers    => vec![
                ("opener_image","Image"), ("opener_video","Video"), ("opener_audio","Audio"),
                ("opener_doc","Documents"), ("opener_editor","Editor"), ("opener_archive","Archives"),
            ],
            SettingsSection::Keybinds   => vec![
                ("key_copy","Copy"), ("key_cut","Cut"), ("key_paste","Paste"),
                ("key_delete","Delete"), ("key_rename","Rename"),
                ("key_new_file","New file"), ("key_new_dir","New directory"),
                ("key_search","Search"), ("key_toggle_hidden","Toggle hidden"),
                ("key_quit","Quit"), ("key_new_tab","New tab"),
                ("key_close_tab","Close tab"), ("key_select","Select"),
                ("key_select_all","Select all"),
            ],
        }
    }
    fn dropdown_options(key: &str, user_themes: &[UserThemeEntry]) -> Option<Vec<String>> {
        match key {
            "theme"       => Some(Theme::all_names_merged(user_themes)),
            "icon_set"    => Some(vec!["nerdfont","emoji","minimal","none"].iter().map(|s| s.to_string()).collect()),
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
}

// ─── App ─────────────────────────────────────────────────────────────────────
struct App {
    cfg:         Config,
    theme:       Theme,
    icon_set:    IconSet,
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
    // User-defined themes loaded from ~/.config/fd-files/themes.json
    user_themes:  Vec<UserThemeEntry>,
}
impl App {
    fn new(start: PathBuf, cfg: Config) -> Self {
        let sh = cfg.show_hidden;
        let user_themes = UserThemeEntry::load_all();
        let theme    = Theme::resolve(&cfg.theme, &user_themes);
        let icon_set = IconSet::by_name(&cfg.icon_set);
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
            img_path: None,
            img_state: None,
            vid_thumb_path: None,
            vid_thumb_file: None,
            vid_thumb_state: None,
            vid_thumb_rx: None,
            user_themes,
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
        else if ARCHIVE_EXT.contains(&ext) { self.extract_archive(&path.clone()); }
        else { self.nvim_path = Some(path); }
    }
    fn extract_archive(&mut self, path: &Path) {
        let dst = path.parent().unwrap_or(Path::new("."));
        let parts: Vec<&str> = self.cfg.opener_archive.split_whitespace().collect();
        let res = if parts.len() >= 2 {
            Command::new(parts[0]).args(&parts[1..]).arg(path).arg("--dir").arg(dst).spawn()
        } else if which("tar") {
            Command::new("tar").args(["xf", &path.to_string_lossy(), "-C", &dst.to_string_lossy()]).spawn()
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
    // ratatui 0.29: f.size()  |  ratatui 0.30+: f.area()
    #[allow(deprecated)]
    let sz = f.size();

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
        _ => {}
    }
}

fn draw_tab_bar(f: &mut Frame, app: &App, rect: Rect) {
    let titles: Vec<Line> = app.tabs.iter().enumerate().map(|(i, tab)| {
        let name = tab.cwd.file_name().and_then(|n| n.to_str()).unwrap_or("/");
        let label = format!(" {} {} ", i+1, name);
        if i == app.tab_idx {
            Line::from(Span::styled(label, bold_bg(app.theme.base, app.theme.mauve)))
        } else {
            Line::from(Span::styled(label, st_bg(app.theme.subtext, app.theme.surface0)))
        }
    }).collect();
    let tabs = Tabs::new(titles)
        .select(app.tab_idx)
        .style(st_bg(app.theme.subtext, app.theme.surface0))
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
        let ic   = file_icon(e, &k, &app.icon_set);
        let name = e.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let is_cur = *e == tab.cwd;
        let(fg, bg) = if is_cur { (app.theme.base, app.theme.blue) } else { (fg, app.theme.base) };
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
            let ic   = file_icon(e, &k, &app.icon_set);
            let name = e.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let size = file_size_str(e);
            let is_cur = tab.state.selected() == Some(i);
            let is_sel = tab.selected.contains(e);
            let (fg, bg) = if is_cur { (app.theme.base, app.theme.mauve) }
                           else if is_sel { (app.theme.mauve, app.theme.surface0) }
                           else { (fg, app.theme.base) };
            let max_name = (rect.width.saturating_sub(9)) as usize;
            let name_clipped: String = name.chars().take(max_name).collect();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", ic), st_bg(fg, bg)),
                Span::styled(name_clipped, st_bg(fg, bg)),
                Span::styled(format!(" {:>6}", size), st_bg(if is_cur { app.theme.base } else { app.theme.overlay0 }, bg)),
            ]))
        }).collect();

    let title = {
        let name = tab.cwd.file_name().and_then(|n| n.to_str()).unwrap_or("/");
        format!(" \u{f07b} {} ", name)
    };
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(st(app.theme.surface1))
        .title(Span::styled(title, bold(app.theme.blue)))
        .style(st_bg(app.theme.text, app.theme.base));

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
            Span::styled(format!(" {} ", file_icon(&current, &k, &app.icon_set)), st_bg(c, app.theme.base)),
            Span::styled(name, bold_bg(c, app.theme.base)),
        ]),
        Line::from(vec![
            Span::styled(format!("  {}   {}", file_size_str(&current), format_mtime(&current, &app.cfg.date_format)), st_bg(app.theme.overlay0, app.theme.base)),
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
                st_bg(app.theme.overlay0, app.theme.base),
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
                Span::styled(format!(" {} ", file_icon(e, &ek, &app.icon_set)), st(kind_color(&ek, &app.theme))),
                Span::styled(e.file_name().unwrap_or_default().to_string_lossy().to_string(), st(kind_color(&ek, &app.theme))),
            ]))
        }).collect();
        let block = Block::default().style(st_bg(app.theme.text, app.theme.base));
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
                    st(app.theme.overlay0),
                )),
                Line::from(Span::styled(format!("  Size:   {}", size_str), st(app.theme.subtext))),
                Line::from(Span::styled(format!("  Format: .{}", ext), st(app.theme.subtext))),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    format!("  Press Enter to open with {}", app.cfg.opener_video),
                    st(app.theme.overlay0),
                )),
            ];
            let p = Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.base));
            f.render_widget(p, content_rect);
        }
        return;
    }

    // Audio — show metadata only, no preview
    if AUDIO_EXT.contains(&ext.as_str()) {
        let size_str = human_size(current.metadata().map(|m| m.len()).unwrap_or(0));
        let lines = vec![
            Line::from(Span::styled("  Audio file", bold(app.theme.subtext))),
            Line::from(Span::styled(format!("  Size:   {}", size_str), st(app.theme.subtext))),
            Line::from(Span::styled(format!("  Format: .{}", ext), st(app.theme.subtext))),
            Line::from(Span::raw("")),
            Line::from(Span::styled(
                format!("  Press Enter to open with {}", app.cfg.opener_audio),
                st(app.theme.overlay0),
            )),
        ];
        let p = Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.base));
        f.render_widget(p, content_rect);
        return;
    }

    // Text preview — skip large files to avoid blocking the UI
    const PREVIEW_SIZE_LIMIT: u64 = 512 * 1024 * 1024; // 512 MB
    const PREVIEW_READ_BYTES: u64 = 32 * 1024;         // read at most 32 KB

    let file_size = current.metadata().map(|m| m.len()).unwrap_or(0);

    if file_size > PREVIEW_SIZE_LIMIT {
        let lines = vec![
            Line::from(Span::styled("  (large file — preview skipped)", st(app.theme.overlay0))),
            Line::from(Span::styled(format!("  Size: {}", human_size(file_size)), st(app.theme.subtext))),
            Line::from(Span::styled("  Press Enter to open in your editor.", st(app.theme.overlay0))),
        ];
        let p = Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.base));
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
            Line::from(Span::styled(clipped, st(app.theme.subtext)))
        }).collect();
        let p = Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.base));
        f.render_widget(p, content_rect);
    } else {
        let lines = vec![
            Line::from(Span::styled("  (binary file)", st(app.theme.overlay0))),
            Line::from(Span::styled(format!("  Size: {}", human_size(file_size)), st(app.theme.subtext))),
        ];
        let p = Paragraph::new(lines).style(st_bg(app.theme.text, app.theme.base));
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
        Span::styled(format!("  {}  ", cwd), bold_bg(app.theme.base, app.theme.blue)),
    ];
    if !app.status_msg.is_empty() {
        let (fg, bg) = if app.status_err { (app.theme.base, app.theme.red) } else { (app.theme.base, app.theme.teal) };
        spans.push(Span::styled(format!("  {}  ", app.status_msg), bold_bg(fg, bg)));
    }
    if !app.yank.is_empty() {
        spans.push(Span::styled(format!("  \u{f0c5} {}  ", app.yank.len()), bold_bg(app.theme.base, app.theme.yellow)));
    }
    if !tab.selected.is_empty() {
        spans.push(Span::styled(format!("  \u{f14a} {}  ", tab.selected.len()), bold_bg(app.theme.base, app.theme.mauve)));
    }
    // Right-align count
    let count_str = format!("  {}/{}", cur, total);
    let used: usize = spans.iter().map(|s| s.content.len()).sum();
    let pad = (rect.width as usize).saturating_sub(used + count_str.len());
    spans.push(Span::styled(" ".repeat(pad), st_bg(app.theme.subtext, app.theme.surface0)));
    spans.push(Span::styled(count_str, st_bg(app.theme.subtext, app.theme.surface0)));

    let bar = Paragraph::new(Line::from(spans)).style(st_bg(app.theme.subtext, app.theme.surface0));
    f.render_widget(bar, rect);
}

fn draw_help_bar(f: &mut Frame, app: &App, rect: Rect) {
    let items = [
        ("\u{2195}","nav"), ("BS","up"), ("\u{23ce}","open"),
        ("Spc","sel"), ("c","copy"), ("u","cut"), ("p","paste"),
        ("d","del"), ("r","ren"), ("f","file"), ("m","dir"),
        ("/","find"), ("t","tab+"), ("x","tab-"), ("Tab","tabs"),
    ];
    let mut spans = vec![];
    for (key, action) in &items {
        spans.push(Span::styled(format!(" {}", key), bold(app.theme.mauve)));
        spans.push(Span::styled(format!(":{} ", action), st(app.theme.overlay0)));
    }
    let bar = Paragraph::new(Line::from(spans)).style(st_bg(app.theme.overlay0, app.theme.base));
    f.render_widget(bar, rect);
}

fn draw_settings(f: &mut Frame, app: &App, rect: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.mauve))
        .title(Span::styled(
            "  \u{f013} Settings  [\u{2190}\u{2192} sections  \u{2191}\u{2193} navigate  Enter edit  S save  Esc close]  ",
            bold_bg(app.theme.subtext, app.theme.base),
        ))
        .style(st_bg(app.theme.text, app.theme.base));
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
        if *sec == app.settings.section { Line::from(Span::styled(s, bold_bg(app.theme.base, app.theme.mauve))) }
        else { Line::from(Span::styled(s, st_bg(app.theme.subtext, app.theme.surface0))) }
    }).collect();
    let tabs = Tabs::new(titles)
        .select(match app.settings.section {
            SettingsSection::Behaviour  => 0, SettingsSection::Appearance => 1,
            SettingsSection::Openers    => 2, SettingsSection::Keybinds   => 3,
        })
        .style(st_bg(app.theme.subtext, app.theme.surface0))
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
        let (lbg, vbg) = if is_cur { (app.theme.surface0, app.theme.surface1) } else { (app.theme.base, app.theme.base) };
        let (lfg, vfg) = if is_cur { (app.theme.text, app.theme.mauve) } else { (app.theme.subtext, app.theme.text) };
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
            .style(st_bg(app.theme.base, app.theme.yellow));
        f.render_widget(hint, inner[2]);
    }

    // ── Dropdown overlay ───────────────────────────────────────────────────────
    if app.settings.dropdown {
        let items  = SettingsState::section_items(&app.settings.section);
        let (k, _) = items[app.settings.cursor];
        let opts   = SettingsState::dropdown_options(k, &app.user_themes).unwrap_or_default();

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
                (app.theme.base, app.theme.mauve)
            } else {
                (app.theme.text, app.theme.surface1)
            };
            ListItem::new(Line::from(vec![
                Span::styled(if is_cur { " \u{f0da} " } else { "   " }, st_bg(fg, bg)),
                Span::styled(opt.clone(), st_bg(fg, bg)),
            ]))
        }).collect();

        let dd_block = Block::default()
            .borders(Borders::ALL)
            .border_style(bold(app.theme.mauve))
            .title(Span::styled(" \u{2191}\u{2193} select  Enter confirm  Esc cancel ", st(app.theme.overlay0)))
            .style(st_bg(app.theme.text, app.theme.surface1));

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
        .border_style(bold(app.theme.mauve))
        .style(st_bg(app.theme.text, app.theme.base));
    let text = format!(" {}: {}\u{2588}", prompt, app.input_buf);
    let p = Paragraph::new(text).block(block).style(bold(app.theme.text));
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
    let p = Paragraph::new(msg).style(bold_bg(app.theme.base, app.theme.red));
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
        .border_style(bold(app.theme.mauve))
        .title(Span::styled(
            "  \u{f422} Fuzzy Find  [Esc cancel  \u{2191}\u{2193} navigate  Enter jump]  ",
            bold(app.theme.subtext),
        ))
        .style(st_bg(app.theme.text, app.theme.base));

    // Live count — always shows current number even while streaming
    let count_str = if app.fuzzy_loading {
        format!("  {} matches, indexing\u{2026}", app.fuzzy_results.len())
    } else {
        format!("  {} matches", app.fuzzy_results.len())
    };
    let search_text = Line::from(vec![
        Span::styled("  ", st(app.theme.overlay0)),
        Span::styled(&app.fuzzy_query, bold(app.theme.text)),
        Span::styled("\u{2588}", bold(app.theme.mauve)),
        Span::styled(&count_str, st(app.theme.overlay0)),
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
            let ic   = file_icon(path, &k, &app.icon_set);
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let rel  = path.strip_prefix(&app.tab().cwd)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            let is_cur = i == cursor;  // i is the absolute index, cursor is absolute — correct
            let ns = if is_cur { bold_bg(app.theme.base, app.theme.mauve) } else { st(c) };
            let ps = if is_cur { st_bg(app.theme.overlay0, app.theme.mauve) } else { st(app.theme.overlay0) };
            let prefix = if is_cur { " \u{f0da} " } else { "   " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, if is_cur { bold(app.theme.mauve) } else { st(app.theme.surface1) }),
                Span::styled(format!("{} ", ic), ns),
                Span::styled(name, ns),
                Span::styled(format!("  {}", rel), ps),
            ]))
        }).collect();

    let list_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(st(app.theme.surface1))
        .style(st_bg(app.theme.text, app.theme.base));
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
    }
    match key {
        KeyCode::Up        => app.tab_mut().move_cursor(-1),
        KeyCode::Down      => app.tab_mut().move_cursor(1),
        KeyCode::Left  | KeyCode::Backspace => app.tab_mut().leave(),
        KeyCode::Right | KeyCode::Enter     => app.open_current(),
        KeyCode::PageUp    => app.tab_mut().move_cursor(-10),
        KeyCode::PageDown  => app.tab_mut().move_cursor(10),
        KeyCode::Home      => { app.tab_mut().state.select(Some(0)); }
        KeyCode::End       => { let n=app.tab().visible().len(); if n>0 { app.tab_mut().state.select(Some(n-1)); } }
        KeyCode::Char(' ') => app.tab_mut().toggle_select(),
        KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) => app.tab_mut().select_all(),
        KeyCode::Char('A') => app.tab_mut().select_all(),
        KeyCode::Char('r') if mods.contains(KeyModifiers::CONTROL) => app.tab_mut().deselect_all(),
        KeyCode::Char('c') => {
            if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
            else { app.yank_files(false); }
        }
        KeyCode::Char('u') => {
            if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
            else { app.yank_files(true); }
        }
        KeyCode::Char('p') => app.paste_files(),
        KeyCode::Char('d') => {
            if app.tab().selected.is_empty() { app.msg("Select files first (Space)", true); }
            else { app.mode = InputMode::Confirm; }
        }
        KeyCode::Char('r') => {
            if let Some(p) = app.tab().current().cloned() {
                let name = p.file_name().and_then(|n|n.to_str()).unwrap_or("").to_string();
                app.input_buf = name.clone(); app.mode = InputMode::Rename(name);
            }
        }
        KeyCode::Char('f') => { app.input_buf.clear(); app.mode = InputMode::NewFile; }
        KeyCode::Char('m') => { app.input_buf.clear(); app.mode = InputMode::NewDir; }
        KeyCode::Char('/') => app.open_fuzzy(),
        KeyCode::Char('.') => {
            let h = !app.tab().show_hidden;
            app.tab_mut().show_hidden = h; app.tab_mut().refresh();
            app.msg(if h {"Hidden files shown"} else {"Hidden files hidden"}, false);
        }
        KeyCode::Char('t') => app.new_tab(),
        KeyCode::Char('x') => app.close_tab(),
        KeyCode::Tab => {
            app.tab_idx = (app.tab_idx + 1) % app.tabs.len();
        }
        KeyCode::Char(':') => { app.mode = InputMode::Settings; }
        KeyCode::Esc | KeyCode::Char('q') => return true,
        _ => {}
    }
    false
}

fn handle_settings_key(app: &mut App, key: KeyCode, _mods: KeyModifiers) -> bool {
    // ── Dropdown mode ──────────────────────────────────────────────────────────
    if app.settings.dropdown {
        let items  = SettingsState::section_items(&app.settings.section);
        let (k, _) = items[app.settings.cursor];
        let opts   = SettingsState::dropdown_options(k, &app.user_themes).unwrap_or_default();
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
                app.theme    = Theme::resolve(&app.cfg.theme, &app.user_themes);
                app.icon_set = IconSet::by_name(&app.cfg.icon_set);
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
            if SettingsState::dropdown_options(k, &app.user_themes).is_some() {
                // Open dropdown — pre-select current value
                let cur_val = SettingsState::get_value(k, &app.cfg);
                let opts    = SettingsState::dropdown_options(k, &app.user_themes).unwrap();
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
                    app.user_themes = UserThemeEntry::load_all();
                    app.theme    = Theme::resolve(&app.cfg.theme, &app.user_themes);
                    app.icon_set = IconSet::by_name(&app.cfg.icon_set);
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
    // ── Auto-create data directories on first run ──────────────────────────
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
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    Ok(())
}
