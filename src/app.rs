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
    // Folder size calculation — async background thread
    // Live clock string, updated every tick
    pub clock_str:    String,
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
        else { self.msg("Nothing to yank", true); return; };
        let n = targets.len();
        self.yank = targets; self.yank_cut = cut;
        self.tab_mut().selected.clear();
        self.msg(&format!("{} item(s) {}", n, if cut {"cut"} else {"copied"}), false);
    }
    pub fn paste_files(&mut self) {
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
        if errors.is_empty() { self.msg(&format!("Deleted {} item(s)", targets.len()), false); }
        else { self.msg(&format!("Error: {}", errors[0]), true); }
    }
    pub fn open_current(&mut self) {
        let path = match self.tab().current().cloned() { Some(p) => p, None => return };
        if path.is_dir() { self.tab_mut().enter(); return; }
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
        let ext = ext.as_str(); let cfg = self.cfg.clone();
        if IMAGE_EXT.contains(&ext)        { let _ = Command::new(&cfg.opener_image).arg(&path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn(); }
        else if VIDEO_EXT.contains(&ext)   { let _ = Command::new(&cfg.opener_video).arg(&path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn(); }
        else if AUDIO_EXT.contains(&ext)   { let _ = Command::new(&cfg.opener_audio).arg(&path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn(); }
        else if DOC_EXT.contains(&ext)     { let _ = Command::new(&cfg.opener_doc).arg(&path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn(); }
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
        else if HTML_EXT.contains(&ext)    { let _ = Command::new(&cfg.opener_browser).arg(&path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn(); }
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
            done: false, error: None, start_time: Instant::now(), pid: None,
        });
        self.mode = InputMode::Extracting;

        let fname = filename.clone();
        std::thread::spawn(move || {
            // Choose command + args based on format; pipe stdout for progress parsing
            let result = Self::run_extraction_with_progress(&src_s, &dst_s, &ext, &name_lower, total_bytes, &tx, &fname);
            let _ = tx.send(ExtractionProgress {
                filename: fname, current: total_bytes.max(1), total: total_bytes.max(1),
                done: true, error: result.err().map(|e| e.to_string()),
                start_time: Instant::now(), pid: None,
            });
        });
    }

    fn archive_total_size(src_s: &str, ext: &str, name_lower: &str) -> u64 {
        if ext == "rar" {
            // Use `unrar lt` (technical listing) which emits "Size: <bytes>" per file
            // Sum those — avoids any ambiguity with the summary line format.
            if let Ok(out) = Command::new("unrar").args(["lt", src_s]).output() {
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                let total: u64 = text.lines()
                    .filter_map(|l| {
                        let t = l.trim();
                        // Lines look like: "   Size: 1234567"
                        if t.to_lowercase().starts_with("size:") {
                            t.split_whitespace().nth(1)?.parse::<u64>().ok()
                        } else {
                            None
                        }
                    })
                    .sum();
                if total > 0 { return total; }
            }
            // Fallback: `unrar l` summary line "X files, Y bytes (Z GiB)"
            if let Ok(out) = Command::new("unrar").args(["l", src_s]).output() {
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                for line in text.lines().rev() {
                    let low = line.to_lowercase();
                    if low.contains("bytes") || low.contains("byte") {
                        // Grab the number immediately before "bytes"
                        let words: Vec<&str> = line.split_whitespace().collect();
                        for i in 1..words.len() {
                            if words[i].to_lowercase().starts_with("byte") {
                                if let Ok(n) = words[i-1].trim_end_matches(',').parse::<u64>() {
                                    if n > 1024 { return n; }
                                }
                            }
                        }
                    }
                }
            }
            return 0;
        }

        let out = if ext == "zip" {
            Command::new("unzip").args(["-l", src_s]).output().ok()
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
        let child_pid = child.id();
        let stdout = child.stdout.take().expect("stdout piped");
        let reader = BufReader::new(stdout);

        // Send PID immediately so App can kill the process if ESC is pressed
        let _ = tx.send(ExtractionProgress {
            filename: fname.to_string(), current: 0, total,
            done: false, error: None, start_time: Instant::now(),
            pid: Some(child_pid),
        });

        let mut extracted: u64 = 0;
        let mut files_extracted: u64 = 0;
        let mut total_files: u64 = 0;

        // For RAR: unrar x output lines look like:
        //   "Extracting  path/to/file.ext                    OK"
        // There are no per-file byte sizes in the output — only filenames.
        // We track file count and scale to bytes proportionally using total_bytes.
        // For other formats: parse sizes from lines as before.
        let is_rar = ext == "rar";

        // Pre-count total files for RAR so we can show proportional progress
        if is_rar && total > 0 {
            if let Ok(out) = Command::new("unrar").args(["l", src_s]).output() {
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                total_files = text.lines()
                    .filter(|l| {
                        // Count listing lines that represent files (have a size column)
                        let trimmed = l.trim();
                        !trimmed.is_empty()
                            && !trimmed.starts_with("Archive:")
                            && !trimmed.starts_with("Details:")
                            && !trimmed.starts_with("Name")
                            && !trimmed.starts_with("----")
                            && !trimmed.contains("files,")
                            && trimmed.split_whitespace().count() >= 2
                    })
                    .count() as u64;
            }
        }

        for line in reader.lines() {
            let line = match line { Ok(l) => l, Err(_) => break };

            if is_rar {
                // Count "Extracting" lines as file completions
                if line.trim_start().starts_with("Extracting") {
                    files_extracted += 1;
                    if total > 0 && total_files > 0 {
                        extracted = (files_extracted * total / total_files).min(total);
                    } else {
                        extracted = files_extracted;
                    }
                }
            } else {
                // For tar/zip/7z: parse size from the output line
                let size_in_line: u64 = line.split_whitespace()
                    .filter_map(|w| w.parse::<u64>().ok())
                    .next()
                    .unwrap_or(0);

                if total == 0 {
                    extracted += 1;
                } else {
                    extracted = (extracted + size_in_line.max(1)).min(total);
                }
            }

            let _ = tx.send(ExtractionProgress {
                filename:   fname.to_string(),
                current:    extracted,
                total,
                done:       false,
                error:      None,
                start_time: Instant::now(),
                pid:        None,  // already sent on first message
            });
        }

        child.wait()?;
        Ok(())
    }
    pub fn new_tab(&mut self) {
        let cwd = self.tab().cwd.clone(); let sh = self.cfg.show_hidden;
        self.tabs.push(Tab::new(cwd, sh));
        self.tab_idx = self.tabs.len() - 1;
        self.msg(&format!("Tab {} opened", self.tab_idx+1), false);
    }
    pub fn close_tab(&mut self) {
        if self.tabs.len() == 1 { self.msg("Can't close last tab", true); return; }
        self.tabs.remove(self.tab_idx);
        self.tab_idx = self.tab_idx.min(self.tabs.len()-1);
    }
    /// Spawn a background thread that runs ffmpeg to extract a thumbnail frame.
    /// Sends the output path on the channel when done; App::tick() picks it up.
    pub fn spawn_folder_size(&mut self, dir: PathBuf) {
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
        // Called from tick() — spawns du once debounce has expired
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

