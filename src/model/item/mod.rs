use crate::util::format::{Unit, format_size, group_digits};
use crate::platform;
use crate::model::scan::{DirNode, FileEntry, SubDir, Times};
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const ATTR_READONLY: u32 = 0x1;
pub const ATTR_HIDDEN: u32 = 0x2;
pub const ATTR_SYSTEM: u32 = 0x4;
pub const ATTR_DIRECTORY: u32 = 0x10;
pub const ATTR_ARCHIVE: u32 = 0x20;
pub const ATTR_REPARSE_POINT: u32 = 0x400;
pub const ATTR_COMPRESSED: u32 = 0x800;
pub const ATTR_ENCRYPTED: u32 = 0x4000;

pub const HASH_NAMES: [&str; 4] = ["MD5", "SHA-1", "SHA-256", "SHA-512"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemState {
    Ready,
    Pending,
    Partial,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub name: Box<str>,
    pub native_name: Option<Box<OsStr>>,
    pub parent: Arc<Path>,
    pub is_dir: bool,
    pub attrs: u32,
    pub times: Times,
    pub size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub state: ItemState,
    pub hashes: Option<Box<[String; 4]>>,
}

fn split_name(name: &OsStr) -> (Box<str>, Option<Box<OsStr>>) {
    match name.to_str() {
        Some(text) => (text.into(), None),
        None => (name.to_string_lossy().into(), Some(name.into())),
    }
}

impl Item {
    pub fn pending_folder(dir: &SubDir, parent: &Arc<Path>) -> Self {
        let (name, native_name) = split_name(&dir.name);
        Self {
            name,
            native_name,
            parent: parent.clone(),
            is_dir: true,
            attrs: dir.attrs,
            times: dir.times,
            size: 0,
            file_count: 0,
            dir_count: 0,
            state: ItemState::Pending,
            hashes: None,
        }
    }

    pub fn file(name: &OsStr, entry: &FileEntry, parent: &Arc<Path>) -> Self {
        let (name, native_name) = split_name(name);
        Self {
            name,
            native_name,
            parent: parent.clone(),
            is_dir: entry.attrs & ATTR_DIRECTORY != 0,
            attrs: entry.attrs,
            times: entry.times,
            size: entry.size,
            file_count: 0,
            dir_count: 0,
            state: ItemState::Ready,
            hashes: None,
        }
    }

    pub fn folder(node: &DirNode, parent: &Arc<Path>) -> Self {
        let (name, native_name) = split_name(&node.name);
        let mut item = Self {
            name,
            native_name,
            parent: parent.clone(),
            is_dir: true,
            attrs: node.attrs,
            times: node.times,
            size: 0,
            file_count: 0,
            dir_count: 0,
            state: ItemState::Pending,
            hashes: None,
        };
        item.apply_totals(node);
        item
    }

    pub fn apply_totals(&mut self, node: &DirNode) {
        self.size = node.size;
        self.file_count = node.file_count;
        self.dir_count = node.dir_count;
        self.state = match (node.complete, node.error_count) {
            (false, _) => ItemState::Cancelled,
            (true, 0) => ItemState::Ready,
            (true, _) => ItemState::Partial,
        };
    }

    pub fn path(&self) -> PathBuf {
        self.parent.join(self.native_name.as_deref().unwrap_or(OsStr::new(&*self.name)))
    }

    pub fn is_hidden(&self) -> bool {
        self.attrs & ATTR_HIDDEN != 0
    }

    fn extension(&self) -> &str {
        match self.name.rsplit_once('.') {
            Some((stem, extension)) if !self.is_dir && !stem.is_empty() => extension,
            _ => "",
        }
    }

    pub fn type_label(&self) -> String {
        match (self.is_dir, self.extension()) {
            (true, _) => "Folder".into(),
            (false, "") => "File".into(),
            (false, extension) => format!("{} File", extension.to_ascii_uppercase()),
        }
    }

    fn status_label(&self) -> &'static str {
        match self.state {
            ItemState::Ready => "OK",
            ItemState::Pending => "Scanning",
            ItemState::Partial => "Partial",
            ItemState::Cancelled => "Cancelled",
        }
    }

    fn size_label(&self, unit: Unit) -> String {
        match self.state {
            ItemState::Pending if self.size == 0 => "…".into(),
            ItemState::Pending | ItemState::Partial | ItemState::Cancelled => {
                format!("≥ {}", format_size(self.size, unit))
            }
            ItemState::Ready => format_size(self.size, unit),
        }
    }

    fn count_label(&self, count: u64) -> String {
        match (self.is_dir, self.state) {
            (false, _) => String::new(),
            (true, ItemState::Pending) if self.size == 0 => "…".into(),
            (true, _) => group_digits(count),
        }
    }

    fn attributes_label(&self) -> String {
        [
            (ATTR_READONLY, 'R'),
            (ATTR_HIDDEN, 'H'),
            (ATTR_SYSTEM, 'S'),
            (ATTR_ARCHIVE, 'A'),
            (ATTR_COMPRESSED, 'C'),
            (ATTR_ENCRYPTED, 'E'),
            (ATTR_REPARSE_POINT, 'L'),
        ]
        .iter()
        .filter(|(flag, _)| self.attrs & flag != 0)
        .map(|(_, letter)| letter)
        .collect()
    }

    fn hash_label(&self, slot: usize) -> String {
        match &self.hashes {
            Some(hashes) => hashes[slot].clone(),
            None if self.is_dir => String::new(),
            None => "…".into(),
        }
    }
}

pub fn compare_text(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        a.bytes().map(|byte| byte.to_ascii_lowercase()).cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
    } else {
        a.to_lowercase().cmp(&b.to_lowercase())
    }
}

mod column;
pub use column::*;

#[cfg(test)]
mod tests;
