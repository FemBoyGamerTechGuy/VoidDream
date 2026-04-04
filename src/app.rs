// app.rs — App struct and impl
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use crate::{config::*, types::*, ui::folder_size};

pub struct App {
    pub cfg:         Config,
    pub theme:       Theme,
    pub icons:       IconData,
    pub tabs:        Vec<Tab>,
    pub tab_idx:     usize,
    pub yank:        Vec<PathBuf>,
    pub yank_cut:    bool,
    pub mode:        InputMode,
    pub input_buf:   String,
    pub status_msg:  String,
    pub status_err:  bool,
    pub msg_time:    Option<Instant>,
    pub nvim_path:   Option<PathBuf>,
    pub settings:    SettingsState,
    pub fuzzy_query:   String,
    pub fuzzy_index:   Vec<PathBuf>,
    pub fuzzy_results: Vec<PathBuf>,
    pub fuzzy_cursor:  usize,
    pub fuzzy_loading: bool,
    pub fuzzy_rx:      Option<mpsc::Receiver<PathBuf>>,
    pub last_preview_size: (u16, u16),
    // Image preview — loaded in background thread
    pub img_picker:   Option<Picker>,
    pub img_path:     Option<PathBuf>,       // path currently displayed
    pub img_pending:  Option<PathBuf>,       // path queued to load
    pub img_state:    Option<StatefulProtocol>,
    pub img_rx:       Option<mpsc::Receiver<image::DynamicImage>>,
    pub img_debounce: Option<Instant>,       // delay before spawning load
    // Video thumbnail preview
    pub vid_thumb_path:  Option<PathBuf>,   // source video path
    pub vid_thumb_file:  Option<PathBuf>,   // temp PNG on disk
    pub vid_thumb_state: Option<StatefulProtocol>,
    pub vid_thumb_rx:    Option<mpsc::Receiver<PathBuf>>, // signals thumb is ready
    pub vid_debounce:    Option<Instant>,   // delay before spawning ffmpeg
    // Folder size — async background du
    pub folder_size_path:     Option<PathBuf>,
    pub folder_size_val:      Option<u64>,
    pub folder_size_rx:       Option<mpsc::Receiver<u64>>,
    pub folder_size_debounce: Option<Instant>,
    // Live clock string, updated every tick
    pub clock_str:    String,
    // Fuse-mount detection cache — rebuilt at most once per second.
    // Lets us skip du / blocking list_dir on jmtpfs, sshfs, etc.
    pub fuse_mounts:      Vec<PathBuf>,
    pub fuse_mounts_time: Option<Instant>,
    // Async parent-pane directory load
    pub parent_load_rx:       Option<mpsc::Receiver<Vec<PathBuf>>>,
    pub parent_load_path:     Option<PathBuf>,
    pub parent_load_debounce: Option<Instant>,
    // Async preview-pane directory load
    pub preview_load_rx:       Option<mpsc::Receiver<Vec<PathBuf>>>,
    pub preview_load_path:     Option<PathBuf>,
    pub preview_load_debounce: Option<Instant>,
    // Drive manager overlay state
    pub lang:           &'static crate::lang::Lang,
    pub drive_devices:  Vec<crate::drives::DriveDevice>,
    pub drive_cursor:   usize,
    pub help_scroll:    usize,
    // Archive extraction progress
    pub extract_progress: Option<ExtractionProgress>,
    pub extract_rx:       Option<mpsc::Receiver<ExtractionProgress>>,
    pub extract_child_pid: Option<u32>,  // PID of extraction process for kill on cancel
}
impl App {
    pub fn new(start: PathBuf, cfg: Config) -> Self {
        let sh = cfg.show_hidden;
        let icons = IconData::load(&cfg.icon_set);
        let theme = Theme::load(&cfg.theme);
        let lang  = crate::lang::load(&cfg.language);
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
            img_pending: None,
            img_state: None,
            img_rx: None,
            img_debounce: None,
            vid_thumb_path: None,
            vid_thumb_file: None,
            vid_thumb_state: None,
            vid_thumb_rx: None,
            vid_debounce: None,
            folder_size_path: None, folder_size_val: None,
            folder_size_rx: None, folder_size_debounce: None,
            clock_str: current_time_str(),
            fuse_mounts: Vec::new(), fuse_mounts_time: None,
            parent_load_rx: None, parent_load_path: None, parent_load_debounce: None,
            preview_load_rx: None, preview_load_path: None, preview_load_debounce: None,
            lang,
            drive_devices: Vec::new(), drive_cursor: 0,
            help_scroll: 0,
            extract_progress: None,
            extract_rx: None,
            extract_child_pid: None,
        }
    }
    pub fn tab(&self)         -> &Tab     { &self.tabs[self.tab_idx] }
    pub fn tab_mut(&mut self) -> &mut Tab { &mut self.tabs[self.tab_idx] }
    pub fn msg(&mut self, text: &str, err: bool) {
        self.status_msg = text.to_string();
        self.status_err = err;
        self.msg_time   = Some(Instant::now());
    }
    pub fn tick(&mut self) {
        self.clock_str = current_time_str();
        if let Some(t) = self.msg_time {
            if t.elapsed() > Duration::from_secs(4) {
                self.status_msg.clear(); self.msg_time = None;
            }
        }
        // Drain async folder size result
        if self.folder_size_rx.is_some() {
            let done = if let Some(rx) = &self.folder_size_rx {
                match rx.try_recv() {
                    Ok(size) => { self.folder_size_val = Some(size); true }
                    Err(mpsc::TryRecvError::Disconnected) => true,
                    Err(mpsc::TryRecvError::Empty) => false,
                }
            } else { false };
            if done { self.folder_size_rx = None; }
        }
        // Spawn du once debounce expires
        self.maybe_spawn_folder_size();
        // Drain async parent-pane dir load
        if self.parent_load_rx.is_some() {
            let result = if let Some(rx) = &self.parent_load_rx {
                match rx.try_recv() {
                    Ok(entries) => Some(entries),
                    Err(mpsc::TryRecvError::Disconnected) => Some(vec![]),
                    Err(mpsc::TryRecvError::Empty) => None,
                }
            } else { None };
            if let Some(entries) = result {
                self.parent_load_rx = None;
                let tab = &mut self.tabs[self.tab_idx];
                tab.parent_entries = entries;
                    }
        }
        self.maybe_spawn_parent_load();
        // Drain async preview-pane dir load
        if self.preview_load_rx.is_some() {
            let result = if let Some(rx) = &self.preview_load_rx {
                match rx.try_recv() {
                    Ok(entries) => Some(entries),
                    Err(mpsc::TryRecvError::Disconnected) => Some(vec![]),
                    Err(mpsc::TryRecvError::Empty) => None,
                }
            } else { None };
            if let Some(entries) = result {
                self.preview_load_rx = None;
                let tab = &mut self.tabs[self.tab_idx];
                tab.preview_entries = entries;
                    }
        }
        self.maybe_spawn_preview_load();
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
                        self.msg(&format!("{}: {}", self.lang.msg_extract_error, e), true);
                    } else {
                        self.msg(&format!("{} {}", self.lang.msg_extracted, p.filename), false);
                        self.tab_mut().refresh();
                    }
                    self.extract_rx           = None;
                    self.extract_progress     = None;
                    self.extract_child_pid    = None;
                    self.mode                 = InputMode::Normal;
                    // Full folder size cache reset so preview recalculates immediately
                    self.folder_size_path     = None;
                    self.folder_size_val      = None;
                    self.folder_size_rx       = None;
                    self.folder_size_debounce = None;
                } else {
                    let st = self.extract_progress.as_ref()
                        .map(|e| e.start_time)
                        .unwrap_or_else(Instant::now);
                    if let Some(pid) = p.pid { self.extract_child_pid = Some(pid); }
                    self.extract_progress = Some(ExtractionProgress { start_time: st, pid: None, ..p });
                }
            }
        }

        // Clear image state if we've navigated away from an image
        let is_image = self.tab().current()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| IMAGE_EXT.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);
        if !is_image && (self.img_state.is_some() || self.img_rx.is_some()) {
            self.img_path     = None;
            self.img_pending  = None;
            self.img_state    = None;
            self.img_rx       = None;
            self.img_debounce = None;
        }

        // Clear video thumbnail state if we've navigated away from a video
        let is_video = self.tab().current()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| VIDEO_EXT.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);
        if !is_video {
            if self.vid_thumb_state.is_some() || self.vid_thumb_rx.is_some() || self.vid_debounce.is_some() {
                if let Some(f) = self.vid_thumb_file.take() { let _ = fs::remove_file(f); }
                self.vid_thumb_path  = None;
                self.vid_thumb_state = None;
                self.vid_thumb_rx    = None;
                self.vid_debounce    = None;
            }
        }

        // Drain image load channel
        if self.img_rx.is_some() {
            let done = if let Some(rx) = &self.img_rx {
                match rx.try_recv() {
                    Ok(img)                               => Some(img),
                    Err(mpsc::TryRecvError::Disconnected) => { self.img_rx = None; None }
                    Err(mpsc::TryRecvError::Empty)        => None,
                }
            } else { None };
            if let Some(img) = done {
                self.img_rx = None;
                if let Some(picker) = self.img_picker.as_mut() {
                    self.img_state = Some(picker.new_resize_protocol(img));
                }
            }
        }

        // Drain video thumbnail channel — load image once ffmpeg signals done
        if self.vid_thumb_rx.is_some() {
            let done = if let Some(rx) = &self.vid_thumb_rx {
                match rx.try_recv() {
                    Ok(thumb_path) => Some(thumb_path),
                    Err(mpsc::TryRecvError::Disconnected) => {
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
                        // Delete temp file immediately after loading into memory
                        let _ = fs::remove_file(&thumb_path);
                        self.vid_thumb_file = None;
                        self.vid_thumb_state = Some(picker.new_resize_protocol(img));
                    }
                }
            }
        }
    }
    pub fn yank_files(&mut self, cut: bool) {
        let targets: Vec<PathBuf> = if !self.tab().selected.is_empty() {
            self.tab().selected.iter().cloned().collect()
        } else if let Some(p) = self.tab().current().cloned() { vec![p] }
        else { self.msg(self.lang.msg_nothing_to_yank, true); return; };
        let n = targets.len();
        self.yank = targets; self.yank_cut = cut;
        self.tab_mut().selected.clear();
        self.msg(&format!("{} item(s) {}", n, if cut { self.lang.msg_cut } else { self.lang.msg_copied }), false);
    }
    pub fn paste_files(&mut self) {
        if self.yank.is_empty() { self.msg(self.lang.msg_nothing_to_paste, true); return; }
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
        if errors.is_empty() { self.msg(self.lang.msg_pasted, false); }
        else { self.msg(&format!("{}: {}", self.lang.msg_error, errors[0]), true); }
    }
    pub fn delete_files(&mut self) {
        let targets: Vec<PathBuf> = if !self.tab().selected.is_empty() {
            self.tab().selected.iter().cloned().collect()
        } else if let Some(p) = self.tab().current().cloned() { vec![p] }
        else { return; };
        let mut errors = vec![];
        for p in &targets {
            let res: std::io::Result<()> = if p.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) };
            if let Err(e) = res { errors.push(e.to_string()); }
        }
        self.tab_mut().selected.clear(); self.tab_mut().refresh();
        if errors.is_empty() { self.msg(&format!("{} {} {}", self.lang.msg_deleted, targets.len(), self.lang.msg_deleted_items), false); }
        else { self.msg(&format!("{}: {}", self.lang.msg_error, errors[0]), true); }
    }
    /// Spawn an external program silently (no stdin/stdout/stderr).
    fn spawn_silent(cmd: &str, arg: &std::path::Path) {
        let _ = Command::new(cmd)
            .arg(arg)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    pub fn open_current(&mut self) {
        let path = match self.tab().current().cloned() { Some(p) => p, None => return };
        if path.is_dir() { self.tab_mut().enter(); return; }
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
        let ext = ext.as_str(); let cfg = self.cfg.clone();
        if IMAGE_EXT.contains(&ext)        { Self::spawn_silent(&cfg.opener_image,   &path); }
        else if VIDEO_EXT.contains(&ext)   { Self::spawn_silent(&cfg.opener_video,   &path); }
        else if AUDIO_EXT.contains(&ext)   { Self::spawn_silent(&cfg.opener_audio,   &path); }
        else if DOC_EXT.contains(&ext)     { Self::spawn_silent(&cfg.opener_doc,     &path); }
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
            self.msg(&format!("{} {} {}", self.lang.msg_launching, path.file_name().and_then(|n| n.to_str()).unwrap_or(""), self.lang.msg_in_terminal), false);
        }
        else if HTML_EXT.contains(&ext)    { Self::spawn_silent(&cfg.opener_browser, &path); }
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
                self.mode = InputMode::RunArgs(path, false, String::new(), String::new());
            } else {
                self.nvim_path = Some(path);
            }
        }
    }
    pub fn extract_archive(&mut self, path: &Path) {
        let (tx, rx) = mpsc::channel::<ExtractionProgress>();
        let (initial, _) = crate::extract::start_extraction(path, tx);
        self.extract_rx       = Some(rx);
        self.extract_progress = Some(initial);
        self.mode             = InputMode::Extracting;
    }


    /// Call after changing tab_idx so the async loaders re-request for the new tab.
    pub fn reset_dir_load_state(&mut self) {
        self.parent_load_path     = None;
        self.parent_load_rx       = None;
        self.parent_load_debounce = None;
        self.preview_load_path    = None;
        self.preview_load_rx      = None;
        self.preview_load_debounce = None;
    }

    pub fn new_tab(&mut self) {
        let cwd = self.tab().cwd.clone(); let sh = self.cfg.show_hidden;
        self.tabs.push(Tab::new(cwd, sh));
        self.tab_idx = self.tabs.len() - 1;
        self.reset_dir_load_state();
        self.msg(&format!("{} {} {}", self.lang.msg_tab_word, self.tab_idx+1, self.lang.msg_opened_word), false);
    }
    pub fn close_tab(&mut self) {
        if self.tabs.len() == 1 { self.msg(self.lang.msg_cant_close_tab, true); return; }
        self.tabs.remove(self.tab_idx);
        self.tab_idx = self.tab_idx.min(self.tabs.len()-1);
        self.reset_dir_load_state();
    }
    // ── Fuse-mount detection ─────────────────────────────────────────────────

    /// Rebuild the fuse-mount list from /proc/mounts at most once per second.
    pub fn refresh_fuse_mounts(&mut self) {
        let stale = self.fuse_mounts_time
            .map(|t| t.elapsed().as_secs() >= 1)
            .unwrap_or(true);
        if !stale { return; }
        self.fuse_mounts_time = Some(Instant::now());
        self.fuse_mounts.clear();
        if let Ok(text) = std::fs::read_to_string("/proc/mounts") {
            for line in text.lines() {
                // Fields: device  mountpoint  fstype  options  ...
                let mut parts = line.split_whitespace();
                let _dev   = parts.next();
                let mnt    = parts.next().unwrap_or("");
                let fstype = parts.next().unwrap_or("");
                // fuse.jmtpfs, fuse.sshfs, fuse.gvfsd-fuse, etc.
                if fstype.starts_with("fuse") {
                    self.fuse_mounts.push(PathBuf::from(mnt));
                }
            }
            // Sort so longest prefix wins on is_fuse_path
            self.fuse_mounts.sort_by_key(|p| std::cmp::Reverse(p.as_os_str().len()));
        }
    }

    /// Returns true if `path` lives on a known fuse filesystem (jmtpfs, sshfs…).
    pub fn is_fuse_path(&mut self, path: &Path) -> bool {
        self.refresh_fuse_mounts();
        self.fuse_mounts.iter().any(|mnt| path.starts_with(mnt))
    }

    // ── Async parent-pane directory load ─────────────────────────────────────

    /// Called from draw_parent_pane — requests a background load if the parent
    /// directory has changed. Never blocks the render thread.
    pub fn request_parent_load(&mut self, parent: PathBuf) {
        if self.parent_load_path.as_deref() == Some(&parent) { return; }
        // New target — reset and start debounce
        self.parent_load_path     = Some(parent);
        self.parent_load_rx       = None;
        self.parent_load_debounce = Some(Instant::now());
    }

    /// Called from tick() — spawns the thread once the debounce expires.
    pub fn maybe_spawn_parent_load(&mut self) {
        if self.parent_load_rx.is_some() { return; }
        let path = match self.parent_load_path.clone() { Some(p) => p, None => return };
        match self.parent_load_debounce {
            None => return,
            Some(t) if t.elapsed() < Duration::from_millis(80) => return,
            _ => { self.parent_load_debounce = None; }
        }
        let sh = self.tab().show_hidden;
        let (tx, rx) = mpsc::channel();
        self.parent_load_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(list_dir(&path, sh));
        });
    }

    // ── Async preview-pane directory load ────────────────────────────────────

    /// Called from draw_preview_pane — requests a background load if the hovered
    /// directory has changed. Never blocks the render thread.
    pub fn request_preview_load(&mut self, dir: PathBuf) {
        if self.preview_load_path.as_deref() == Some(&dir) { return; }
        self.preview_load_path     = Some(dir);
        self.preview_load_rx       = None;
        self.preview_load_debounce = Some(Instant::now());
    }

    /// Called from tick() — spawns the thread once the debounce expires.
    pub fn maybe_spawn_preview_load(&mut self) {
        if self.preview_load_rx.is_some() { return; }
        let path = match self.preview_load_path.clone() { Some(p) => p, None => return };
        match self.preview_load_debounce {
            None => return,
            Some(t) if t.elapsed() < Duration::from_millis(80) => return,
            _ => { self.preview_load_debounce = None; }
        }
        let sh = self.tab().show_hidden;
        let (tx, rx) = mpsc::channel();
        self.preview_load_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(list_dir(&path, sh));
        });
    }

    /// Request an async `du` calculation for `dir`. Skips fuse filesystems.
    pub fn spawn_folder_size(&mut self, dir: PathBuf) {
        // Skip fuse filesystems (jmtpfs, sshfs, …) — du blocks on these
        if self.is_fuse_path(&dir) {
            self.folder_size_val  = None;
            self.folder_size_path = None;
            return;
        }

        // Already spawned or computed for this exact directory — nothing to do
        if self.folder_size_path.as_deref() == Some(&dir) { return; }

        // Directory changed — reset state and start debounce timer
        self.folder_size_val      = None;
        self.folder_size_rx       = None;
        self.folder_size_path     = Some(dir.clone());
        self.folder_size_debounce = Some(Instant::now());
        // Return now — wait for debounce to expire on subsequent calls
    }

    /// Called from tick() — spawns `du` once the debounce timer expires.
    pub fn maybe_spawn_folder_size(&mut self) {
        if self.folder_size_rx.is_some() || self.folder_size_val.is_some() { return; }
        let dir = match &self.folder_size_path {
            Some(p) => p.clone(),
            None    => return,
        };
        match self.folder_size_debounce {
            None => return,
            Some(t) if t.elapsed() < Duration::from_millis(200) => return,
            _ => { self.folder_size_debounce = None; }
        }
        let (tx, rx) = mpsc::channel::<u64>();
        self.folder_size_rx = Some(rx);
        std::thread::spawn(move || {
            if let Some(size) = folder_size(&dir) {
                let _ = tx.send(size);
            }
        });
    }

    pub fn spawn_video_thumb(&mut self, video: PathBuf) {
        // Already displaying or already generating for this file
        if self.vid_thumb_path.as_deref() == Some(&video) { return; }

        // Debounce — only start ffmpeg after 150ms of hovering on the same file
        match self.vid_debounce {
            None => {
                self.vid_debounce = Some(Instant::now());
                return;
            }
            Some(t) if t.elapsed() < Duration::from_millis(150) => return,
            _ => { self.vid_debounce = None; }
        }

        // Cancel any in-flight generation and clean up
        if let Some(f) = self.vid_thumb_file.take() { let _ = fs::remove_file(&f); }
        self.vid_thumb_state = None;
        self.vid_thumb_path  = Some(video.clone());

        // Use a hash of the full path as the temp filename to avoid collisions
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            video.hash(&mut h);
            h.finish()
        };
        let tmp = env::temp_dir().join(format!("voiddream_thumb_{:x}.png", hash));
        self.vid_thumb_file = Some(tmp.clone());

        let (tx, rx) = mpsc::channel();
        self.vid_thumb_rx = Some(rx);

        std::thread::spawn(move || {
            let status = Command::new("ffmpeg")
                .args([
                    "-y", "-i", &video.to_string_lossy(),
                    "-ss", "00:00:02",
                    "-vframes", "1",
                    "-vf", "scale=320:-1",   // smaller = faster decode
                    "-q:v", "5",              // faster JPEG-quality encode
                    &tmp.to_string_lossy(),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if status.map(|s| s.success()).unwrap_or(false) && tmp.exists() {
                let _ = tx.send(tmp);
            }
        });
    }

    pub fn spawn_image_load(&mut self, path: PathBuf) {
        // Already showing this image
        if self.img_path.as_deref() == Some(&path) { return; }

        // Debounce — only load after 150ms of hovering
        match self.img_debounce {
            None => {
                self.img_debounce = Some(Instant::now());
                self.img_pending  = Some(path);
                return;
            }
            Some(t) if t.elapsed() < Duration::from_millis(150) => {
                self.img_pending = Some(path); // update target but keep waiting
                return;
            }
            _ => { self.img_debounce = None; }
        }

        // Only load the pending path (latest hovered file)
        let target = self.img_pending.take().unwrap_or(path);
        if self.img_path.as_deref() == Some(&target) { return; }

        self.img_path  = Some(target.clone());
        self.img_state = None;

        let (tx, rx) = mpsc::channel();
        self.img_rx = Some(rx);

        std::thread::spawn(move || {
            if let Ok(img) = image::open(&target) {
                let _ = tx.send(img);
            }
        });
    }

    /// Build and show the open-with context menu for the current file.
    pub fn open_with_menu(&mut self) {
        let path = match self.tab().current().cloned() { Some(p) => p, None => return };
        if path.is_dir() { return; }

        let ext = path.extension().and_then(|e| e.to_str())
            .map(|s| s.to_lowercase()).unwrap_or_default();
        let ext = ext.as_str();
        let cfg = &self.cfg;

        // Build list of (label, command) pairs based on file type
        // Always include editor and xdg-open as fallbacks
        let mut entries: Vec<(String, String)> = Vec::new();

        // Default opener for this file type first
        if IMAGE_EXT.contains(&ext) {
            entries.push(("Image viewer".into(), cfg.opener_image.clone()));
        } else if VIDEO_EXT.contains(&ext) {
            entries.push(("Video player".into(), cfg.opener_video.clone()));
        } else if AUDIO_EXT.contains(&ext) {
            entries.push(("Audio player".into(), cfg.opener_audio.clone()));
        } else if DOC_EXT.contains(&ext) {
            entries.push(("Document viewer".into(), cfg.opener_doc.clone()));
        } else if HTML_EXT.contains(&ext) {
            entries.push(("Browser".into(), cfg.opener_browser.clone()));
        } else if JAR_EXT.contains(&ext) {
            entries.push(("Java runtime".into(), cfg.opener_jar.clone()));
        }

        // Always offer: editor, image viewer, video player, browser, xdg-open
        let always = [
            ("Text editor",    cfg.opener_editor.clone()),
            ("Image viewer",   cfg.opener_image.clone()),
            ("Video player",   cfg.opener_video.clone()),
            ("Audio player",   cfg.opener_audio.clone()),
            ("Browser",        cfg.opener_browser.clone()),
            ("xdg-open",       "xdg-open".into()),
        ];
        for (label, cmd) in &always {
            // Skip if already in list (same cmd)
            if !entries.iter().any(|(_, c)| c == cmd) {
                entries.push((label.to_string(), cmd.clone()));
            }
        }

        // Always add custom command option at the bottom
        entries.push(("Custom command…".into(), "__custom__".into()));
        self.mode = InputMode::OpenWith(path, entries, 0);
    }

    pub fn open_drive_manager(&mut self) {
        self.drive_devices = crate::drives::list_devices();
        self.drive_cursor  = 0;
        self.mode          = InputMode::DriveManager;
    }

    pub fn drive_mount(&mut self) {
        let dev = match self.drive_devices.get(self.drive_cursor).cloned() {
            Some(d) => d, None => return,
        };
        if dev.mount.is_some() {
            self.msg(self.lang.msg_already_mounted, true);
            return;
        }
        match crate::drives::mount_device(&dev) {
            crate::drives::MountResult::Ok(path) => {
                self.msg(&format!("{} {}", self.lang.msg_mounted_at, path.display()), false);
                self.drive_devices = crate::drives::list_devices();
            }
            crate::drives::MountResult::Err(e) => {
                self.msg(&format!("{}: {}", self.lang.msg_mount_failed, e), true);
            }
        }
    }

    pub fn drive_unmount(&mut self) {
        let dev = match self.drive_devices.get(self.drive_cursor).cloned() {
            Some(d) => d, None => return,
        };
        if dev.mount.is_none() {
            self.msg(self.lang.msg_not_mounted, true);
            return;
        }
        match crate::drives::unmount_device(&dev) {
            Ok(()) => {
                self.msg(&format!("{} {}", self.lang.msg_unmounted, dev.label), false);
                self.drive_devices = crate::drives::list_devices();
            }
            Err(e) => {
                self.msg(&format!("{}: {}", self.lang.msg_unmount_failed, e), true);
            }
        }
    }

    pub fn drive_navigate(&mut self) {
        let dev = match self.drive_devices.get(self.drive_cursor).cloned() {
            Some(d) => d, None => return,
        };
        if !dev.is_navigable() {
            if dev.mount.is_none() {
                self.msg(self.lang.msg_not_mounted_hint, true);
            } else {
                self.msg(self.lang.msg_cant_navigate, true);
            }
            return;
        }
        if let Some(mnt) = dev.mount {
            self.mode = InputMode::Normal;
            self.tabs[self.tab_idx].cwd = mnt;
            self.tabs[self.tab_idx].refresh();
            // Explicitly land at top of directory, not wherever the cursor was before
            self.tabs[self.tab_idx].state.select(Some(0));
            self.reset_dir_load_state();
        }
    }

    pub fn open_fuzzy(&mut self) {
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
    pub fn fuzzy_update_results(&mut self) {
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
    pub fn fuzzy_accept(&mut self) {
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

