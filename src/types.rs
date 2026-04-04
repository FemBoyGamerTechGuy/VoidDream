// types.rs — FileKind, InputMode, Tab, ExtractionProgress, file-type lists, helpers
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};
use ratatui::widgets::ListState;

// ─── File type extension lists ────────────────────────────────────────────────

pub const IMAGE_EXT: &[&str] = &[
    "png","jpg","jpeg","gif","bmp","webp","ico","tiff","avif",
    "hdr","exr","pnm","pbm","pgm","ppm","pam","ff","qoi",
    "raw","arw","cr2","cr3","nef","nrw","orf","raf","rw2","dng","pef","srw","x3f",
    "heic","heif","jxl","svg","xcf","tga","dds","psd",
];
pub const VIDEO_EXT: &[&str] = &[
    "mp4","mkv","avi","mov","webm","flv","wmv","m4v","mpg","mpeg",
    "ts","mts","m2ts","vob","ogv","3gp","3g2","rm","rmvb","divx",
    "xvid","asf","f4v","hevc","h264","h265","264","265","mxf","dv","qt","amv",
];
pub const AUDIO_EXT: &[&str] = &[
    "mp3","flac","ogg","wav","aac","m4a","opus","wma","ape","mka",
    "aiff","aif","alac","dsd","dsf","dff","mid","midi","amr","tta",
    "wv","caf","ra","au","snd","spx","mpc","ac3","dts","eac3","thd","truehd",
];
pub const ARCHIVE_EXT: &[&str] = &["zip","tar","gz","bz2","xz","7z","rar","zst","tgz","tbz2"];
pub const HTML_EXT:    &[&str] = &["html","htm","xhtml","mhtml","mht"];
pub const JAR_EXT:     &[&str] = &["jar","war","ear"];
pub const DOC_EXT:     &[&str] = &["pdf","doc","docx","odt","xls","xlsx","ods","ppt","pptx","odp"];
pub const CODE_EXT:    &[&str] = &[
    "py","js","ts","rs","go","c","cpp","h","java","rb","php",
    "sh","bash","zsh","fish","lua","vim","toml","yaml","yml",
    "json","xml","html","css","scss","md","rst","txt","conf","ini","cfg","env","lock",
];

// ─── FileKind ─────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum FileKind { Dir, Image, Video, Audio, Archive, Jar, Doc, Code, Html, Exec, Symlink, Other }

pub fn file_kind(path: &Path) -> FileKind {
    if path.is_symlink() { return FileKind::Symlink; }
    if path.is_dir()     { return FileKind::Dir; }
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
    let ext = ext.as_str();
    if IMAGE_EXT.contains(&ext)   { return FileKind::Image; }
    if VIDEO_EXT.contains(&ext)   { return FileKind::Video; }
    if AUDIO_EXT.contains(&ext)   { return FileKind::Audio; }
    if ARCHIVE_EXT.contains(&ext) { return FileKind::Archive; }
    if HTML_EXT.contains(&ext)    { return FileKind::Html; }
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

// ─── Size / time helpers ──────────────────────────────────────────────────────

/// Compact human-readable byte count: "4.2M", "128K", "512B".
pub fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B","K","M","G","T"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 { v /= 1024.0; u += 1; }
    if u == 0 { format!("{:.0}B", v) } else { format!("{:.1}{}", v, UNITS[u]) }
}

/// File size string for the file list (empty for directories).
pub fn file_size_str(path: &Path) -> String {
    if path.is_dir() { return String::new(); }
    path.metadata().map(|m| human_size(m.len())).unwrap_or("?".into())
}

/// Returns `(time_str, date_str)` from a path's mtime, e.g. `("20:54", "07/03/2026")`.
pub fn format_mtime_split(path: &Path) -> (String, String) {
    use std::time::SystemTime;
    if let Ok(meta) = path.metadata() {
        if let Ok(modified) = meta.modified() {
            let secs = modified.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
            let (y, mo, d, h, mi) = secs_to_datetime(secs);
            return (format!("{:02}:{:02}", h, mi), format!("{:02}/{:02}/{:04}", d, mo, y));
        }
    }
    ("?".into(), "?".into())
}

pub fn secs_to_datetime(secs: u64) -> (u64, u64, u64, u64, u64) {
    let mi = (secs % 3600) / 60;
    let h  = (secs % 86400) / 3600;
    let mut rem = secs / 86400;
    let mut y = 1970u64;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if rem < dy { break; }
        rem -= dy; y += 1;
    }
    let months = if is_leap(y) {
        [31u64,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31u64,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut mo = 1u64;
    for &dm in &months { if rem < dm { break; } rem -= dm; mo += 1; }
    (y, mo, rem + 1, h, mi)
}

pub fn is_leap(y: u64) -> bool { y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) }

/// Local timezone offset in seconds.
/// Cached after first call so we never spawn a subprocess on every tick.
pub fn local_tz_offset_secs() -> i64 {
    use std::sync::OnceLock;
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        use std::process::Command;
        Command::new("date").arg("+%z").output().ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                let s = s.trim();
                if s.len() < 5 { return None; }
                let sign: i64 = if s.starts_with('-') { -1 } else { 1 };
                let d = s.trim_start_matches(['+','-']);
                let hh = d[..2].parse::<i64>().ok()?;
                let mm = d[2..4].parse::<i64>().ok()?;
                Some(sign * (hh * 3600 + mm * 60))
            })
            .unwrap_or(0)
    })
}

pub fn local_secs() -> u64 {
    use std::time::SystemTime;
    let utc = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
    let off = local_tz_offset_secs();
    if off >= 0 { utc.saturating_add(off as u64) } else { utc.saturating_sub((-off) as u64) }
}

pub fn current_time_str() -> String {
    let secs = local_secs();
    let (_, _, _, h, mi) = secs_to_datetime(secs);
    format!("{:02}:{:02}:{:02}", h, mi, secs % 60)
}

pub fn current_date_str() -> String {
    let secs = local_secs();
    let (y, mo, d, _, _) = secs_to_datetime(secs);
    format!("{:02}/{:02}/{:04}", d, mo, y)
}

// ─── Directory helpers ────────────────────────────────────────────────────────

pub fn list_dir(path: &Path, show_hidden: bool) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = match fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return vec![],
    };
    if !show_hidden {
        entries.retain(|p| {
            !p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(false)
        });
    }
    entries.sort_by(|a, b| {
        let ad = a.is_dir(); let bd = b.is_dir();
        if ad != bd { return bd.cmp(&ad); }
        a.file_name().cmp(&b.file_name())
    });
    entries
}

pub fn dirs_home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"))
}

pub fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty    = entry.file_type()?;
        if ty.is_dir() { copy_dir(&entry.path(), &dst.join(entry.file_name()))?; }
        else           { fs::copy(entry.path(), dst.join(entry.file_name()))?; }
    }
    Ok(())
}

// ─── Tab ──────────────────────────────────────────────────────────────────────

pub struct Tab {
    pub cwd:             PathBuf,
    pub entries:         Vec<PathBuf>,
    pub state:           ListState,
    pub scroll:          usize,
    pub selected:        HashSet<PathBuf>,
    pub show_hidden:     bool,
    pub search_query:    String,
    pub search_results:  Option<Vec<PathBuf>>,
    pub parent_entries:  Vec<PathBuf>,   // async cache for parent pane
    pub parent_path:     Option<PathBuf>,
    pub preview_entries: Vec<PathBuf>,   // async cache for preview pane
    pub preview_path:    Option<PathBuf>,
}

impl Tab {
    pub fn new(cwd: PathBuf, show_hidden: bool) -> Self {
        let mut t = Self {
            cwd, entries: vec![], state: ListState::default(), scroll: 0,
            selected: HashSet::new(), show_hidden,
            search_query: String::new(), search_results: None,
            parent_entries: Vec::new(), parent_path: None,
            preview_entries: Vec::new(), preview_path: None,
        };
        t.refresh();
        if !t.entries.is_empty() { t.state.select(Some(0)); }
        t
    }

    pub fn refresh(&mut self) {
        self.entries = list_dir(&self.cwd, self.show_hidden);
        self.parent_path     = None; self.parent_entries.clear();
        self.preview_path    = None; self.preview_entries.clear();
        let cur = self.state.selected().unwrap_or(0);
        if self.entries.is_empty() { self.state.select(None); }
        else { self.state.select(Some(cur.min(self.entries.len() - 1))); }
        self.selected.retain(|p| self.entries.contains(p));
    }

    pub fn visible(&self) -> &[PathBuf] {
        if let Some(ref r) = self.search_results { r.as_slice() } else { &self.entries }
    }
    pub fn current(&self) -> Option<&PathBuf> {
        self.state.selected().and_then(|i| self.visible().get(i))
    }
    pub fn move_cursor(&mut self, delta: i32) {
        let len = self.visible().len(); if len == 0 { return; }
        let cur  = self.state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).clamp(0, len as i32 - 1) as usize;
        self.state.select(Some(next));
    }
    pub fn enter(&mut self) {
        if let Some(p) = self.current().cloned() {
            if p.is_dir() {
                self.cwd = p; self.state.select(Some(0)); self.scroll = 0;
                self.selected.clear(); self.search_query.clear();
                self.search_results = None; self.refresh();
            }
        }
    }
    pub fn leave(&mut self) {
        if let Some(parent) = self.cwd.parent().map(|p| p.to_path_buf()) {
            if parent == self.cwd { return; }
            let old  = self.cwd.clone();
            self.cwd = parent; self.state.select(Some(0)); self.scroll = 0;
            self.selected.clear(); self.search_query.clear();
            self.search_results = None; self.refresh();
            if let Some(i) = self.entries.iter().position(|e| *e == old) {
                self.state.select(Some(i));
            }
        }
    }
    pub fn toggle_select(&mut self) {
        if let Some(p) = self.current().cloned() {
            if self.selected.contains(&p) { self.selected.remove(&p); }
            else { self.selected.insert(p); }
            self.move_cursor(1);
        }
    }
    pub fn select_all(&mut self)   { self.selected = self.entries.iter().cloned().collect(); }
    pub fn deselect_all(&mut self) { self.selected.clear(); }
}

// ─── ExtractionProgress ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ExtractionProgress {
    pub filename:   String,
    pub current:    u64,
    pub total:      u64,
    pub done:       bool,
    pub error:      Option<String>,
    pub start_time: Instant,
    pub pid:        Option<u32>,
}

// ─── InputMode ────────────────────────────────────────────────────────────────

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    FuzzySearch,
    Rename(String),
    NewFile,
    NewDir,
    Confirm,
    Settings,
    Help,
    Extracting,
    RunArgs(PathBuf, bool, String, String), // (path, focus_on_end, start_args, end_args)
    OpenWith(PathBuf, Vec<(String, String)>, usize), // (path, [(label, cmd)], cursor)
    OpenWithCustom(PathBuf),
    DriveManager,
}
