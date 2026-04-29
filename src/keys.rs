// keys.rs — keyboard input handlers
use std::{fs, process::Command};
use crossterm::event::{KeyCode, KeyModifiers};
use crate::{config::*, types::*, app::App};

pub fn handle_key(app: &mut App, key: KeyCode, mods: KeyModifiers) -> bool {
    if app.mode == InputMode::Settings {
        return handle_settings_key(app, key, mods);
    }
    match &app.mode {
        InputMode::Normal | InputMode::Settings => {}
        InputMode::DriveManager => {
            match key {
                KeyCode::Esc => { app.mode = InputMode::Normal; }
                _ if crate::config::key_matches(&app.cfg.key_nav_up,       key, mods) => {
                    if app.drive_cursor > 0 { app.drive_cursor -= 1; }
                }
                _ if crate::config::key_matches(&app.cfg.key_nav_down,     key, mods) => {
                    if app.drive_cursor + 1 < app.drive_devices.len() { app.drive_cursor += 1; }
                }
                _ if crate::config::key_matches(&app.cfg.key_drive_mount,   key, mods) => { app.drive_mount(); }
                _ if crate::config::key_matches(&app.cfg.key_drive_unmount, key, mods) => { app.drive_unmount(); }
                _ if crate::config::key_matches(&app.cfg.key_drive_refresh, key, mods) => { app.drive_devices = crate::drives::list_devices(); }
                KeyCode::Enter => { app.drive_navigate(); }
                _ => {}
            }
            return false;
        }
        InputMode::FuzzySearch => { handle_fuzzy_key(app, key, mods); return false; }
        InputMode::Rename(_)|InputMode::NewFile|InputMode::NewDir => { handle_input_key(app, key); return false; }
        InputMode::RunArgs(..) => { handle_runargs_key(app, key); return false; }
        InputMode::OpenWith(..) => { handle_openwith_key(app, key); return false; }
        InputMode::OpenWithCustom(..) => { handle_openwith_custom_key(app, key); return false; }
        InputMode::FirstRunSetup => { handle_setup_key(app, key); return false; }
        InputMode::KeybindMenu   => { handle_keybind_menu(app, key); return false; }
        InputMode::KeyCapture    => { handle_key_capture(app, key, mods); return false; }
        InputMode::KeybindRemove => { handle_keybind_remove(app, key); return false; }
        InputMode::Confirm => {
            app.mode = InputMode::Normal;
            if matches!(key, KeyCode::Char('y')|KeyCode::Char('Y')) { app.delete_files(); }
            return false;
        }
        InputMode::Help => {
            match key {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                    app.mode = InputMode::Normal;
                    app.help_scroll = 0;
                }
                KeyCode::Up   | KeyCode::Char('k') => {
                    if app.help_scroll > 0 { app.help_scroll -= 1; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.help_scroll = app.help_scroll.saturating_add(1);
                }
                KeyCode::PageUp => {
                    app.help_scroll = app.help_scroll.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    app.help_scroll = app.help_scroll.saturating_add(10);
                }
                KeyCode::Home => { app.help_scroll = 0; }
                _ => {}
            }
            return false;
        }
        InputMode::Extracting => {
            if key == KeyCode::Esc {
                // Kill the child extraction process
                if let Some(pid) = app.extract_child_pid.take() {
                    // SIGTERM the extraction process — no libc dep needed
                    let _ = Command::new("kill").arg(pid.to_string()).spawn();
                }
                app.extract_rx        = None;
                app.extract_progress  = None;
                app.extract_child_pid = None;
                app.mode              = InputMode::Normal;
                // Invalidate folder size cache so new partial files are counted
                app.folder_size_path     = None;
                app.folder_size_val      = None;
                app.folder_size_rx       = None;
                app.folder_size_debounce = None;
                app.tab_mut().refresh();
                app.msg("Extraction cancelled", true);
            }
            return false;
        }
        InputMode::Copying => {
            if key == KeyCode::Esc {
                // Signal the background thread to stop — it will clean up partial files
                if let Some(ref cancel) = app.copy_cancel {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                app.copy_rx       = None;
                app.copy_progress = None;
                app.copy_cancel   = None;
                app.mode          = InputMode::Normal;
                // Clear yank so the files aren't still queued for copy
                app.yank.clear();
                app.yank_cut = false;
                app.tab_mut().refresh();
                app.msg("Copy cancelled", true);
            }
            return false;
        }
        InputMode::Deleting => {
            if key == KeyCode::Esc {
                if let Some(ref cancel) = app.delete_cancel {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                app.delete_rx       = None;
                app.delete_progress = None;
                app.delete_cancel   = None;
                app.mode            = InputMode::Normal;
                app.tab_mut().refresh();
                app.msg("Delete cancelled", true);
            }
            return false;
        }
        InputMode::Trashing => {
            if key == KeyCode::Esc {
                app.trash_rx       = None;
                app.trash_progress = None;
                app.mode           = InputMode::Normal;
                app.tab_mut().refresh();
                app.msg("Trash cancelled", true);
            }
            return false;
        }
        InputMode::TrashBrowser => {
            match key {
                KeyCode::Esc | KeyCode::Char('q') => { app.mode = InputMode::Normal; }
                KeyCode::Up   | KeyCode::Char('k') => {
                    if app.trash_cursor > 0 { app.trash_cursor -= 1; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if app.trash_cursor + 1 < app.trash_entries.len() { app.trash_cursor += 1; }
                }
                KeyCode::Char('r') | KeyCode::Enter => { app.trash_restore(); }
                KeyCode::Char('d') => { app.trash_purge_selected(); }
                KeyCode::Char('D') => { app.trash_empty(); }
                _ => {}
            }
            return false;
        }
    }
    let cfg = &app.cfg;
    use crate::config::key_matches;

    // All actions now use key_matches() which supports comma-separated multi-bindings
    // and both character keys and special keys (Up, Down, Enter, etc.)
    if key_matches(&cfg.key_nav_up,    key, mods) { app.tab_mut().move_cursor(-1); return false; }
    if key_matches(&cfg.key_nav_down,  key, mods) { app.tab_mut().move_cursor(1);  return false; }
    if key_matches(&cfg.key_nav_left,  key, mods) { app.tab_mut().leave();          return false; }
    if key_matches(&cfg.key_nav_right, key, mods) { app.open_current();             return false; }
    if key_matches(&cfg.key_page_up,   key, mods) { app.tab_mut().move_cursor(-10); return false; }
    if key_matches(&cfg.key_page_down, key, mods) { app.tab_mut().move_cursor(10);  return false; }
    if key_matches(&cfg.key_first,     key, mods) { app.tab_mut().state.select(Some(0)); return false; }
    if key_matches(&cfg.key_last,      key, mods) {
        let n = app.tab().visible().len();
        if n > 0 { app.tab_mut().state.select(Some(n - 1)); }
        return false;
    }

    match key {
        KeyCode::Esc => return true,
        _ => {}
    }

    if key_matches(&cfg.key_select,         key, mods) { app.tab_mut().toggle_select(); }
    else if key_matches(&cfg.key_select_all, key, mods) { app.tab_mut().select_all(); }
    else if key_matches(&cfg.key_select_all_alt, key, mods) { app.tab_mut().select_all(); }
    else if key_matches(&cfg.key_deselect,   key, mods) { app.tab_mut().deselect_all(); }
    else if key_matches(&cfg.key_copy,       key, mods) {
        if app.tab().selected.is_empty() { app.msg(app.lang.msg_select_first, true); }
        else { app.yank_files(false); }
    }
    else if key_matches(&cfg.key_cut,        key, mods) {
        if app.tab().selected.is_empty() { app.msg(app.lang.msg_select_first, true); }
        else { app.yank_files(true); }
    }
    else if key_matches(&cfg.key_paste,      key, mods) { app.paste_files(); }
    else if key_matches(&cfg.key_delete,     key, mods) {
        if app.tab().selected.is_empty() { app.msg(app.lang.msg_select_first, true); }
        else { app.mode = InputMode::Confirm; }
    }
    else if key_matches(&cfg.key_trash,      key, mods) { app.trash_files(); }
    else if key_matches(&cfg.key_rename,     key, mods) {
        if let Some(p) = app.tab().current().cloned() {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            app.input_buf = name.clone(); app.input_cursor = name.len(); app.mode = InputMode::Rename(name);
        }
    }
    else if key_matches(&cfg.key_new_file,      key, mods) { app.input_buf.clear(); app.input_cursor = 0; app.mode = InputMode::NewFile; }
    else if key_matches(&cfg.key_new_dir,       key, mods) { app.input_buf.clear(); app.input_cursor = 0; app.mode = InputMode::NewDir; }
    else if key_matches(&cfg.key_search,        key, mods) { app.open_fuzzy(); }
    else if key_matches(&cfg.key_toggle_hidden, key, mods) {
        let h = !app.tab().show_hidden;
        app.tab_mut().show_hidden = h; app.tab_mut().refresh();
        app.msg(if h { app.lang.msg_hidden_shown } else { app.lang.msg_hidden_hidden }, false);
    }
    else if key_matches(&cfg.key_cycle_tab,     key, mods) { app.tab_idx = (app.tab_idx + 1) % app.tabs.len(); }
    else if key_matches(&cfg.key_new_tab,       key, mods) { app.new_tab(); }
    else if key_matches(&cfg.key_close_tab,     key, mods) { app.close_tab(); }
    else if key_matches(&cfg.key_open_with,     key, mods) { app.open_with_menu(); }
    else if key_matches(&cfg.key_settings,      key, mods) { app.mode = InputMode::Settings; }
    else if key_matches(&cfg.key_help,          key, mods) { app.mode = InputMode::Help; }
    else if key_matches(&cfg.key_drives,        key, mods) { app.open_drive_manager(); }
    else if key_matches(&cfg.key_trash_browser, key, mods) { app.open_trash_browser(); }
    else if key_matches(&cfg.key_quit,          key, mods) { return true; }

    false
}

pub fn handle_settings_key(app: &mut App, key: KeyCode, _mods: KeyModifiers) -> bool {
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
        KeyCode::Up   => {
            let items = SettingsState::section_items(&app.settings.section);
            loop {
                if app.settings.cursor == 0 { break; }
                app.settings.cursor -= 1;
                let (k, _) = items[app.settings.cursor];
                if !k.starts_with("fixed_header_") { break; }
            }
        }
        KeyCode::Down => {
            let items = SettingsState::section_items(&app.settings.section);
            let m = items.len().saturating_sub(1);
            loop {
                if app.settings.cursor >= m { break; }
                app.settings.cursor += 1;
                let (k, _) = items[app.settings.cursor];
                if !k.starts_with("fixed_header_") { break; }
            }
        }
        KeyCode::Left => {
            app.settings.section = match app.settings.section {
                SettingsSection::Behaviour=>SettingsSection::About,     SettingsSection::Appearance=>SettingsSection::Behaviour,
                SettingsSection::Openers=>SettingsSection::Appearance,  SettingsSection::Keybinds=>SettingsSection::Openers,
                SettingsSection::About=>SettingsSection::Keybinds,
            }; app.settings.cursor=0;
        }
        KeyCode::Right => {
            app.settings.section = match app.settings.section {
                SettingsSection::Behaviour=>SettingsSection::Appearance, SettingsSection::Appearance=>SettingsSection::Openers,
                SettingsSection::Openers=>SettingsSection::Keybinds,    SettingsSection::Keybinds=>SettingsSection::About,
                SettingsSection::About=>SettingsSection::Behaviour,
            }; app.settings.cursor=0;
        }
        KeyCode::Enter => {
            let items = SettingsState::section_items(&app.settings.section);
            let (k, label) = items[app.settings.cursor];
            if k.starts_with("fixed_") {
                // Fixed — informational only
            } else if k.starts_with("key_") && app.settings.section == SettingsSection::Keybinds {
                // Open keybind editor overlay
                app.keybind_key         = k.to_string();
                app.keybind_label       = label.to_string();
                app.keybind_menu_cursor = 0;
                app.mode                = InputMode::KeybindMenu;
            } else if let Some(opts) = SettingsState::dropdown_options(k) {
                let cur_val = SettingsState::get_value(k, &app.cfg);
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
                    app.theme = Theme::load(&app.cfg.theme);
                    app.icons = IconData::load(&app.cfg.icon_set);
                    // Reload language — must happen before msg so the message
                    // itself uses the newly selected language
                    app.lang = crate::lang::load(&app.cfg.language);
                    app.msg(app.lang.msg_settings_saved, false);
                }
                Err(e) => { app.msg(&format!("{}: {}", app.lang.msg_save_error, e), true); }
            }
        }
        _ => {}
    }
    false
}

pub fn handle_fuzzy_key(app: &mut App, key: KeyCode, _mods: KeyModifiers) {
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

pub fn handle_runargs_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.mode = InputMode::Normal;
            app.input_buf.clear();
        }
        KeyCode::Tab => {
            // Switch focus between start and end fields, saving current input
            if let InputMode::RunArgs(_, ref mut focus_end, ref mut start, ref mut end) = app.mode {
                if *focus_end {
                    *end   = app.input_buf.clone();
                    app.input_buf = start.clone();
                } else {
                    *start = app.input_buf.clone();
                    app.input_buf = end.clone();
                }
                *focus_end = !*focus_end;
            }
        }
        KeyCode::Enter => {
            // Save current field before launching
            if let InputMode::RunArgs(_, focus_end, ref mut start_a, ref mut end_a) = app.mode {
                if focus_end { *end_a   = app.input_buf.clone(); }
                else         { *start_a = app.input_buf.clone(); }
            }
            let (path, _fe, start_args, end_args) = match std::mem::replace(&mut app.mode, InputMode::Normal) {
                InputMode::RunArgs(p, fe, sa, ea) => (p, fe, sa, ea),
                _ => return,
            };
            app.input_buf.clear();

            let cfg  = app.cfg.clone();
            let term = cfg.opener_terminal.clone();
            let ext  = path.extension().and_then(|e| e.to_str())
                .map(|s| s.to_lowercase()).unwrap_or_default();
            let is_script = matches!(ext.as_str(), "sh"|"bash"|"zsh"|"fish");
            let path_escaped = path.to_string_lossy().replace("'", "'\''");

            // is_executable() is the shared helper defined in app.rs — same check used in open_current.
            let base_cmd = if is_script && !crate::app::is_executable(&path) {
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

            // Build: [start_args] base_cmd [end_args]
            let mut parts: Vec<String> = Vec::new();
            if !start_args.is_empty() { parts.push(start_args.clone()); }
            parts.push(base_cmd.clone());
            if !end_args.is_empty() { parts.push(end_args.clone()); }
            let run_cmd = parts.join(" ");

            // Show command preview only if args were provided
            let has_args = !start_args.is_empty() || !end_args.is_empty();
            let full_cmd = if has_args {
                format!("echo '$ {}'; echo; {}; echo; echo '-- Press Enter to close --'; read _",
                    run_cmd, run_cmd)
            } else {
                format!("{}; echo; echo '-- Press Enter to close --'; read _", run_cmd)
            };

            let work_dir = path.parent().unwrap_or(std::path::Path::new("/")).to_path_buf();
            let _ = Command::new(&term)
                .args(["--", "sh", "-c", &full_cmd])
                .current_dir(&work_dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .stdin(std::process::Stdio::null())
                .spawn();
            app.msg(&format!("{} {} {}",
                app.lang.msg_launching,
                path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                app.lang.msg_in_terminal), false);
        }
        KeyCode::Backspace => { app.input_buf.pop(); }
        KeyCode::Char(c)   => app.input_buf.push(c),
        _ => {}
    }
}

pub fn handle_input_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.mode = InputMode::Normal;
            app.input_buf.clear();
            app.input_cursor = 0;
        }
        KeyCode::Enter => {
            let val  = app.input_buf.clone();
            let mode = std::mem::replace(&mut app.mode, InputMode::Normal);
            app.input_buf.clear();
            app.input_cursor = 0;
            if val.is_empty() { return; }
            match mode {
                InputMode::Rename(orig) if val != orig => {
                    let src = app.tab().cwd.join(&orig);
                    let dst = app.tab().cwd.join(&val);
                    match fs::rename(&src, &dst) {
                        Ok(_)  => { app.tab_mut().refresh(); app.msg(&format!("Renamed \u{2192} {}", val), false); }
                        Err(e) => app.msg(&e.to_string(), true),
                    }
                }
                InputMode::NewFile => {
                    let t = app.tab().cwd.join(&val);
                    match fs::File::create(&t) {
                        Ok(_)  => { app.tab_mut().refresh(); app.msg(&format!("Created {}", val), false); }
                        Err(e) => app.msg(&e.to_string(), true),
                    }
                }
                InputMode::NewDir => {
                    let t = app.tab().cwd.join(&val);
                    match fs::create_dir_all(&t) {
                        Ok(_)  => { app.tab_mut().refresh(); app.msg(&format!("Created dir {}", val), false); }
                        Err(e) => app.msg(&e.to_string(), true),
                    }
                }
                _ => {}
            }
        }
        KeyCode::Left => {
            // Move cursor left one character (unicode-safe)
            if app.input_cursor > 0 {
                app.input_cursor -= 1;
                while app.input_cursor > 0 && !app.input_buf.is_char_boundary(app.input_cursor) {
                    app.input_cursor -= 1;
                }
            }
        }
        KeyCode::Right => {
            // Move cursor right one character (unicode-safe)
            if app.input_cursor < app.input_buf.len() {
                app.input_cursor += 1;
                while app.input_cursor < app.input_buf.len() && !app.input_buf.is_char_boundary(app.input_cursor) {
                    app.input_cursor += 1;
                }
            }
        }
        KeyCode::Home => { app.input_cursor = 0; }
        KeyCode::End  => { app.input_cursor = app.input_buf.len(); }
        KeyCode::Backspace => {
            // Delete character before cursor
            if app.input_cursor > 0 {
                let mut pos = app.input_cursor - 1;
                while pos > 0 && !app.input_buf.is_char_boundary(pos) { pos -= 1; }
                app.input_buf.remove(pos);
                app.input_cursor = pos;
            }
        }
        KeyCode::Delete => {
            // Delete character at cursor
            if app.input_cursor < app.input_buf.len() {
                app.input_buf.remove(app.input_cursor);
            }
        }
        KeyCode::Char(c) => {
            app.input_buf.insert(app.input_cursor, c);
            app.input_cursor += c.len_utf8();
        }
        _ => {}
    }
}

// Handle Open With overlay keypresses.
pub fn handle_openwith_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => { app.mode = InputMode::Normal; }
        KeyCode::Up => {
            if let InputMode::OpenWith(_, _, ref mut cur) = app.mode {
                if *cur > 0 { *cur -= 1; }
            }
        }
        KeyCode::Down => {
            if let InputMode::OpenWith(_, ref entries, ref mut cur) = app.mode {
                if *cur + 1 < entries.len() { *cur += 1; }
            }
        }
        KeyCode::Enter => {
            if let InputMode::OpenWith(path, entries, cur) =
                std::mem::replace(&mut app.mode, InputMode::Normal)
            {
                if let Some((_, cmd)) = entries.get(cur) {
                    if cmd == "__custom__" {
                        // Switch to custom command input
                        app.input_buf.clear();
                        app.mode = InputMode::OpenWithCustom(path);
                    } else {
                        let cmd = cmd.clone();
                        // Terminal apps (editor) need the TUI to suspend and
                        // hand over the terminal — use nvim_path mechanism.
                        if cmd == app.cfg.opener_editor {
                            app.msg(&format!("Opening with {}", cmd), false);
                            app.nvim_path = Some(path);
                        } else {
                            // GUI apps and xdg-open: spawn directly (not via sh -c)
                            // so they inherit the full session env (DISPLAY, WAYLAND_DISPLAY…).
                            spawn_gui_open(&cmd, &path);
                            app.msg(&format!("Opening with {}", cmd), false);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn handle_openwith_custom_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.mode = InputMode::Normal;
            app.input_buf.clear();
        }
        KeyCode::Enter => {
            let cmd = app.input_buf.clone();
            let path = match std::mem::replace(&mut app.mode, InputMode::Normal) {
                InputMode::OpenWithCustom(p) => p,
                _ => return,
            };
            app.input_buf.clear();
            if cmd.trim().is_empty() { return; }
            let cmd = cmd.trim().to_string();
            // If the custom command matches the configured editor, use the
            // proper terminal handoff instead of spawning silently.
            if cmd == app.cfg.opener_editor {
                app.msg(&format!("Opening with {}", cmd), false);
                app.nvim_path = Some(path);
            } else {
                spawn_gui_open(&cmd, &path);
                app.msg(&format!("Opening with {}", cmd), false);
            }
        }
        KeyCode::Backspace => { app.input_buf.pop(); }
        KeyCode::Char(c)   => app.input_buf.push(c),
        _ => {}
    }
}

pub fn handle_setup_key(app: &mut App, key: KeyCode) {
    let candidates = app.setup_step.candidates();

    // If typing a custom value
    if app.setup_typing {
        match key {
            KeyCode::Esc => {
                app.setup_typing = false;
                app.setup_custom.clear();
            }
            KeyCode::Backspace => { app.setup_custom.pop(); }
            KeyCode::Enter => {
                if !app.setup_custom.trim().is_empty() {
                    apply_setup_value(app, app.setup_custom.trim().to_string());
                    app.setup_custom.clear();
                    app.setup_typing = false;
                }
            }
            KeyCode::Char(c) => { app.setup_custom.push(c); }
            _ => {}
        }
        return;
    }

    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.setup_cursor > 0 { app.setup_cursor -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.setup_cursor + 1 < candidates.len() { app.setup_cursor += 1; }
        }
        KeyCode::BackTab | KeyCode::Left => {
            app.setup_step   = app.setup_step.prev();
            app.setup_cursor = 0;
        }
        KeyCode::Enter | KeyCode::Right => {
            if let Some(&chosen) = candidates.get(app.setup_cursor) {
                if chosen == "custom" {
                    app.setup_typing = true;
                    app.setup_custom.clear();
                } else {
                    apply_setup_value(app, chosen.to_string());
                }
            }
        }
        KeyCode::Esc => {
            // Skip remaining setup and go straight to the file manager
            finish_setup(app);
        }
        _ => {}
    }
}

fn apply_setup_value(app: &mut App, value: String) {
    match app.setup_step {
        SetupStep::Language    => {
            app.cfg.language = value.clone();
            app.lang = crate::lang::load(&value);
        }
        SetupStep::Browser     => app.cfg.opener_browser  = value,
        SetupStep::ImageViewer => app.cfg.opener_image    = value,
        SetupStep::VideoPlayer => app.cfg.opener_video    = value,
        SetupStep::AudioPlayer => app.cfg.opener_audio    = value,
        SetupStep::DocViewer   => app.cfg.opener_doc      = value,
        SetupStep::Editor      => app.cfg.opener_editor   = value,
        SetupStep::Terminal    => app.cfg.opener_terminal = value,
        SetupStep::Done        => {}
    }
    app.setup_step = app.setup_step.next();
    app.setup_cursor = 0;

    if app.setup_step == SetupStep::Done {
        finish_setup(app);
    }
}

fn finish_setup(app: &mut App) {
    app.cfg.first_run = false;
    let _ = app.cfg.save();
    app.mode = InputMode::Normal;
}

// ── Keybind editor ────────────────────────────────────────────────────────────
// Storage format:  "Up/k/Ctrl+C"
//   /  separates independent bindings (each works alone)
//   +  joins keys in a combo (all must be pressed together)

const KEYBIND_MENU_OPTIONS: &[&str] = &[
    "Add one key binding",
    "Add combination  (e.g. Ctrl+K)",
    "Remove a binding",
    "Reset to default",
];

fn handle_keybind_menu(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => { app.mode = InputMode::Settings; }
        KeyCode::Up   => { if app.keybind_menu_cursor > 0 { app.keybind_menu_cursor -= 1; } }
        KeyCode::Down => {
            if app.keybind_menu_cursor + 1 < KEYBIND_MENU_OPTIONS.len() {
                app.keybind_menu_cursor += 1;
            }
        }
        KeyCode::Enter => {
            match app.keybind_menu_cursor {
                0 => { // Add isolated — one keypress, appended with /
                    app.keybind_capture_mode = 0;
                    app.mode = InputMode::KeyCapture;
                }
                1 => { // Add combination — two keypresses, joined with +
                    app.keybind_capture_mode = 1;
                    app.keybind_combo_first.clear();
                    app.mode = InputMode::KeyCapture;
                }
                2 => { // Remove — show list of current bindings to pick from
                    let k = app.keybind_key.clone();
                    let current = crate::config::SettingsState::get_value(&k, &app.cfg);
                    let bindings: Vec<String> = current.split('/').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    if bindings.is_empty() {
                        app.msg("No bindings to remove", true);
                    } else {
                        app.keybind_remove_cursor = 0;
                        app.mode = InputMode::KeybindRemove;
                    }
                }
                3 => { // Reset to default
                    reset_keybind_default(app);
                    app.settings.dirty = true;
                    app.mode = InputMode::Settings;
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn handle_key_capture(app: &mut App, key: KeyCode, mods: crossterm::event::KeyModifiers) {
    if key == KeyCode::Esc {
        app.mode = InputMode::KeybindMenu;
        return;
    }
    let Some(captured) = crate::config::keycode_to_string(key, mods) else { return };

    match app.keybind_capture_mode {
        0 => {
            // Add isolated binding — append with /
            let k = app.keybind_key.clone();
            let current = crate::config::SettingsState::get_value(&k, &app.cfg);
            let new_val = if current.is_empty() {
                captured.clone()
            } else {
                format!("{}/{}", current, captured)
            };
            crate::config::SettingsState::set_value(&k, &new_val, &mut app.cfg);
            app.settings.dirty = true;
            app.msg(&format!("Added: {}", captured), false);
            app.mode = InputMode::Settings;
        }
        1 => {
            // Combo step 1 — capture first key
            app.keybind_combo_first = captured;
            app.keybind_capture_mode = 2; // wait for second key
        }
        2 => {
            // Combo step 2 — combine with + and append with /
            let combo = format!("{}+{}", app.keybind_combo_first, captured);
            let k = app.keybind_key.clone();
            let current = crate::config::SettingsState::get_value(&k, &app.cfg);
            let new_val = if current.is_empty() {
                combo.clone()
            } else {
                format!("{}/{}", current, combo)
            };
            crate::config::SettingsState::set_value(&k, &new_val, &mut app.cfg);
            app.settings.dirty = true;
            app.msg(&format!("Added combo: {}", combo), false);
            app.mode = InputMode::Settings;
        }
        _ => {}
    }
}

pub fn handle_keybind_remove(app: &mut App, key: KeyCode) {
    let k = app.keybind_key.clone();
    let current = crate::config::SettingsState::get_value(&k, &app.cfg);
    let bindings: Vec<String> = current.split('/').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

    match key {
        KeyCode::Esc => { app.mode = InputMode::KeybindMenu; }
        KeyCode::Up   => { if app.keybind_remove_cursor > 0 { app.keybind_remove_cursor -= 1; } }
        KeyCode::Down => {
            if app.keybind_remove_cursor + 1 < bindings.len() { app.keybind_remove_cursor += 1; }
        }
        KeyCode::Enter => {
            if app.keybind_remove_cursor < bindings.len() {
                let removed = bindings[app.keybind_remove_cursor].clone();
                let new_bindings: Vec<&String> = bindings.iter().enumerate()
                    .filter(|(i, _)| *i != app.keybind_remove_cursor)
                    .map(|(_, b)| b)
                    .collect();
                let new_val = new_bindings.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("/");
                crate::config::SettingsState::set_value(&k, &new_val, &mut app.cfg);
                app.settings.dirty = true;
                app.msg(&format!("Removed: {}", removed), false);
                app.mode = InputMode::Settings;
            }
        }
        _ => {}
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Spawn a GUI application (or xdg-open) with `path` as its argument.
/// The command string may contain a leading argument separated by a space,
/// e.g. `"java -jar"` splits into binary=`java`, first_arg=`-jar`.
/// Spawned without stdin/stdout/stderr so it does not touch the TUI.
/// Must be launched directly (not via `sh -c`) so the child process inherits
/// the full desktop session environment: DISPLAY, WAYLAND_DISPLAY, DBUS_SESSION_BUS_ADDRESS…
fn spawn_gui_open(cmd: &str, path: &std::path::Path) {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let mut c = std::process::Command::new(parts[0]);
    if parts.len() > 1 { c.arg(parts[1]); }
    c.arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = c.spawn();
}

fn reset_keybind_default(app: &mut App) {
    let default_cfg = crate::config::Config::default();
    let k = app.keybind_key.as_str();
    let default_val = crate::config::SettingsState::get_value(k, &default_cfg);
    crate::config::SettingsState::set_value(k, &default_val, &mut app.cfg);
    app.msg(&format!("Reset to default: {}", default_val), false);
}
