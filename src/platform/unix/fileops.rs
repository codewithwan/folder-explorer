//! Opening/revealing items in Finder and the file operations
//! (copy / move / Trash / delete / rename, Recycle Bin = ~/.Trash).

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

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
                Ok(meta) => total += crate::util::fsmeta::size(&meta),
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
