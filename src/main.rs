// main.rs — VoidDream entry point
mod config;
mod types;
mod lang;
mod app;
mod extract;
mod drives;
mod trash;
mod ui;
mod keys;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{fs, io::{self, Write}, time::Duration, path::PathBuf, process::Command};
use config::Config;
use types::dirs_home;
use app::App;
use ui::ui;
use keys::handle_key;

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
