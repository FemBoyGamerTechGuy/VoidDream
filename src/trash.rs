// trash.rs — XDG Trash Standard implementation
// Spec: https://specifications.freedesktop.org/trash-spec/trashspec-latest.html
//
// ~/.local/share/Trash/
//   files/   — actual deleted files/dirs
//   info/    — .trashinfo metadata (original path + deletion date)

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

// ── Trash location ────────────────────────────────────────────────────────────

pub fn trash_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let xdg  = std::env::var("XDG_DATA_HOME").unwrap_or_default();
    let base  = if !xdg.is_empty() { PathBuf::from(xdg) }
                else               { PathBuf::from(&home).join(".local").join("share") };
    base.join("Trash")
}

pub fn trash_files_dir()  -> PathBuf { trash_dir().join("files") }
pub fn trash_info_dir()   -> PathBuf { trash_dir().join("info") }

pub fn ensure_trash_dirs() -> std::io::Result<()> {
    fs::create_dir_all(trash_files_dir())?;
    fs::create_dir_all(trash_info_dir())?;
    Ok(())
}

// ── TrashEntry — a single item in the trash ───────────────────────────────────

#[derive(Clone)]
pub struct TrashEntry {
    /// Name as stored inside ~/.local/share/Trash/files/
    pub trash_name:    String,
    /// Original absolute path before deletion
    pub original_path: PathBuf,
    /// ISO 8601 deletion date string from .trashinfo
    pub deletion_date: String,
    /// True if it was a directory
    pub is_dir:        bool,
}

// ── Move to trash ─────────────────────────────────────────────────────────────

/// Move `src` to the XDG trash. Returns an error string on failure.
pub fn move_to_trash(src: &Path) -> Result<(), String> {
    ensure_trash_dirs().map_err(|e| e.to_string())?;

    let orig_name = src.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Find a unique name in ~/.local/share/Trash/files/
    let trash_name = unique_trash_name(&orig_name);
    let dest       = trash_files_dir().join(&trash_name);
    let info_path  = trash_info_dir().join(format!("{}.trashinfo", trash_name));

    // Write .trashinfo BEFORE moving so if the move fails, no orphan info exists
    let abs_src = src.canonicalize()
        .unwrap_or_else(|_| src.to_path_buf());
    let date_str = iso8601_now();
    let info_content = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        abs_src.display(),
        date_str,
    );
    fs::write(&info_path, &info_content).map_err(|e| e.to_string())?;

    // Try atomic rename first (same filesystem)
    let res = fs::rename(src, &dest).or_else(|_| {
        // Cross-device: copy then delete
        if src.is_dir() {
            copy_dir(src, &dest)?;
            fs::remove_dir_all(src)
        } else {
            fs::copy(src, &dest).map(|_| ())?;
            fs::remove_file(src)
        }
    });

    if let Err(e) = res {
        // Clean up orphan .trashinfo on failure
        let _ = fs::remove_file(&info_path);
        return Err(e.to_string());
    }

    Ok(())
}

// ── List trash ────────────────────────────────────────────────────────────────

pub fn list_trash() -> Vec<TrashEntry> {
    let info_dir   = trash_info_dir();
    let files_dir  = trash_files_dir();
    let mut entries = Vec::new();

    let rd = match fs::read_dir(&info_dir) { Ok(r) => r, Err(_) => return entries };
    for entry in rd.filter_map(|e| e.ok()) {
        let info_path = entry.path();
        if info_path.extension().and_then(|e| e.to_str()) != Some("trashinfo") { continue; }

        let trash_name = info_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let content = match fs::read_to_string(&info_path) { Ok(c) => c, Err(_) => continue };
        let original_path = parse_trashinfo_path(&content).unwrap_or_else(|| PathBuf::from(&trash_name));
        let deletion_date = parse_trashinfo_date(&content).unwrap_or_else(|| "Unknown".into());

        let trash_file = files_dir.join(&trash_name);
        let is_dir     = trash_file.is_dir();

        entries.push(TrashEntry { trash_name, original_path, deletion_date, is_dir });
    }

    // Sort by deletion date, newest first
    entries.sort_by(|a, b| b.deletion_date.cmp(&a.deletion_date));
    entries
}

// ── Restore from trash ────────────────────────────────────────────────────────

pub fn restore_entry(entry: &TrashEntry) -> Result<(), String> {
    let src  = trash_files_dir().join(&entry.trash_name);
    let dest = &entry.original_path;

    // Recreate parent dirs if they no longer exist
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Refuse to overwrite existing file at destination
    if dest.exists() {
        return Err(format!("Destination already exists: {}", dest.display()));
    }

    fs::rename(&src, dest).or_else(|_| {
        if src.is_dir() {
            copy_dir(&src, dest)?;
            fs::remove_dir_all(&src)
        } else {
            fs::copy(&src, dest).map(|_| ())?;
            fs::remove_file(&src)
        }
    }).map_err(|e| e.to_string())?;

    // Remove .trashinfo
    let info = trash_info_dir().join(format!("{}.trashinfo", entry.trash_name));
    let _ = fs::remove_file(info);

    Ok(())
}

// ── Permanently delete from trash ─────────────────────────────────────────────

pub fn purge_entry(entry: &TrashEntry) -> Result<(), String> {
    let src = trash_files_dir().join(&entry.trash_name);
    if src.is_dir() { fs::remove_dir_all(&src).map_err(|e| e.to_string())?; }
    else            { fs::remove_file(&src).map_err(|e| e.to_string())?; }

    let info = trash_info_dir().join(format!("{}.trashinfo", entry.trash_name));
    let _ = fs::remove_file(info);
    Ok(())
}

/// Permanently delete every item in the trash.
pub fn empty_trash() -> Result<(), String> {
    for entry in list_trash() {
        purge_entry(&entry)?;
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn unique_trash_name(name: &str) -> String {
    let files_dir = trash_files_dir();
    if !files_dir.join(name).exists() { return name.to_string(); }
    // Split stem and extension
    let stem = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext  = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("");
    for i in 1..=9999u32 {
        let candidate = if ext.is_empty() {
            format!("{}.{}", stem, i)
        } else {
            format!("{}.{}.{}", stem, i, ext)
        };
        if !files_dir.join(&candidate).exists() { return candidate; }
    }
    format!("{}.{}", name, iso8601_now().replace(':', "-"))
}

fn iso8601_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Convert to naive local datetime (same helper as types.rs)
    let (y, mo, d, h, mi) = crate::types::secs_to_datetime(
        (secs as i64 + crate::types::local_tz_offset_secs()) as u64
    );
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, mo, d, h, mi, secs % 60)
}

fn parse_trashinfo_path(content: &str) -> Option<PathBuf> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Path=") {
            return Some(PathBuf::from(rest.trim()));
        }
    }
    None
}

fn parse_trashinfo_date(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("DeletionDate=") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty    = entry.file_type()?;
        let dest  = dst.join(entry.file_name());
        if ty.is_dir() { copy_dir(&entry.path(), &dest)?; }
        else           { fs::copy(entry.path(), &dest)?; }
    }
    Ok(())
}
