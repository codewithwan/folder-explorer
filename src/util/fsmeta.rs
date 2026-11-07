//! Cross-platform access to the file metadata the app models in Windows terms:
//! a logical size, a Windows-style attribute bitmask and FILETIME timestamps
//! (100 ns ticks since 1601). On Windows these come straight from the OS; on
//! Unix they are synthesized from `std::fs::Metadata` so the rest of the code
//! (sorting, filtering, `format_time`) stays identical.

use std::fs::Metadata;

/// FILETIME ticks at the Unix epoch (1970-01-01).
pub const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

#[cfg(windows)]
pub fn size(meta: &Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;
    meta.file_size()
}

#[cfg(not(windows))]
pub fn size(meta: &Metadata) -> u64 {
    meta.len()
}

#[cfg(windows)]
pub fn attrs(meta: &Metadata, _name: &std::ffi::OsStr) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
pub fn attrs(meta: &Metadata, name: &std::ffi::OsStr) -> u32 {
    use crate::model::item::{ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_READONLY, ATTR_REPARSE_POINT};
    let mut attrs = 0;
    if meta.is_dir() {
        attrs |= ATTR_DIRECTORY;
    }
    if meta.file_type().is_symlink() {
        attrs |= ATTR_REPARSE_POINT;
    }
    if meta.permissions().readonly() {
        attrs |= ATTR_READONLY;
    }
    if name.as_encoded_bytes().first() == Some(&b'.') {
        attrs |= ATTR_HIDDEN;
    }
    attrs
}

/// Convert an optional `SystemTime` into FILETIME ticks, 0 when unknown or
/// before the Unix epoch.
#[cfg(not(windows))]
pub fn to_ticks(time: Option<std::time::SystemTime>) -> u64 {
    use std::time::UNIX_EPOCH;
    match time.and_then(|time| time.duration_since(UNIX_EPOCH).ok()) {
        Some(since_epoch) => UNIX_EPOCH_TICKS + (since_epoch.as_nanos() / 100) as u64,
        None => 0,
    }
}
