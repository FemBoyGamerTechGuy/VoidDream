// extract.rs — Native archive extraction engine
//
// Natively supported (no system binaries):
//   zip, tar, tar.gz, tar.bz2, tar.xz, tar.zst, gz, bz2, xz, zst (single-file)
// Requires unrar binary:
//   rar — proprietary format, no free Rust decoder exists
use std::{
    path::Path,
    process::Command,
    sync::mpsc,
    time::Instant,
};
use crate::types::ExtractionProgress;

/// Entry point called from App::open_current.
/// Spawns a background thread and sends progress updates on a channel.
pub fn start_extraction(
    path: &Path,
    tx: mpsc::Sender<ExtractionProgress>,
) -> (ExtractionProgress, u64) {
    let dst      = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let src_s    = path.to_string_lossy().into_owned();
    let dst_s    = dst.to_string_lossy().into_owned();
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let ext      = path.extension()
        .and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
    let name_lower = path.file_name()
        .and_then(|n| n.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();

    let total_bytes = archive_total_size(&src_s, &ext, &name_lower);
    let initial = ExtractionProgress {
        filename: filename.clone(), current: 0, total: total_bytes,
        done: false, error: None, start_time: Instant::now(), pid: None,
    };

    let fname = filename.clone();
    std::thread::spawn(move || {
        let result = run_extraction_with_progress(
            &src_s, &dst_s, &ext, &name_lower, total_bytes, &tx, &fname,
        );
        let _ = tx.send(ExtractionProgress {
            filename: fname, current: total_bytes.max(1), total: total_bytes.max(1),
            done: true, error: result.err().map(|e| e.to_string()),
            start_time: Instant::now(), pid: None,
        });
    });

    (initial, total_bytes)
}

// ── Size estimation ───────────────────────────────────────────────────────────

fn archive_total_size(src_s: &str, ext: &str, name_lower: &str) -> u64 {
    let path = Path::new(src_s);

    if ext == "zip" {
        if let Ok(f) = std::fs::File::open(path) {
            if let Ok(mut za) = zip::ZipArchive::new(f) {
                let mut total: u64 = 0;
                for i in 0..za.len() {
                    if let Ok(entry) = za.by_index(i) {
                        total += entry.size();
                    }
                }
                if total > 0 { return total; }
            }
        }
        return 0;
    }

    if name_lower.ends_with(".tar.gz")  || name_lower.ends_with(".tgz")
        || name_lower.ends_with(".tar.bz2") || name_lower.ends_with(".tbz2")
        || name_lower.ends_with(".tar.xz")  || name_lower.ends_with(".tar.zst")
        || ext == "tar"
    {
        return tar_total_size(path, name_lower);
    }

    if matches!(ext, "gz" | "bz2" | "xz" | "zst") { return 0; }

    if ext == "rar" {
        if let Ok(out) = Command::new("unrar").args(["lt", src_s]).output() {
            let total: u64 = String::from_utf8_lossy(&out.stdout).lines()
                .filter_map(|l| {
                    let t = l.trim();
                    if t.to_lowercase().starts_with("size:") {
                        t.split_whitespace().nth(1)?.parse::<u64>().ok()
                    } else { None }
                })
                .sum();
            if total > 0 { return total; }
        }
    }
    0
}

fn tar_total_size(path: &Path, name_lower: &str) -> u64 {
    use std::io::BufReader;
    let f = match std::fs::File::open(path) { Ok(f) => f, Err(_) => return 0 };
    let buf = BufReader::new(f);
    macro_rules! sum_tar {
        ($dec:expr) => {
            tar::Archive::new($dec).entries().ok()
                .map(|e| e.filter_map(|en| en.ok())
                          .map(|en| en.header().size().unwrap_or(0))
                          .sum())
                .unwrap_or(0)
        };
    }
    if name_lower.ends_with(".tar.gz")  || name_lower.ends_with(".tgz") {
        sum_tar!(flate2::read::GzDecoder::new(buf))
    } else if name_lower.ends_with(".tar.bz2") || name_lower.ends_with(".tbz2") {
        sum_tar!(bzip2::read::BzDecoder::new(buf))
    } else if name_lower.ends_with(".tar.xz") {
        sum_tar!(xz2::read::XzDecoder::new(buf))
    } else if name_lower.ends_with(".tar.zst") {
        match zstd::stream::read::Decoder::new(buf) {
            Ok(dec) => sum_tar!(dec),
            Err(_)  => 0,
        }
    } else {
        sum_tar!(buf)
    }
}

// ── Extraction dispatch ───────────────────────────────────────────────────────

fn run_extraction_with_progress(
    src_s: &str, dst_s: &str, ext: &str, name_lower: &str,
    total: u64, tx: &mpsc::Sender<ExtractionProgress>, fname: &str,
) -> std::io::Result<()> {
    let src = Path::new(src_s);
    let dst = Path::new(dst_s);

    let _ = tx.send(ExtractionProgress {
        filename: fname.to_string(), current: 0, total,
        done: false, error: None, start_time: Instant::now(), pid: None,
    });

    if ext == "zip" {
        return extract_zip(src, dst, total, tx, fname);
    }

    if name_lower.ends_with(".tar.gz")  || name_lower.ends_with(".tgz")
        || name_lower.ends_with(".tar.bz2") || name_lower.ends_with(".tbz2")
        || name_lower.ends_with(".tar.xz")  || name_lower.ends_with(".tar.zst")
        || ext == "tar"
    {
        return extract_tar(src, dst, name_lower, total, tx, fname);
    }

    macro_rules! decompress_single {
        ($dec:expr) => {{
            let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
            let mut out = std::fs::File::create(dst.join(stem))?;
            std::io::copy(&mut $dec, &mut out)?;
            return Ok(());
        }};
    }

    if ext == "gz"  { let f = std::fs::File::open(src)?; decompress_single!(flate2::read::GzDecoder::new(f)); }
    if ext == "bz2" { let f = std::fs::File::open(src)?; decompress_single!(bzip2::read::BzDecoder::new(f)); }
    if ext == "xz"  { let f = std::fs::File::open(src)?; decompress_single!(xz2::read::XzDecoder::new(f)); }
    if ext == "zst" {
        let f = std::fs::File::open(src)?;
        let mut dec = zstd::stream::read::Decoder::new(f)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        decompress_single!(dec);
    }

    if ext == "rar" {
        if Command::new("unrar").arg("-?")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status().is_err()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "RAR requires 'unrar' — install it (e.g. pacman -S unrar)",
            ));
        }
        return extract_rar_via_unrar(src_s, dst_s, total, tx, fname);
    }

    Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,
        format!("No extractor for .{}", ext)))
}

// ── Format implementations ────────────────────────────────────────────────────

fn extract_zip(
    src: &Path, dst: &Path,
    total: u64, tx: &mpsc::Sender<ExtractionProgress>, fname: &str,
) -> std::io::Result<()> {
    let f = std::fs::File::open(src)?;
    let mut za = zip::ZipArchive::new(f)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let file_count = za.len() as u64;
    let mut done: u64 = 0;

    for i in 0..za.len() {
        let mut entry = za.by_index(i)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // zip-slip guard: skip entries with absolute paths or ".." components.
        // Use enclosed_name() which returns None for unsafe paths.
        let safe_path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None    => continue,
        };
        let out_path = dst.join(&safe_path);

        // A directory entry either has is_dir()==true OR has a trailing slash
        // in its name (some archivers don't set the dir bit correctly).
        let is_dir = entry.is_dir()
            || entry.name().ends_with('/')
            || entry.name().ends_with("\\");

        if is_dir {
            std::fs::create_dir_all(&out_path)?;
        } else {
            // Ensure parent directory exists before creating file
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }

        done += 1;
        let current = if total > 0 && file_count > 0 {
            (done * total / file_count).min(total)
        } else { done };
        let _ = tx.send(ExtractionProgress {
            filename: fname.to_string(), current, total,
            done: false, error: None, start_time: Instant::now(), pid: None,
        });
    }
    Ok(())
}

fn extract_tar(
    src: &Path, dst: &Path, name_lower: &str,
    total: u64, tx: &mpsc::Sender<ExtractionProgress>, fname: &str,
) -> std::io::Result<()> {
    use std::io::BufReader;
    let f   = std::fs::File::open(src)?;
    let buf = BufReader::new(f);

    macro_rules! unpack {
        ($dec:expr) => {{
            let mut archive = tar::Archive::new($dec);
            // Don't try to restore original ownership/permissions — we're extracting
            // as the current user, so preserving root-owned file permissions causes EPERM.
            archive.set_preserve_permissions(false);
            archive.set_preserve_ownerships(false);
            archive.set_unpack_xattrs(false);
            let mut done_bytes: u64 = 0;
            let mut done_files: u64 = 0;
            for entry in archive.entries()? {
                let mut entry = entry?;
                let size = entry.header().size().unwrap_or(0);
                // unpack_in checks for path traversal attacks internally
                entry.unpack_in(dst)?;
                done_bytes += size;
                done_files += 1;
                let current = if total > 0 { done_bytes.min(total) } else { done_files };
                let _ = tx.send(ExtractionProgress {
                    filename: fname.to_string(), current, total,
                    done: false, error: None, start_time: Instant::now(), pid: None,
                });
            }
        }};
    }

    if name_lower.ends_with(".tar.gz")  || name_lower.ends_with(".tgz") {
        unpack!(flate2::read::GzDecoder::new(buf));
    } else if name_lower.ends_with(".tar.bz2") || name_lower.ends_with(".tbz2") {
        unpack!(bzip2::read::BzDecoder::new(buf));
    } else if name_lower.ends_with(".tar.xz") {
        unpack!(xz2::read::XzDecoder::new(buf));
    } else if name_lower.ends_with(".tar.zst") {
        let dec = zstd::stream::read::Decoder::new(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        unpack!(dec);
    } else {
        unpack!(buf);
    }
    Ok(())
}

fn extract_rar_via_unrar(
    src_s: &str, dst_s: &str,
    total: u64, tx: &mpsc::Sender<ExtractionProgress>, fname: &str,
) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader};

    let mut child = Command::new("unrar")
        .args(["x", "-o+", src_s, &format!("{}/", dst_s)])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())  // capture stderr for error reporting
        .stdin(std::process::Stdio::null())
        .spawn()?;

    let child_pid = child.id();
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let _ = tx.send(ExtractionProgress {
        filename: fname.to_string(), current: 0, total,
        done: false, error: None, start_time: Instant::now(),
        pid: Some(child_pid),
    });

    // Count total files for proportional progress
    let total_files: u64 = Command::new("unrar").args(["l", src_s]).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("Archive:") && !t.starts_with("Details:")
                    && !t.starts_with("Name") && !t.starts_with("----")
                    && !t.contains("files,") && t.split_whitespace().count() >= 2
            })
            .count() as u64)
        .unwrap_or(0);

    // Drain stderr in a separate thread so it doesn't block stdout reading
    let stderr_thread = std::thread::spawn(move || {
        BufReader::new(stderr).lines()
            .filter_map(|l| l.ok())
            .collect::<Vec<_>>()
            .join("
")
    });

    let mut done: u64 = 0;
    for line in BufReader::new(stdout).lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        if line.trim_start().starts_with("Extracting") {
            done += 1;
            let current = if total > 0 && total_files > 0 {
                (done * total / total_files).min(total)
            } else { done };
            let _ = tx.send(ExtractionProgress {
                filename: fname.to_string(), current, total,
                done: false, error: None, start_time: Instant::now(), pid: None,
            });
        }
    }

    let status = child.wait()?;
    let stderr_output = stderr_thread.join().unwrap_or_default();

    if !status.success() {
        let msg = if !stderr_output.trim().is_empty() {
            stderr_output.trim().to_string()
        } else {
            format!("unrar exited with code {:?}", status.code())
        };
        return Err(std::io::Error::new(std::io::ErrorKind::Other, msg));
    }
    Ok(())
}
