// ui.rs — all TUI drawing functions
use std::path::Path;
use ratatui::{
    prelude::StatefulWidget,
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
};
use ratatui_image::StatefulImage;
use crate::{config::*, types::*, app::App};

pub fn ui(f: &mut Frame, app: &mut App) {
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
        InputMode::OpenWith(..) => draw_openwith_overlay(f, app, sz),
        InputMode::OpenWithCustom(..) => draw_openwith_custom_overlay(f, app, sz),
        InputMode::DriveManager => draw_drive_overlay(f, app, sz),
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

fn draw_parent_pane(f: &mut Frame, app: &mut App, rect: Rect) {
    // Request async load — never blocks render thread.
    // tick() drains the channel and writes into tab.parent_entries.
    {
        let parent_path = app.tab().cwd.parent()
            .unwrap_or(&app.tab().cwd).to_path_buf();
        app.request_parent_load(parent_path);
    }
    let tab     = app.tab();
    let entries = app.tab().parent_entries.clone();
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

            // Fixed column layout so time/date always appear right after size:
            //  [icon 3] [name · · · · col_name] [size 7] [time 5] [  ] [date 10]
            let (time_str, date_str) = if app.cfg.show_file_mtime {
                format_mtime_split(e)
            } else { (String::new(), String::new()) };
            // col_meta = space taken by size + time + date + separators
            // icon=4 (nerd font wide char + spaces), size=7, " HH:MM  DD/MM/YYYY"=18
            // +2 safety margin for wide unicode rendering differences
            let col_meta: u16 = if app.cfg.show_file_mtime { 4 + 7 + 18 + 2 } else { 4 + 7 + 2 };
            let col_name = (rect.width.saturating_sub(col_meta)) as usize;
            let name_padded = {
                let clipped: String = name.chars().take(col_name).collect();
                format!("{:<col_name$}", clipped)
            };
            let meta_fg = if is_cur { app.theme.bg_primary } else { app.theme.fg_muted };
            let mut spans = vec![
                Span::styled(format!(" {} ", ic), st_bg(fg, bg)),
                Span::styled(name_padded, st_bg(fg, bg)),
                Span::styled(format!(" {:>6}", size), st_bg(meta_fg, bg)),
            ];
            if app.cfg.show_file_mtime {
                spans.push(Span::styled(format!(" {}  {}", time_str, date_str), st_bg(meta_fg, bg)));
            }
            ListItem::new(Line::from(spans))
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

    let mut state = ListState::default();
    if let Some(sel) = app.tab().state.selected() {
        let scroll = app.tab().scroll;
        if sel >= scroll {
            state.select(Some(sel - scroll));
        }
    }
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

    // Image preview — non-blocking, loaded in background thread
    if IMAGE_EXT.contains(&ext.as_str()) {
        // SVG: show file info — neither image crate nor ffmpeg handle SVGs reliably
        if ext == "svg" || ext == "svgz" {
            let size_str = human_size(current.metadata().map(|m| m.len()).unwrap_or(0));
            let lines = vec![
                Line::from(Span::styled("  SVG Vector Image", bold(app.theme.fg_dim))),
                Line::from(Span::styled(format!("  Size:   {}", size_str), st(app.theme.fg_dim))),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    format!("  Press Enter to open with {}", app.cfg.opener_image),
                    st(app.theme.fg_muted),
                )),
            ];
            let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
            f.render_widget(p, content_rect);
            return;
        }
        // RAW/HEIC/JXL/PSD/XCF can't be decoded by the image crate —
        // fall back to ffmpeg thumbnail (same path as video)
        // SVG is excluded — ffmpeg can't thumbnail SVGs; we show info text instead
        const FFMPEG_FALLBACK: &[&str] = &[
            "raw","arw","cr2","cr3","nef","nrw","orf","raf","rw2","dng","pef","srw","x3f",
            "heic","heif","jxl","xcf","psd","dds",
        ];
        if FFMPEG_FALLBACK.contains(&ext.as_str()) {
            app.spawn_video_thumb(current.clone());
            if let Some(state) = app.vid_thumb_state.as_mut() {
                StatefulImage::new().render(content_rect, f.buffer_mut(), state);
            } else {
                let generating = app.vid_thumb_rx.is_some();
                let p = Paragraph::new(Span::styled(
                    if generating { "  Generating preview…" } else { "  (preview unavailable — ffmpeg needed)" },
                    st_bg(app.theme.fg_muted, app.theme.bg_primary),
                ));
                f.render_widget(p, content_rect);
            }
        } else {
            app.spawn_image_load(current.clone());
            if let Some(state) = app.img_state.as_mut() {
                StatefulImage::new().render(content_rect, f.buffer_mut(), state);
            } else {
                let p = Paragraph::new(Span::styled(
                    &app.icons.chrome.no_image,
                    st_bg(app.theme.fg_muted, app.theme.bg_primary),
                ));
                f.render_widget(p, content_rect);
            }
        }
        return;
    }

    // Directory listing
    if current.is_dir() {
        // Request async directory load — tick() populates preview_entries.
        // Draw only reads the cache: no filesystem I/O on the render thread.
        app.request_preview_load(current.clone());
        let entries = app.tab().preview_entries.clone();

        // Kick off async folder size calculation (debounced, non-blocking)
        app.spawn_folder_size(current.clone());

        // Reserve 3 lines at the bottom for disk usage info (if space allows)
        let disk_lines: u16 = if content_rect.height >= 5 { 3 } else { 0 };
        let list_rect = Rect {
            x: content_rect.x,
            y: content_rect.y,
            width: content_rect.width,
            height: content_rect.height.saturating_sub(disk_lines),
        };
        let disk_rect = Rect {
            x: content_rect.x,
            y: content_rect.y + list_rect.height,
            width: content_rect.width,
            height: disk_lines,
        };

        let items: Vec<ListItem> = entries.iter().take(list_rect.height as usize).map(|e| {
            let ek = file_kind(e);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", app.icons.file_icon(e, &ek)), st(kind_color(&ek, &app.theme))),
                Span::styled(e.file_name().unwrap_or_default().to_string_lossy().to_string(), st(kind_color(&ek, &app.theme))),
            ]))
        }).collect();
        let block = Block::default().style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(List::new(items).block(block), list_rect);

        // Folder size footer — uses async cached result, never blocks
        if disk_lines > 0 {
            let entry_count = entries.len();
            let disk_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
                .split(disk_rect);

            let size_line = if let Some(folder_bytes) = app.folder_size_val {
                Line::from(vec![
                    Span::styled(format!("  {}: ", app.lang.preview_folder_size), st(app.theme.fg_muted)),
                    Span::styled(si_size(folder_bytes), bold(app.theme.accent)),
                ])
            } else {
                Line::from(Span::styled(format!("  {}: {}", app.lang.preview_folder_size, app.lang.preview_computing), st(app.theme.fg_muted)))
            };

            let count_line = Line::from(Span::styled(
                format!("  {} {}", entry_count, if entry_count == 1 { app.lang.preview_item } else { app.lang.preview_items }),
                st(app.theme.fg_muted),
            ));

            f.render_widget(
                Paragraph::new(size_line).style(st_bg(app.theme.fg_primary, app.theme.bg_primary)),
                disk_rows[0],
            );
            f.render_widget(
                Paragraph::new(count_line).style(st_bg(app.theme.fg_primary, app.theme.bg_primary)),
                disk_rows[1],
            );
        }
        return;
    }

    // HTML — open in browser, show info
    if HTML_EXT.contains(&ext.as_str()) {
        let size_str = human_size(current.metadata().map(|m| m.len()).unwrap_or(0));
        let lines = vec![
            Line::from(Span::styled("  HTML file", bold(app.theme.fg_dim))),
            Line::from(Span::styled(format!("  Size:   {}", size_str), st(app.theme.fg_dim))),
            Line::from(Span::raw("")),
            Line::from(Span::styled(
                format!("  Press Enter to open in {}", app.cfg.opener_browser),
                st(app.theme.fg_muted),
            )),
        ];
        let p = Paragraph::new(lines).style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(p, content_rect);
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
            format!("  \u{f410} {} {}  {}  ", app.lang.msg_extracting, prog.filename, app.lang.msg_esc_cancel),
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
            si_size(prog.current),
            si_size(prog.total),
        )
    } else {
        format!("  {} extracted  (total size unknown)", si_size(prog.current))
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

/// Format bytes using SI base-10 units (GB/MB/KB) to match file managers.
pub fn si_size(b: u64) -> String {
    // Use SI base-10 units (GB = 10^9) to match file managers like Nemo/Nautilus
    if b >= 1_000_000_000 { format!("{:.1} GB", b as f64 / 1_000_000_000.0) }
    else if b >= 1_000_000 { format!("{:.1} MB", b as f64 / 1_000_000.0) }
    else if b >= 1_000     { format!("{:.0} KB", b as f64 / 1_000.0) }
    else                   { format!("{} B", b) }
}

/// Returns the total apparent size of a folder in bytes using `du -sb`,
/// matching what file managers like Nemo/Nautilus report.
///
/// Returns the total apparent size of a folder in bytes using `du -sb`.
/// Fuse-filesystem guard is handled upstream in `App::spawn_folder_size`
/// via `is_fuse_path`, so by the time this function is called the path is
/// known to be on a normal local filesystem.
pub fn folder_size(path: &Path) -> Option<u64> {
    use std::process::Command;
    let out = Command::new("du")
        .args(["-sb", path.to_str()?])
        .output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next()
        .and_then(|l| l.split_whitespace().next())
        .and_then(|w| w.parse::<u64>().ok())
}

fn draw_drive_overlay(f: &mut Frame, app: &App, rect: Rect) {
    use crate::drives::DeviceKind;

    let ow = (rect.width * 3 / 5).max(64).min(rect.width);
    let oh = (rect.height * 2 / 3).max(14).min(rect.height);
    let ox = (rect.width  - ow) / 2;
    let oy = (rect.height - oh) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .title(Span::styled(
            format!("  {} {}  ", app.icons.chrome.help_icon, app.lang.drive_title),
            bold_bg(app.theme.fg_dim, app.theme.bg_primary),
        ))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
    f.render_widget(block, popup);

    let inner = Rect {
        x: popup.x + 1, y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };

    let devices = &app.drive_devices;

    if devices.is_empty() {
        let msg = Paragraph::new(vec![
            Line::from(Span::styled(format!("  {}", app.lang.drive_no_devices), st(app.theme.fg_muted))),
            Line::from(Span::raw("")),
            Line::from(Span::styled(format!("  • {}", app.lang.drive_tip_plug), st(app.theme.fg_dim))),
            Line::from(Span::styled(format!("  • {}", app.lang.drive_tip_mtp), st(app.theme.fg_dim))),
            Line::from(Span::styled(format!("  • {}", app.lang.drive_tip_jmtpfs), st(app.theme.fg_dim))),
        ])
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
        f.render_widget(msg, inner);
        return;
    }

    // Header row
    let header_rect = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
    let list_rect   = Rect {
        x: inner.x, y: inner.y + 1,
        width: inner.width, height: inner.height.saturating_sub(2),
    };
    let hint_rect   = Rect {
        x: inner.x, y: inner.y + inner.height.saturating_sub(1),
        width: inner.width, height: 1,
    };

    // Column widths
    let w = inner.width as usize;
    let col_icon  = 2usize;
    let col_size  = 6usize;
    let col_fs    = 7usize;
    let col_mnt   = (w / 3).max(10);
    let col_label = w.saturating_sub(col_icon + col_size + col_fs + col_mnt + 4);

    let hdr = Line::from(vec![
        Span::styled(format!("  {:<col_label$}  {:<col_size$}  {:<col_fs$}  {}", app.lang.drive_col_name, app.lang.drive_col_size, app.lang.drive_col_fs, app.lang.drive_col_mount),
            bold(app.theme.fg_dim)),
    ]);
    f.render_widget(Paragraph::new(hdr).style(st_bg(app.theme.fg_primary, app.theme.bg_primary)), header_rect);

    let items: Vec<ListItem> = devices.iter().enumerate().map(|(i, dev)| {
        let selected = i == app.drive_cursor;
        let is_internal = matches!(dev.kind, DeviceKind::Internal);
        let (fg, bg) = if selected {
            (app.theme.bg_primary, app.theme.accent)
        } else if is_internal {
            (app.theme.fg_dim, app.theme.bg_primary)
        } else {
            (app.theme.fg_primary, app.theme.bg_primary)
        };

        let icon = match dev.kind {
            DeviceKind::Internal  => "󰋊",   // internal HDD/SSD
            DeviceKind::Removable => "󱊡",   // USB / external
            DeviceKind::MtpPhone  => "󰄜",   // phone
        };
        let mounted_str = dev.mount.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "—".to_string());
        let mnt_display = if mounted_str.len() > col_mnt {
            format!("…{}", &mounted_str[mounted_str.len().saturating_sub(col_mnt - 1)..])
        } else {
            format!("{:<col_mnt$}", mounted_str)
        };

        let label_display = if dev.label.len() > col_label {
            format!("{}…", &dev.label[..col_label.saturating_sub(1)])
        } else {
            format!("{:<col_label$}", dev.label)
        };

        let mount_color = if dev.mount.is_some() { app.theme.accent } else { app.theme.fg_muted };

        Line::from(vec![
            Span::styled(format!(" {} ", icon), st_bg(fg, bg)),
            Span::styled(format!("{} ", label_display), st_bg(fg, bg)),
            Span::styled(format!("{:<col_size$} ", dev.size), st_bg(app.theme.fg_muted, bg)),
            Span::styled(format!("{:<col_fs$} ", dev.fstype), st_bg(app.theme.fg_dim, bg)),
            Span::styled(mnt_display, if selected { st_bg(fg, bg) } else { st_bg(mount_color, bg) }),
        ])
    }).map(ListItem::new).collect();

    let list = List::new(items)
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
    f.render_widget(list, list_rect);

    // Hint bar at bottom
    let l = app.lang;
    let hint = Line::from(vec![
        Span::styled("  m", bold(app.theme.accent)),
        Span::styled(format!(" {}  ", l.drive_hint_mount), st(app.theme.fg_muted)),
        Span::styled("u", bold(app.theme.accent)),
        Span::styled(format!(" {}  ", l.drive_hint_unmount), st(app.theme.fg_muted)),
        Span::styled("Enter", bold(app.theme.accent)),
        Span::styled(format!(" {}  ", l.drive_hint_navigate), st(app.theme.fg_muted)),
        Span::styled("r", bold(app.theme.accent)),
        Span::styled(format!(" {}  ", l.drive_hint_refresh), st(app.theme.fg_muted)),
        Span::styled("Esc", bold(app.theme.accent)),
        Span::styled(format!(" {}", l.drive_hint_close), st(app.theme.fg_muted)),
    ]);
    f.render_widget(
        Paragraph::new(hint).style(st_bg(app.theme.fg_primary, app.theme.bg_primary)),
        hint_rect,
    );
}

fn draw_help_overlay(f: &mut Frame, app: &mut App, rect: Rect) {
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
            format!("  {} Keybinds  [Esc close | ↑↓ / jk scroll | Home top]  ", app.icons.chrome.help_icon),
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
    let l = app.lang;
    let nav_section = vec![
        ("\u{2191} / \u{2193}".to_string(),  l.help_move_cursor.to_string()),
        ("\u{2192} / Enter".to_string(),     l.help_open_enter.to_string()),
        ("\u{2190} / Backspace".to_string(), l.help_go_up.to_string()),
        ("Page Up / Down".to_string(),       l.help_jump10.to_string()),
        ("Home / End".to_string(),           l.help_first_last.to_string()),
    ];
    let file_section = vec![
        (cfg.key_select.clone(),             l.help_select.to_string()),
        ("Ctrl+a  /  A".to_string(),         l.help_select_all.to_string()),
        ("Ctrl+r".to_string(),               l.help_deselect_all.to_string()),
        (cfg.key_copy.clone(),               l.help_copy.to_string()),
        (cfg.key_cut.clone(),                l.help_cut.to_string()),
        (cfg.key_paste.clone(),              l.help_paste.to_string()),
        (cfg.key_delete.clone(),             l.help_delete.to_string()),
        (cfg.key_rename.clone(),             l.help_rename.to_string()),
        (cfg.key_new_file.clone(),           l.help_new_file.to_string()),
        (cfg.key_new_dir.clone(),            l.help_new_dir.to_string()),
    ];
    let search_section = vec![
        (cfg.key_search.clone(),             l.help_fuzzy.to_string()),
    ];
    let tab_section = vec![
        (cfg.key_new_tab.clone(),            l.help_new_tab.to_string()),
        (cfg.key_close_tab.clone(),          l.help_close_tab.to_string()),
        (cfg.key_cycle_tab.clone(),          l.help_cycle_tab.to_string()),
    ];
    let app_section = vec![
        (cfg.key_toggle_hidden.clone(),      l.help_toggle_hidden.to_string()),
        ("D".to_string(),                    l.help_drives.to_string()),
        ("m  (in drive overlay)".to_string(),l.help_drive_mount.to_string()),
        ("u  (in drive overlay)".to_string(),l.help_drive_unmount.to_string()),
        ("r  (in drive overlay)".to_string(),l.help_drive_refresh.to_string()),
        (":".to_string(),                    l.help_open_settings.to_string()),
        ("?".to_string(),                    l.help_show_help.to_string()),
        (format!("{}  /  Esc", cfg.key_quit), l.help_quit.to_string()),
    ];

    let sections: &[(&str, &Vec<(String, String)>)] = &[
        (&format!("{}  {}", app.icons.chrome.nav_icon,        l.help_navigation), &nav_section),
        (&format!("{}  {}", app.icons.chrome.ops_icon,        l.help_file_ops),   &file_section),
        (&format!("{}  {}", app.icons.chrome.search_sec_icon, l.help_search),     &search_section),
        (&format!("{}  {}", app.icons.chrome.tab_sec_icon,    l.help_tabs),       &tab_section),
        (&format!("{}  {}", app.icons.chrome.settings_icon,   l.help_app),        &app_section),
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

    let total_lines = lines.len() as u16;
    let visible_h   = inner.height;
    // Clamp scroll so we can't scroll past the last line
    let max_scroll = total_lines.saturating_sub(visible_h) as usize;
    if app.help_scroll > max_scroll { app.help_scroll = max_scroll; }
    let scroll = app.help_scroll as u16;

    let p = Paragraph::new(lines)
        .scroll((scroll, 0))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_primary));
    f.render_widget(p, inner);

    // Scroll indicator in bottom-right if there's more content
    if total_lines > visible_h {
        let pct = if max_scroll == 0 { 100 } else { (app.help_scroll * 100 / max_scroll).min(100) };
        let indicator = format!(" ↑↓ scroll  {}% ", pct);
        let ind_w = indicator.chars().count() as u16;
        let ind_rect = Rect {
            x: popup.x + popup.width.saturating_sub(ind_w + 1),
            y: popup.y + popup.height.saturating_sub(1),
            width: ind_w,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(indicator, bold_bg(app.theme.fg_dim, app.theme.bg_primary))),
            ind_rect,
        );
    }
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
    let l = app.lang;
    let sections = [
        (SettingsSection::Behaviour,  l.sec_behaviour),
        (SettingsSection::Appearance, l.sec_appearance),
        (SettingsSection::Openers,    l.sec_openers),
        (SettingsSection::Keybinds,   l.sec_keybinds),
        (SettingsSection::About,      l.sec_about),
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
            SettingsSection::About      => 4,
        })
        .style(st_bg(app.theme.fg_dim, app.theme.bg_panel))
        .divider(Span::raw(""));
    f.render_widget(tabs, inner[0]);

    // About section — custom read-only display
    if app.settings.section == SettingsSection::About {
        let accent = app.theme.accent;
        let fg     = app.theme.fg_primary;
        let muted  = app.theme.fg_muted;
        let bg     = app.theme.bg_primary;
        let l = app.lang;
        let lines = vec![
            Line::from(vec![]),
            Line::from(vec![
                Span::styled("  VoidDream", bold(accent)),
                Span::styled(format!("  —  {}", l.about_tagline), st(fg)),
            ]),
            Line::from(vec![]),
            Line::from(vec![
                Span::styled(format!("  {:20}", l.about_ver),     st(muted)),
                Span::styled("0.1.6", bold(fg)),
            ]),
            Line::from(vec![
                Span::styled(format!("  {:20}", l.about_author),  st(muted)),
                Span::styled("FemBoyGamerTechGuy", bold(fg)),
            ]),
            Line::from(vec![
                Span::styled(format!("  {:20}", l.about_license), st(muted)),
                Span::styled("GPL-3.0-or-later", bold(fg)),
            ]),
            Line::from(vec![
                Span::styled(format!("  {:20}", l.about_repo),    st(muted)),
                Span::styled("github.com/FemBoyGamerTechGuy/VoidDream", bold(accent)),
            ]),
            Line::from(vec![]),
            Line::from(vec![
                Span::styled(format!("  {}", l.about_built_with), st(muted)),
            ]),
        ];
        f.render_widget(
            Paragraph::new(lines).style(st_bg(fg, bg)),
            inner[1],
        );
        return;
    }

    // Items
    let items = SettingsState::section_items(&app.settings.section);
    let l: &'static crate::lang::Lang = app.lang;
    let translate_label = |key: &str, label: &'static str| -> &'static str {
        match key {
            "language"          => "Language",
            "show_hidden"       => l.set_show_hidden,
            "date_format"       => l.set_date_format,
            "show_clock"        => l.set_show_clock,
            "show_file_mtime"   => l.set_show_mtime,
            "key_select"        => l.kb_select,
            "key_select_all"    => l.kb_select_all,
            "key_copy"          => l.kb_copy,
            "key_cut"           => l.kb_cut,
            "key_paste"         => l.kb_paste,
            "key_delete"        => l.kb_delete,
            "key_rename"        => l.kb_rename,
            "key_new_file"      => l.kb_new_file,
            "key_new_dir"       => l.kb_new_dir,
            "key_search"        => l.kb_search,
            "key_toggle_hidden" => l.kb_toggle_hidden,
            "key_new_tab"       => l.kb_new_tab,
            "key_close_tab"     => l.kb_close_tab,
            "key_cycle_tab"     => l.kb_cycle_tab,
            "key_quit"          => l.kb_quit,
            "fixed_drives"      => l.kb_drives,
            "fixed_drive_m"     => l.kb_drive_mount,
            "fixed_drive_u"     => l.kb_drive_unmount,
            "fixed_drive_r"     => l.kb_drive_refresh,
            "fixed_nav"         => l.kb_navigate,
            "fixed_open"        => l.kb_open,
            "fixed_up"          => l.kb_go_up,
            "fixed_pgupdown"    => l.kb_jump,
            "fixed_homeend"     => l.kb_first_last,
            "fixed_deselect"    => l.kb_deselect_all,
            "fixed_sel_all2"    => l.kb_select_all2,
            "fixed_open_with"   => l.kb_open_with,
            "fixed_settings"    => l.kb_settings,
            "fixed_help"        => l.kb_help,
            "fixed_quit2"       => l.kb_quit2,
            "fixed_about_app"   => l.about_app,
            "fixed_about_ver"   => l.about_ver,
            "fixed_about_author"=> l.about_author,
            "fixed_about_license"=>l.about_license,
            "fixed_about_repo"  => l.about_repo,
            _                   => label,
        }
    };
    let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, (key, label))| {
        let label = translate_label(key, label);
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
        let hint = Paragraph::new(format!("  {}", app.lang.msg_unsaved_changes))
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
    let (path, focus_end, start_args, end_args) = match &app.mode {
        InputMode::RunArgs(p, fe, sa, ea) => (p.clone(), *fe, sa.clone(), ea.clone()),
        _ => return,
    };

    // ── Build base command string ─────────────────────────────────────────────
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
    let is_script = matches!(ext.as_str(), "sh"|"bash"|"zsh"|"fish");
    let is_exec = {
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false) }
        #[cfg(not(unix))] { false }
    };
    let path_escaped = path.to_string_lossy().replace("'", "'\\''");
    let _base_cmd = if is_script && !is_exec {
        match ext.as_str() {
            "fish" => format!("fish '{}'", path_escaped),
            "zsh"  => format!("zsh '{}'",  path_escaped),
            "bash" => format!("bash '{}'", path_escaped),
            _      => format!("sh '{}'",   path_escaped),
        }
    } else if path.is_absolute() {
        format!("'{}'", path_escaped)
    } else {
        format!("./'{}'", path_escaped)
    };

    // Current live args (what's typed in input_buf for the active field)
    let live_start = if !focus_end { app.input_buf.as_str() } else { start_args.as_str() };
    let live_end   = if  focus_end { app.input_buf.as_str() } else { end_args.as_str() };

    // Build full command preview — use just the filename, not the full path
    let mut parts: Vec<&str> = Vec::new();
    if !live_start.is_empty() { parts.push(live_start); }
    parts.push(fname);
    if !live_end.is_empty() { parts.push(live_end); }
    let preview = parts.join(" ");

    // ── Layout ────────────────────────────────────────────────────────────────
    let ow = (rect.width * 2 / 3).max(56).min(rect.width);
    let oh = 9u16;
    let ox = (rect.width.saturating_sub(ow)) / 2;
    let oy = (rect.height.saturating_sub(oh)) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_panel));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // centered filename
            Constraint::Length(1), // command preview
            Constraint::Length(1), // spacer
            Constraint::Length(1), // START input
            Constraint::Length(1), // END input
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // ── Row 0: centered filename ──────────────────────────────────────────────
    let name_max = inner.width as usize;
    let fname_shown: String = if fname.chars().count() > name_max.saturating_sub(4) {
        fname.chars().take(name_max.saturating_sub(5)).collect::<String>() + "…"
    } else { fname.to_string() };
    let pad = (inner.width as usize).saturating_sub(fname_shown.chars().count()) / 2;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ".repeat(pad), st_bg(app.theme.bg_panel, app.theme.bg_panel)),
            Span::styled(&fname_shown, bold_bg(app.theme.fg_primary, app.theme.bg_panel)),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[0],
    );

    // ── Row 1: command preview ────────────────────────────────────────────────
    let max_prev = (rows[1].width as usize).saturating_sub(5);
    let prev_shown: String = if preview.chars().count() > max_prev {
        preview.chars().take(max_prev.saturating_sub(1)).collect::<String>() + "…"
    } else { preview.clone() };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  $ ", bold(app.theme.ok)),
            Span::styled(prev_shown, st(app.theme.fg_dim)),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[1],
    );

    // ── Row 3: START input ────────────────────────────────────────────────────
    let start_active = !focus_end;
    let start_label = Span::styled(
        if start_active { " START ▸ " } else { " START   " },
        if start_active { bold_bg(app.theme.bg_primary, app.theme.ok) }
        else { st_bg(app.theme.fg_muted, app.theme.bg_panel) },
    );
    let start_val = if start_active {
        format!("{}{}", app.input_buf, app.icons.chrome.cursor_block)
    } else { start_args.clone() };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            start_label,
            Span::styled(start_val, st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[3],
    );

    // ── Row 4: END input ──────────────────────────────────────────────────────
    let end_active = focus_end;
    let end_label = Span::styled(
        if end_active { "   END ▸ " } else { "   END   " },
        if end_active { bold_bg(app.theme.bg_primary, app.theme.accent) }
        else { st_bg(app.theme.fg_muted, app.theme.bg_panel) },
    );
    let end_val = if end_active {
        format!("{}{}", app.input_buf, app.icons.chrome.cursor_block)
    } else { end_args.clone() };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            end_label,
            Span::styled(end_val, st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[4],
    );

    // ── Row 6: hints ─────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Tab", bold(app.theme.accent)),
            Span::styled(" switch  ", st(app.theme.fg_muted)),
            Span::styled("Enter", bold(app.theme.accent)),
            Span::styled(" run  ", st(app.theme.fg_muted)),
            Span::styled("Esc", bold(app.theme.accent)),
            Span::styled(" cancel", st(app.theme.fg_muted)),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[6],
    );
}

fn draw_openwith_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let (path, entries, cur) = match &app.mode {
        InputMode::OpenWith(p, e, c) => (p.clone(), e.clone(), *c),
        _ => return,
    };
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");

    let oh = (entries.len() as u16 + 4).min(rect.height);
    let ow = (rect.width / 2).max(48).min(rect.width);
    let ox = (rect.width.saturating_sub(ow)) / 2;
    let oy = (rect.height.saturating_sub(oh)) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .title(Span::styled(
            format!("  Open \"{}\" with…  ", fname),
            bold_bg(app.theme.fg_dim, app.theme.bg_primary),
        ))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_panel));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // Hint row at bottom
    let hint_rect = Rect { x: inner.x, y: inner.y + inner.height.saturating_sub(1),
                           width: inner.width, height: 1 };
    let list_rect = Rect { x: inner.x, y: inner.y,
                           width: inner.width, height: inner.height.saturating_sub(1) };

    let items: Vec<ListItem> = entries.iter().enumerate().map(|(i, (label, cmd))| {
        let is_cur = i == cur;
        let (fg, bg) = if is_cur {
            (app.theme.bg_primary, app.theme.accent)
        } else {
            (app.theme.fg_primary, app.theme.bg_panel)
        };
        let arrow = if is_cur { "▸ " } else { "  " };
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {}{}", arrow, label), bold_bg(fg, bg)),
            Span::styled(format!("  {}", cmd), st_bg(app.theme.fg_muted, bg)),
        ]))
    }).collect();

    let mut state = ListState::default();
    state.select(Some(cur));
    f.render_stateful_widget(
        List::new(items).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        list_rect, &mut state,
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ↑↓", bold(app.theme.accent)),
            Span::styled(" navigate  ", st(app.theme.fg_muted)),
            Span::styled("Enter", bold(app.theme.accent)),
            Span::styled(" open  ", st(app.theme.fg_muted)),
            Span::styled("Esc", bold(app.theme.accent)),
            Span::styled(" cancel", st(app.theme.fg_muted)),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        hint_rect,
    );
}

fn draw_openwith_custom_overlay(f: &mut Frame, app: &App, rect: Rect) {
    let path = match &app.mode {
        InputMode::OpenWithCustom(p) => p.clone(),
        _ => return,
    };
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");

    let ow = (rect.width / 2).max(52).min(rect.width);
    let oh = 5u16;
    let ox = (rect.width.saturating_sub(ow)) / 2;
    let oy = (rect.height.saturating_sub(oh)) / 2;
    let popup = Rect { x: ox, y: oy, width: ow, height: oh };
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(bold(app.theme.accent))
        .title(Span::styled(
            format!("  Custom command for \"{}\"  ", fname),
            bold_bg(app.theme.fg_dim, app.theme.bg_primary),
        ))
        .style(st_bg(app.theme.fg_primary, app.theme.bg_panel));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  Will run: <cmd> '{}'", fname),
            st(app.theme.fg_muted),
        )).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  cmd: ", st_bg(app.theme.fg_muted, app.theme.bg_panel)),
            Span::styled(
                format!("{}\u{2588}", app.input_buf),
                bold_bg(app.theme.fg_primary, app.theme.bg_panel),
            ),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Enter", bold(app.theme.accent)),
            Span::styled(" run  ", st(app.theme.fg_muted)),
            Span::styled("Esc", bold(app.theme.accent)),
            Span::styled(" back", st(app.theme.fg_muted)),
        ])).style(st_bg(app.theme.fg_primary, app.theme.bg_panel)),
        rows[2],
    );
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

