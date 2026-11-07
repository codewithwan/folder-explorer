//! Mounted volumes, disk usage (statfs/statvfs) and the home-folder pins.

use super::{Drive, DiskUsage};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

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
