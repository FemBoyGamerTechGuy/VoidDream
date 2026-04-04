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
                KeyCode::Up   | KeyCode::Char('k') => {
                    if app.drive_cursor > 0 { app.drive_cursor -= 1; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if app.drive_cursor + 1 < app.drive_devices.len() { app.drive_cursor += 1; }
                }
                KeyCode::Char('m') => { app.drive_mount(); }
                KeyCode::Char('u') => { app.drive_unmount(); }
                KeyCode::Char('r') => { app.drive_devices = crate::drives::list_devices(); }
                KeyCode::Enter     => { app.drive_navigate(); }
                _ => {}
            }
            return false;
        }
        InputMode::FuzzySearch => { handle_fuzzy_key(app, key, mods); return false; }
        InputMode::Rename(_)|InputMode::NewFile|InputMode::NewDir => { handle_input_key(app, key); return false; }
        InputMode::RunArgs(..) => { handle_runargs_key(app, key); return false; }
        InputMode::OpenWith(..) => { handle_openwith_key(app, key); return false; }
        InputMode::OpenWithCustom(..) => { handle_openwith_custom_key(app, key); return false; }
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
    }
    let cfg = &app.cfg;

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
                if app.tab().selected.is_empty() { app.msg(app.lang.msg_select_first, true); }
                else { app.yank_files(false); }
            } else if s == cfg.key_cut {
                if app.tab().selected.is_empty() { app.msg(app.lang.msg_select_first, true); }
                else { app.yank_files(true); }
            } else if s == cfg.key_paste {
                app.paste_files();
            } else if s == cfg.key_delete {
                if app.tab().selected.is_empty() { app.msg(app.lang.msg_select_first, true); }
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
                app.msg(if h { app.lang.msg_hidden_shown } else { app.lang.msg_hidden_hidden }, false);
            } else if s == cfg.key_cycle_tab {
                app.tab_idx = (app.tab_idx + 1) % app.tabs.len();
            } else if s == cfg.key_new_tab {
                app.new_tab();
            } else if s == cfg.key_close_tab {
                app.close_tab();
            } else if s == cfg.key_quit {
                return true;
            } else if c == 'k' {
                // Open-with context menu
                app.open_with_menu();
            } else if c == ':' {
                app.mode = InputMode::Settings;
            } else if c == '?' {
                app.mode = InputMode::Help;
            } else if c == 'D' {
                app.open_drive_manager();
            }
        }
        KeyCode::Esc => return true,
        _ => {}
    }
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
        KeyCode::Up   => { if app.settings.cursor > 0 { app.settings.cursor -= 1; } }
        KeyCode::Down => {
            let m = SettingsState::section_items(&app.settings.section).len().saturating_sub(1);
            if app.settings.cursor < m { app.settings.cursor += 1; }
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
            let (k, _) = items[app.settings.cursor];
            if k.starts_with("fixed_") {
                // Fixed keys are informational — not configurable
            } else if let Some(opts) = SettingsState::dropdown_options(k) {
                // Open dropdown — pre-select current value
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
            let is_exec = {
                #[cfg(unix)] {
                    use std::os::unix::fs::PermissionsExt;
                    path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
                }
                #[cfg(not(unix))] { false }
            };
            let path_escaped = path.to_string_lossy().replace("'", "'\''");

            let base_cmd = if is_script && !is_exec {
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

/// Spawn a shell command silently (no stdin/stdout/stderr).
fn spawn_sh_silent(cmd: &str) {
    let _ = std::process::Command::new("sh")
        .args(["-c", cmd])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

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
                        let path_escaped = path.to_string_lossy().replace("'", "'\''");
                        let full_cmd = if cmd.contains(' ') {
                            format!("{} '{}'", cmd, path_escaped)
                        } else {
                            format!("'{}' '{}'", cmd, path_escaped)
                        };
                        spawn_sh_silent(&full_cmd);
                        app.msg(&format!("Opening with {}", cmd), false);
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
            let path_escaped = path.to_string_lossy().replace("'", "'\''");
            let full_cmd = format!("{} '{}'", cmd.trim(), path_escaped);
            spawn_sh_silent(&full_cmd);
            app.msg(&format!("Opening with {}", cmd.trim()), false);
        }
        KeyCode::Backspace => { app.input_buf.pop(); }
        KeyCode::Char(c)   => app.input_buf.push(c),
        _ => {}
    }
}
