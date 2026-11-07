//! macOS / Unix implementation of the platform layer. Mirrors the public API
//! of `platform_windows.rs`. Timestamps stay in Windows FILETIME ticks (100 ns
//! since 1601) so the rest of the app is unchanged; conversions use libc.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::ops::Range;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fsmeta::UNIX_EPOCH_TICKS;

const TICKS_PER_SECOND: u64 = 10_000_000;

/// Parsing name for the Windows "Quick access" shell folder; unused here.
pub const QUICK_ACCESS: &str = "";

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

pub fn now_filetime() -> u64 {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    UNIX_EPOCH_TICKS + (since_epoch.as_nanos() / 100) as u64
}

pub fn format_time(ticks: u64) -> String {
    if ticks == 0 || ticks < UNIX_EPOCH_TICKS {
        return String::new();
    }
    let secs = ((ticks - UNIX_EPOCH_TICKS) / TICKS_PER_SECOND) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return String::new();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

/// FILETIME ticks at local midnight starting the given calendar day. `mktime`
/// interprets the broken-down time as local, normalizing out-of-range fields
/// (so day + 1 rolls into the next month) and resolving DST via `tm_isdst = -1`.
fn local_midnight_ticks(year: i32, month: i32, day: i32) -> Option<u64> {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_isdst = -1;
    let secs = unsafe { libc::mktime(&mut tm) };
    if secs == -1 {
        return None;
    }
    Some(UNIX_EPOCH_TICKS + (secs as u64) * TICKS_PER_SECOND)
}

pub fn local_day(text: &str) -> Option<Range<u64>> {
    let mut parts = text.trim().splitn(3, '-').map(|part| part.parse::<i32>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(local_midnight_ticks(year, month, day)?..local_midnight_ticks(year, month, day + 1)?)
}

// ---------------------------------------------------------------------------
// Window frame / placement
// ---------------------------------------------------------------------------

pub fn allow_dark_menus() {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPlacement {
    pub rect: [i32; 4],
    pub maximized: bool,
}

impl WindowPlacement {
    pub fn parse(value: &str) -> Option<Self> {
        let fields: Vec<&str> = value.split(',').collect();
        let [left, top, right, bottom, maximized] = fields[..] else { return None };
        let rect = [left.parse().ok()?, top.parse().ok()?, right.parse().ok()?, bottom.parse().ok()?];
        (rect[2] > rect[0] && rect[3] > rect[1]).then_some(Self { rect, maximized: maximized == "true" })
    }

    pub fn serialize(&self) -> String {
        let [left, top, right, bottom] = self.rect;
        format!("{left},{top},{right},{bottom},{}", self.maximized)
    }

    pub fn size(&self) -> (u32, u32) {
        ((self.rect[2] - self.rect[0]) as u32, (self.rect[3] - self.rect[1]) as u32)
    }
}

pub fn window_placement(window: &slint::Window) -> Option<WindowPlacement> {
    let size = window.size();
    if size.width == 0 || size.height == 0 {
        return None;
    }
    let position = window.position();
    Some(WindowPlacement {
        rect: [position.x, position.y, position.x + size.width as i32, position.y + size.height as i32],
        maximized: false,
    })
}

pub fn apply_window_placement(window: &slint::Window, saved: WindowPlacement) {
    let (width, height) = saved.size();
    window.set_size(slint::PhysicalSize::new(width, height));
    window.set_position(slint::PhysicalPosition::new(saved.rect[0], saved.rect[1]));
}

/// No custom frame on macOS: the Slint window keeps its own decorations/move
/// area. Mouse back/forward buttons are not wired on this platform.
pub fn restore_frame_appearance(_window: &slint::Window, _on_navigate: impl Fn(bool) + 'static) {}

// ---------------------------------------------------------------------------
// Drives / disk usage
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Drive {
    pub root: PathBuf,
    pub title: String,
    pub usage: Option<DiskUsage>,
    pub network: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DiskUsage {
    pub total: u64,
    pub free: u64,
}

impl DiskUsage {
    pub fn used_ratio(self) -> f32 {
        if self.total == 0 { 0.0 } else { (self.total - self.free) as f32 / self.total as f32 }
    }
}

#[cfg(target_os = "macos")]
pub fn disk_usage(root: &Path) -> Option<DiskUsage> {
    let path = std::ffi::CString::new(root.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_bsize as u64;
    let total = stat.f_blocks * block;
    let free = stat.f_bavail * block;
    (total > 0).then_some(DiskUsage { total, free })
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn disk_usage(root: &Path) -> Option<DiskUsage> {
    let path = std::ffi::CString::new(root.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = if stat.f_frsize != 0 { stat.f_frsize } else { stat.f_bsize } as u64;
    let total = stat.f_blocks as u64 * block;
    let free = stat.f_bavail as u64 * block;
    (total > 0).then_some(DiskUsage { total, free })
}

fn describe_volume(path: &Path) -> Option<Drive> {
    let usage = disk_usage(path)?;
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Macintosh HD".to_string());
    Some(Drive { root: path.to_path_buf(), title, usage: Some(usage), network: false })
}

pub fn drives() -> Vec<Drive> {
    let mut list = Vec::new();
    if let Some(root) = describe_volume(Path::new("/")) {
        list.push(root);
    }
    if let Ok(entries) = fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            // The boot volume shows up as a symlink to "/"; skip it.
            if entry.file_type().map(|kind| kind.is_symlink()).unwrap_or(true) {
                continue;
            }
            if let Some(drive) = describe_volume(&entry.path()) {
                if !list.iter().any(|existing| existing.root == drive.root) {
                    list.push(drive);
                }
            }
        }
    }
    list
}

pub fn quick_access_pins() -> Vec<(PathBuf, String)> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return Vec::new() };
    ["Desktop", "Documents", "Downloads", "Pictures", "Music", "Movies"]
        .iter()
        .map(|name| (home.join(name), (*name).to_string()))
        .filter(|(path, _)| path.is_dir())
        .collect()
}

// ---------------------------------------------------------------------------
// Opening / revealing
// ---------------------------------------------------------------------------

fn applescript_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn open(path: &Path) -> bool {
    Command::new("open").arg(path).status().map(|status| status.success()).unwrap_or(false)
}

pub fn show_properties(path: &Path) -> bool {
    let script = format!(
        "tell application \"Finder\"\nactivate\nopen information window of (POSIX file \"{}\" as alias)\nend tell",
        applescript_escape(&path.to_string_lossy())
    );
    Command::new("osascript").arg("-e").arg(script).status().map(|status| status.success()).unwrap_or(false)
}

pub fn reveal_in_explorer(path: &Path) {
    let _ = Command::new("open").arg("-R").arg(path).status();
}

pub fn active_window() -> usize {
    0
}

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

fn remove_any(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path).is_ok(),
        Ok(_) => fs::remove_file(path).is_ok(),
        Err(_) => false,
    }
}

fn unique_destination(dir: &Path, name: &OsStr) -> PathBuf {
    let direct = dir.join(name);
    if !direct.exists() {
        return direct;
    }
    let name = Path::new(name);
    let stem = name.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = name.extension().map(|ext| ext.to_string_lossy().into_owned());
    for n in 2..10_000 {
        let candidate = match &extension {
            Some(extension) => format!("{stem} {n}.{extension}"),
            None => format!("{stem} {n}"),
        };
        let path = dir.join(candidate);
        if !path.exists() {
            return path;
        }
    }
    direct
}

fn copy_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(src)?, dest)?;
    } else if kind.is_dir() {
        fs::create_dir(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        fs::copy(src, dest)?;
    }
    Ok(())
}

pub fn recycle(paths: &[PathBuf], _owner: usize) -> bool {
    !paths.is_empty() && trash::delete_all(paths).is_ok()
}

pub fn delete_permanently(paths: &[PathBuf], _owner: usize) -> bool {
    !paths.is_empty() && paths.iter().all(|path| remove_any(path))
}

pub fn copy_into(paths: &[PathBuf], target: &Path, _owner: usize) -> bool {
    paths.iter().all(|src| {
        let Some(name) = src.file_name() else { return false };
        copy_recursive(src, &unique_destination(target, name)).is_ok()
    })
}

pub fn move_into(paths: &[PathBuf], target: &Path, _owner: usize) -> bool {
    paths.iter().all(|src| {
        let Some(name) = src.file_name() else { return false };
        let dest = unique_destination(target, name);
        match fs::rename(src, &dest) {
            Ok(()) => true,
            // Cross-device move: copy then remove the source.
            Err(_) => copy_recursive(src, &dest).is_ok() && remove_any(src),
        }
    })
}

/// Rename without overwriting (matching Windows `MoveFileW`). A case-only
/// rename of the same file on a case-insensitive volume is allowed.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    if let Ok(to_meta) = to.symlink_metadata() {
        use std::os::unix::fs::MetadataExt;
        let same = from
            .symlink_metadata()
            .map(|from_meta| from_meta.ino() == to_meta.ino() && from_meta.dev() == to_meta.dev())
            .unwrap_or(false);
        if !same {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "A file with that name already exists"));
        }
    }
    fs::rename(from, to)
}

fn trash_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Trash"))
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => total += dir_size(&entry.path()),
                Ok(meta) => total += crate::fsmeta::size(&meta),
                Err(_) => {}
            }
        }
    }
    total
}

pub fn recycle_bin_size() -> Option<u64> {
    Some(dir_size(&trash_dir()?))
}

pub fn empty_recycle_bin() {
    let Some(trash) = trash_dir() else { return };
    if let Ok(entries) = fs::read_dir(&trash) {
        for entry in entries.flatten() {
            remove_any(&entry.path());
        }
    }
}

// ---------------------------------------------------------------------------
// Clipboard
//
// macOS has no simple, dependency-free way to put real file references on the
// pasteboard for Finder to paste, so cut/copy/paste works within the app via a
// process-global store. Text and file paths are mirrored to the system text
// clipboard through `arboard` so copied paths are at least pasteable elsewhere.
// ---------------------------------------------------------------------------

static CLIPBOARD: Mutex<Option<(Vec<PathBuf>, bool)>> = Mutex::new(None);
static SEQUENCE: AtomicU32 = AtomicU32::new(1);

fn bump_sequence() {
    SEQUENCE.fetch_add(1, Ordering::SeqCst);
}

fn set_system_text(text: String) -> bool {
    arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text)).is_ok()
}

pub fn copy_text(text: impl AsRef<OsStr>) -> bool {
    let text = text.as_ref().to_string_lossy().into_owned();
    *CLIPBOARD.lock().unwrap() = None;
    let stored = set_system_text(text);
    bump_sequence();
    stored
}

fn put_files(paths: &[PathBuf], is_cut: bool) -> bool {
    *CLIPBOARD.lock().unwrap() = Some((paths.to_vec(), is_cut));
    let text = paths.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>().join("\n");
    set_system_text(text);
    bump_sequence();
    true
}

pub fn copy_files(paths: &[PathBuf]) -> bool {
    put_files(paths, false)
}

pub fn cut_files(paths: &[PathBuf]) -> bool {
    put_files(paths, true)
}

pub fn clipboard_has_files() -> bool {
    CLIPBOARD.lock().unwrap().is_some()
}

pub fn clipboard_sequence() -> u32 {
    SEQUENCE.load(Ordering::SeqCst)
}

pub fn clear_clipboard_if_unchanged(sequence: u32) {
    if SEQUENCE.load(Ordering::SeqCst) == sequence {
        *CLIPBOARD.lock().unwrap() = None;
    }
}

pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    pub is_cut: bool,
    pub sequence: u32,
}

pub fn clipboard_files() -> Option<ClipboardFiles> {
    let guard = CLIPBOARD.lock().unwrap();
    let (paths, is_cut) = guard.as_ref()?;
    Some(ClipboardFiles { paths: paths.clone(), is_cut: *is_cut, sequence: SEQUENCE.load(Ordering::SeqCst) })
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

pub fn confirm(title: &str, message: &str) -> bool {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(title)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

pub fn alert(title: &str, message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

pub fn save_dialog(title: &str, file_name: &str, filters: &[(&str, &str)]) -> Option<(PathBuf, usize)> {
    let mut dialog = rfd::FileDialog::new().set_title(title).set_file_name(file_name);
    for (label, extension) in filters {
        dialog = dialog.add_filter(*label, &[*extension]);
    }
    let path = dialog.save_file()?;
    let chosen = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    let index = chosen
        .as_deref()
        .and_then(|ext| filters.iter().position(|(_, candidate)| candidate.eq_ignore_ascii_case(ext)))
        .unwrap_or(0);
    Some((path, index))
}
