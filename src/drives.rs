// drives.rs — Drive/USB/MTP detection, mounting, unmounting
//
// Block devices  → lsblk (all partitions, internal + external)
// Mount/unmount  → udisksctl (no sudo needed for removable media)
//                  internal system partitions show as read-only info
// Android phones → jmtpfs
// Mount points   → auto via udisksctl; MTP mounts to ~/mnt/phone-<label>

use std::{path::PathBuf, process::Command};

// ─── Device model ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum DeviceKind {
    Internal,    // system partition (/, /boot, swap, etc.)
    Removable,   // external drive, USB stick
    MtpPhone,    // Android phone via jmtpfs
}

#[derive(Clone)]
pub struct DriveDevice {
    pub kind:    DeviceKind,
    pub label:   String,   // human-readable name
    pub device:  String,   // "/dev/sdb1" or "003:014" for MTP
    pub size:    String,   // "128G"
    pub fstype:  String,   // "ext4", "vfat", "MTP", …
    pub mount:   Option<PathBuf>,
}

impl DriveDevice {
    pub fn is_navigable(&self) -> bool {
        self.mount.is_some()
            && self.fstype != "swap"
            && self.fstype != "linux-swap"
    }
}

// ─── Device listing ───────────────────────────────────────────────────────────

pub fn list_devices() -> Vec<DriveDevice> {
    let mut devices = Vec::new();
    devices.extend(list_block_devices());
    devices.extend(list_mtp_phones());
    // Sort: internal first (by mount point), then removable, then MTP
    devices.sort_by(|a, b| {
        let rank = |d: &DriveDevice| match d.kind {
            DeviceKind::Internal  => 0,
            DeviceKind::Removable => 1,
            DeviceKind::MtpPhone  => 2,
        };
        rank(a).cmp(&rank(b))
            .then(a.mount.cmp(&b.mount))
            .then(a.device.cmp(&b.device))
    });
    devices
}

// ── Block devices (lsblk) ────────────────────────────────────────────────────

/// Classify a block device as Internal or Removable.
///
/// Priority:
///   1. lsblk hotplug/rm flag  → Removable  (most reliable)
///   2. Mount under /run/media or /media → Removable (udisksctl always mounts here)
///   3. Mount under known system paths   → Internal
///   4. Unmounted, not flagged           → Removable (let user try to mount it)
fn classify_kind(is_hotplug: bool, mount: &Option<std::path::PathBuf>) -> DeviceKind {
    if is_hotplug { return DeviceKind::Removable; }
    if let Some(p) = mount {
        let s = p.to_string_lossy();
        // udisksctl mounts USB drives here — always removable
        if s.starts_with("/run/media") || s.starts_with("/media") {
            return DeviceKind::Removable;
        }
        // Genuine system mounts
        if matches!(s.as_ref(),
            "/" | "/boot" | "/home" | "/var" | "/tmp" | "/opt" | "/usr" | "/srv"
        ) || s.starts_with("/boot")
          || s.starts_with("/efi")
          || s.starts_with("/esp")
          || s.starts_with("/nix")
        {
            return DeviceKind::Internal;
        }
        // Mounted somewhere else (e.g. /mnt/data) — could go either way.
        // Treat as Internal since the user set it up deliberately in fstab.
        return DeviceKind::Internal;
    }
    // Unmounted and not flagged — show it, let the user decide
    DeviceKind::Removable
}

fn list_block_devices() -> Vec<DriveDevice> {
    // Try JSON first (lsblk >= 2.27), fall back to plain text
    lsblk_json().unwrap_or_else(lsblk_plain)
}

fn lsblk_json() -> Option<Vec<DriveDevice>> {
    let out = Command::new("lsblk")
        .args(["-J", "-o", "NAME,SIZE,FSTYPE,LABEL,MODEL,MOUNTPOINTS,MOUNTPOINT,TYPE,HOTPLUG,RM,PARTLABEL"])
        .output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut devs = Vec::new();
    // Walk the blockdevices array recursively so we never process a disk node
    // AND its child partitions — only the leaves get through.
    // A flat brace-walk was the root cause of duplicate/stray swap entries.
    let bd_start = text.find("\"blockdevices\"")?;
    let arr_off  = text[bd_start..].find('[')?;
    walk_lsblk_array(&text, bd_start + arr_off, &mut devs);
    if devs.is_empty() { None } else { Some(devs) }
}

/// Recursively walk a lsblk JSON array.
/// Recurse into "children" before processing the parent — that way only
/// leaf nodes (real partitions) are ever passed to parse_lsblk_obj.
fn walk_lsblk_array(text: &str, arr_start: usize, out: &mut Vec<DriveDevice>) {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = arr_start;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => { depth += 1; i += 1; }
            b']' => { depth -= 1; if depth <= 0 { break; } i += 1; }
            b'{' if depth == 1 => {
                let obj = extract_json_object(text, i);
                let obj_end = i + obj.len();
                if let Some(ch_off) = obj.find("\"children\"") {
                    if let Some(arr_off) = obj[ch_off..].find('[') {
                        walk_lsblk_array(&obj, ch_off + arr_off, out);
                    }
                } else {
                    // Leaf node — no children, safe to parse directly
                    if let Some(d) = parse_lsblk_obj(&obj) { out.push(d); }
                }
                i = obj_end;
            }
            _ => { i += 1; }
        }
    }
}

fn extract_json_object(text: &str, start: usize) -> String {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => { depth -= 1; if depth == 0 { return text[start..=i].to_string(); } }
            _ => {}
        }
        i += 1;
    }
    text[start..].to_string()
}

fn parse_lsblk_obj(obj: &str) -> Option<DriveDevice> {
    let typ = json_str(obj, "type")?;
    // Only partitions and plain disks without a partition table
    if typ != "part" && typ != "disk" { return None; }
    // Skip disks that have children (partitions) — we list the partitions instead
    if typ == "disk" && obj.contains("\"children\"") { return None; }

    let name       = json_str(obj, "name").unwrap_or_default();
    let size       = json_str(obj, "size").unwrap_or_default();
    let fstype     = json_str(obj, "fstype").unwrap_or_default();
    let label      = json_str(obj, "label").unwrap_or_default();
    let partlabel  = json_str(obj, "partlabel").unwrap_or_default();
    let model      = json_str(obj, "model").unwrap_or_default();
    let hotplug    = json_str(obj, "hotplug").unwrap_or_default();
    let rm         = json_str(obj, "rm").unwrap_or_default();

    if name.is_empty() { return None; }

    // Skip swap entirely — not useful to show in file manager
    let fstype_clean = fstype.trim().to_string();
    if fstype_clean == "swap" || fstype_clean == "linux-swap" { return None; }

    // Determine mount point — lsblk >= 2.38 uses "mountpoints" (array),
    // older versions use "mountpoint" (string). Try both.
    let mnt_str = json_str(obj, "mountpoint")
        .or_else(|| json_array_first(obj, "mountpoints"))
        .unwrap_or_default();
    let mnt_decoded = unescape_path(&mnt_str);
    let mount = if mnt_decoded.is_empty() || mnt_decoded == "null" || mnt_decoded == "[SWAP]" {
        None
    } else {
        Some(PathBuf::from(&mnt_decoded))
    };

    // Build label: prefer partition label > filesystem label > model > device name
    let display_label = [&label, &partlabel, &model, &name]
        .iter()
        .find(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| name.clone());

    let is_removable = hotplug == "1" || rm == "1";
    let kind = classify_kind(is_removable, &mount);

    Some(DriveDevice {
        kind,
        label: display_label,
        device: format!("/dev/{}", name),
        size,
        fstype: if fstype_clean.is_empty() { "—".into() } else { fstype_clean },
        mount,
    })
}

fn lsblk_plain() -> Vec<DriveDevice> {
    let out = match Command::new("lsblk")
        .args(["-rno", "NAME,SIZE,FSTYPE,LABEL,MOUNTPOINT,HOTPLUG,RM,TYPE"])
        .output() { Ok(o) => o, Err(_) => return vec![] };
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut devs = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.splitn(8, ' ').collect();
        if cols.len() < 8 { continue; }
        let (name, size, fstype, label, mnt, hotplug, rm, typ) =
            (cols[0], cols[1], cols[2], cols[3], cols[4], cols[5], cols[6], cols[7]);
        if typ != "part" && typ != "disk" { continue; }
        let mnt_decoded = unescape_path(mnt);
        let mount = if mnt_decoded.is_empty() || mnt_decoded == "[SWAP]" { None } else { Some(PathBuf::from(&mnt_decoded)) };
        if fstype == "swap" || fstype == "linux-swap" { continue; }
        let is_removable = hotplug == "1" || rm == "1";
        let kind = classify_kind(is_removable, &mount);
        let display_label = if !label.is_empty() { label.to_string() } else { name.to_string() };
        devs.push(DriveDevice {
            kind,
            label: display_label,
            device: format!("/dev/{}", name),
            size: size.to_string(),
            fstype: if fstype.is_empty() { "—".into() } else { fstype.to_string() },
            mount,
        });
    }
    devs
}

// ── MTP phones — detected via /sys/bus/usb/devices/ ─────────────────────────
//
// Instead of parsing `jmtpfs -l` (unreliable output format), we read the USB
// device tree directly from sysfs. Every USB device exposes its class, vendor,
// product, and manufacturer there — no external tools needed for detection.
// We still use jmtpfs for the actual mount step.

fn list_mtp_phones() -> Vec<DriveDevice> {
    let mounted = mtp_mounts();
    let mut phones = Vec::new();

    // Walk every USB device entry in sysfs
    let usb_root = std::path::Path::new("/sys/bus/usb/devices");
    let rd = match std::fs::read_dir(usb_root) { Ok(r) => r, Err(_) => return vec![] };

    for entry in rd.filter_map(|e| e.ok()) {
        let dev_path = entry.path();

        // bDeviceClass == 0 means class is defined per-interface — check interfaces.
        // bDeviceClass == 6 is PTP/MTP directly on the device.
        let dev_class = read_sysfs_u8(&dev_path, "bDeviceClass").unwrap_or(0xFF);

        let is_mtp = if dev_class == 6 {
            // PTP class on device level — definitely a camera/phone
            true
        } else if dev_class == 0 {
            // Class defined per-interface — scan interface descriptors
            has_mtp_interface(&dev_path)
        } else {
            false
        };

        if !is_mtp { continue; }

        // Read bus and device numbers for the jmtpfs device identifier
        let busnum = read_sysfs_u32(&dev_path, "busnum").unwrap_or(0);
        let devnum = read_sysfs_u32(&dev_path, "devnum").unwrap_or(0);
        if busnum == 0 || devnum == 0 { continue; }

        let device_id = format!("{}:{}", busnum, devnum);

        // Build a human-readable label from manufacturer + product strings
        let manufacturer = read_sysfs_str(&dev_path, "manufacturer").unwrap_or_default();
        let product      = read_sysfs_str(&dev_path, "product").unwrap_or_default();
        let label = match (manufacturer.is_empty(), product.is_empty()) {
            (false, false) => format!("{} {}", manufacturer, product),
            (false, true)  => manufacturer,
            (true,  false) => product,
            (true,  true)  => "Android Phone".to_string(),
        };

        // Check if already mounted — search all known MTP mount points
        let label_slug: String = label.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>().trim_matches('-').to_string();
        let mount = mounted.iter().find(|p| {
            let dir = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let full = p.to_string_lossy().to_lowercase();
            // gvfs dir name starts with "mtp:"
            dir.starts_with("mtp:")
                // jmtpfs slug match
                || (!label_slug.is_empty() && full.contains(&format!("phone-{}", label_slug)))
                // jmtpfs bus:dev fallback
                || full.contains(&format!("{}-{}", busnum, devnum))
        }).cloned();

        phones.push(DriveDevice {
            kind:   DeviceKind::MtpPhone,
            label,
            device: device_id,
            size:   String::new(),
            fstype: "MTP".into(),
            mount,
        });
    }

    phones
}

/// Read a single-line sysfs attribute as a trimmed string.
fn read_sysfs_str(dev: &std::path::Path, attr: &str) -> Option<String> {
    std::fs::read_to_string(dev.join(attr))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Read a sysfs hex attribute (e.g. "06") as u8.
fn read_sysfs_u8(dev: &std::path::Path, attr: &str) -> Option<u8> {
    let s = read_sysfs_str(dev, attr)?;
    u8::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// Read a sysfs decimal attribute as u32.
fn read_sysfs_u32(dev: &std::path::Path, attr: &str) -> Option<u32> {
    read_sysfs_str(dev, attr)?.parse().ok()
}

/// Check if any interface of a USB device is MTP (class=6) or has the MTP
/// protocol string. Android phones expose MTP as an interface, not at device level.
fn has_mtp_interface(dev_path: &std::path::Path) -> bool {
    // IMPORTANT: interface directories (e.g. "1-9:1.0") are NOT children of the
    // device directory ("1-9/"). They are siblings in /sys/bus/usb/devices/.
    // So we must scan the PARENT directory for entries prefixed with "devname:".
    let dev_name = match dev_path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n.to_string(),
        None    => return false,
    };
    let parent = match dev_path.parent() {
        Some(p) => p,
        None    => return false,
    };
    let prefix = format!("{}:", dev_name);

    let rd = match std::fs::read_dir(parent) { Ok(r) => r, Err(_) => return false };
    for iface in rd.filter_map(|e| e.ok()) {
        let iface_path = iface.path();
        let iface_name = iface_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if !iface_name.starts_with(&prefix) { continue; }

        let class    = read_sysfs_u8(&iface_path, "bInterfaceClass").unwrap_or(0);
        let subclass = read_sysfs_u8(&iface_path, "bInterfaceSubClass").unwrap_or(0);

        // class 6, subclass 1 = Still Image / PTP / MTP
        if class == 6 && subclass == 1 {
            return true;
        }

        // Vendor-specific class (0xFF) with "MTP" in the interface string
        // covers some Samsung and OPPO/OnePlus devices
        if class == 0xFF {
            if let Some(s) = read_sysfs_str(&iface_path, "interface") {
                if s.to_uppercase().contains("MTP") || s.to_uppercase().contains("ANDROID") {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns mount points of all active MTP mounts — jmtpfs and gvfs.
/// jmtpfs appears in /proc/mounts; gvfs appears in /run/user/<uid>/gvfs/mtp:...
fn mtp_mounts() -> Vec<PathBuf> {
    let mut mounts = Vec::new();
    // jmtpfs
    if let Ok(text) = std::fs::read_to_string("/proc/mounts") {
        for line in text.lines() {
            let mut p = line.split_whitespace();
            let _dev = p.next().unwrap_or("");
            let mnt  = p.next().unwrap_or("");
            let fstp = p.next().unwrap_or("");
            if fstp == "fuse.jmtpfs" {
                mounts.push(PathBuf::from(unescape_path(mnt)));
            }
        }
    }
    // gvfs — scan directory directly, these never appear in /proc/mounts
    let gvfs = PathBuf::from("/run/user").join(get_uid()).join("gvfs");
    if let Ok(rd) = std::fs::read_dir(&gvfs) {
        for entry in rd.filter_map(|e| e.ok()) {
            if entry.file_name().to_string_lossy().to_lowercase().starts_with("mtp:") {
                mounts.push(entry.path());
            }
        }
    }
    mounts
}

// ─── Minimal JSON helpers ─────────────────────────────────────────────────────



fn json_str(obj: &str, key: &str) -> Option<String> {
    let pat  = format!("\"{}\":", key);
    let pos  = obj.find(&pat)?;
    let rest = obj[pos + pat.len()..].trim_start();
    if rest.starts_with("null") { return Some(String::new()); }
    if rest.starts_with('"') {
        let inner = &rest[1..];
        let end   = inner.find('"')?;
        return Some(inner[..end].to_string());
    }
    None
}

/// Grab first non-null string from a JSON array value: "key": ["val", ...]
fn json_array_first(obj: &str, key: &str) -> Option<String> {
    let pat  = format!("\"{}\":", key);
    let pos  = obj.find(&pat)?;
    let rest = obj[pos + pat.len()..].trim_start();
    if !rest.starts_with('[') { return None; }
    // Find first quoted string inside the array
    let inner = &rest[1..];
    let sq = inner.find('"')?;
    let after_quote = &inner[sq+1..];
    let eq = after_quote.find('"')?;
    let val = &after_quote[..eq];
    if val.is_empty() || val == "null" { None } else { Some(val.to_string()) }
}

/// Decode lsblk's octal/hex escape sequences in paths.
/// lsblk encodes spaces as \x20, tabs as \x09, etc.
fn unescape_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // collect next char
            match chars.peek().copied() {
                Some('x') => {
                    chars.next(); // consume 'x'
                    let h1 = chars.next().unwrap_or('0');
                    let h2 = chars.next().unwrap_or('0');
                    let hex = format!("{}{}", h1, h2);
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        out.push(byte as char);
                    } else {
                        out.push('\\'); out.push('x'); out.push(h1); out.push(h2);
                    }
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ─── Mount / Unmount ──────────────────────────────────────────────────────────

pub enum MountResult {
    Ok(PathBuf),
    Err(String),
}

pub fn mount_device(dev: &DriveDevice) -> MountResult {
    match dev.kind {
        DeviceKind::Removable => mount_partition(dev),
        DeviceKind::Internal  => MountResult::Err("Internal partitions are managed by the system".into()),
        DeviceKind::MtpPhone  => mount_mtp(dev),
    }
}

pub fn unmount_device(dev: &DriveDevice) -> Result<(), String> {
    match dev.kind {
        DeviceKind::Removable => unmount_partition(dev),
        DeviceKind::Internal  => Err("Cannot unmount system partitions from here".into()),
        DeviceKind::MtpPhone  => unmount_mtp(dev),
    }
}

fn mount_partition(dev: &DriveDevice) -> MountResult {
    let out = Command::new("udisksctl")
        .args(["mount", "-b", &dev.device, "--no-user-interaction"])
        .output();
    match out {
        Err(e) => MountResult::Err(format!("udisksctl not found: {}", e)),
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
            if o.status.success() {
                let mnt = stdout.split(" at ").nth(1)
                    .map(|s| s.trim().trim_end_matches('.').to_string())
                    .unwrap_or_default();
                if mnt.is_empty() { MountResult::Err("Mounted but couldn't determine mount point".into()) }
                else { MountResult::Ok(PathBuf::from(mnt)) }
            } else {
                let msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else { stdout.trim().to_string() };
                MountResult::Err(msg)
            }
        }
    }
}

fn unmount_partition(dev: &DriveDevice) -> Result<(), String> {
    let out = Command::new("udisksctl")
        .args(["unmount", "-b", &dev.device, "--no-user-interaction"])
        .output()
        .map_err(|e| format!("udisksctl not found: {}", e))?;
    if out.status.success() { Ok(()) }
    else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() { "Unmount failed".into() } else { msg })
    }
}

fn mount_mtp(dev: &DriveDevice) -> MountResult {
    // Try gio mount first — same as Nautilus/Nemo, no permissions needed
    let (bus_n, dev_n) = dev.device.split_once(':').unwrap_or(("0", "0"));
    let usb_path = format!("/dev/bus/usb/{:03}/{:03}",
        bus_n.parse::<u32>().unwrap_or(0),
        dev_n.parse::<u32>().unwrap_or(0));

    if let Ok(out) = Command::new("gio").args(["mount", "-d", &usb_path]).output() {
        if out.status.success() {
            // Give gvfsd a moment to register the mount
            std::thread::sleep(std::time::Duration::from_millis(500));
            // Reuse mtp_mounts() which already scans gvfs
            let mnt = mtp_mounts().into_iter().find(|p| {
                p.file_name().map(|n| n.to_string_lossy().to_lowercase().starts_with("mtp:")).unwrap_or(false)
            });
            return match mnt {
                Some(p) => MountResult::Ok(p),
                None    => MountResult::Err("gio mounted but path not found in gvfs".into()),
            };
        }
    }

    // Fallback: jmtpfs into ~/.cache/VoidDream/mounts/ (always writable)
    mount_mtp_jmtpfs(dev)
}

fn get_uid() -> String {
    if let Ok(out) = Command::new("id").arg("-u").output() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() { return s; }
    }
    if let Ok(text) = std::fs::read_to_string("/proc/self/status") {
        for line in text.lines() {
            if line.starts_with("Uid:") {
                if let Some(uid) = line.split_whitespace().nth(1) {
                    return uid.to_string();
                }
            }
        }
    }
    "1000".to_string()
}

fn mount_mtp_jmtpfs(dev: &DriveDevice) -> MountResult {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let label_slug: String = dev.label.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>().trim_matches('-').to_string();
    let (bus_n, dev_n) = dev.device.split_once(':').unwrap_or(("0", "0"));
    let dir_name = if label_slug.is_empty() {
        format!("phone-{}-{}", bus_n, dev_n)
    } else {
        format!("phone-{}", label_slug)
    };
    let mnt = PathBuf::from(&home).join(".cache").join("VoidDream").join("mounts").join(&dir_name);
    if let Err(e) = std::fs::create_dir_all(&mnt) {
        return MountResult::Err(format!("Can't create mount point: {}", e));
    }
    let out = Command::new("jmtpfs").arg(&mnt).output();
    match out {
        Err(e) => { let _ = std::fs::remove_dir(&mnt); MountResult::Err(format!("jmtpfs not found: {}", e)) }
        Ok(o) => {
            if o.status.success() { MountResult::Ok(mnt) }
            else {
                let _ = std::fs::remove_dir(&mnt);
                let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
                MountResult::Err(if msg.is_empty() { "jmtpfs mount failed".into() } else { msg })
            }
        }
    }
}

fn unmount_mtp(dev: &DriveDevice) -> Result<(), String> {
    let mnt = dev.mount.as_ref().ok_or("Not mounted")?;
    let mnt_str = mnt.to_string_lossy();

    // gvfs mount — use gio unmount
    if mnt_str.contains("gvfs") || mnt_str.contains("mtp:") {
        let out = Command::new("gio")
            .args(["mount", "-u", &mnt_str])
            .output()
            .map_err(|e| format!("gio not found: {}", e))?;
        if out.status.success() { return Ok(()); }
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if msg.is_empty() { "gio unmount failed".into() } else { msg });
    }

    // jmtpfs mount — use fusermount
    let out = Command::new("fusermount")
        .args(["-u", &mnt_str])
        .output()
        .map_err(|e| format!("fusermount not found: {}", e))?;
    if out.status.success() { let _ = std::fs::remove_dir(mnt); Ok(()) }
    else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() { "fusermount failed".into() } else { msg })
    }
}
