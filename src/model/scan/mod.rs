use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
const MAX_LOGGED_ERRORS: usize = 10_000;
const ATTR_DIRECTORY: u32 = 0x10;
const ATTR_REPARSE_POINT: u32 = 0x400;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Times {
    pub modified: u64,
    pub created: u64,
    pub accessed: u64,
}

impl Times {
    #[cfg(windows)]
    fn of(meta: &fs::Metadata) -> Self {
        use std::os::windows::fs::MetadataExt;
        Self {
            modified: meta.last_write_time(),
            created: meta.creation_time(),
            accessed: meta.last_access_time(),
        }
    }

    #[cfg(not(windows))]
    fn of(meta: &fs::Metadata) -> Self {
        use crate::util::fsmeta::to_ticks;
        Self {
            modified: to_ticks(meta.modified().ok()),
            created: to_ticks(meta.created().ok()),
            accessed: to_ticks(meta.accessed().ok()),
        }
    }
}

#[derive(Clone)]
pub struct FileEntry {
    name_end: u32,
    pub attrs: u32,
    pub size: u64,
    pub times: Times,
}

impl FileEntry {
    pub fn is_folder_link(&self) -> bool {
        self.attrs & ATTR_DIRECTORY != 0
    }
}

pub struct DirNode {
    pub name: Box<OsStr>,
    pub attrs: u32,
    pub times: Times,
    pub size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub error_count: u64,
    pub complete: bool,
    pub children: Vec<Arc<DirNode>>,
    pub entries: Vec<FileEntry>,
    names: Vec<u8>,
    errors: Vec<Box<str>>,
}

impl DirNode {
    fn new(dir: &SubDir) -> Self {
        Self {
            name: dir.name.clone(),
            attrs: dir.attrs,
            times: dir.times,
            size: 0,
            file_count: 0,
            dir_count: 0,
            error_count: 0,
            complete: true,
            children: Vec::new(),
            entries: Vec::new(),
            names: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn push_entry(&mut self, name: &OsStr, meta: &fs::Metadata) -> u64 {
        self.names.extend_from_slice(name.as_encoded_bytes());
        let size = crate::util::fsmeta::size(meta);
        self.entries.push(FileEntry {
            name_end: self.names.len() as u32,
            attrs: crate::util::fsmeta::attrs(meta, name),
            size,
            times: Times::of(meta),
        });
        size
    }

    fn add_file(&mut self, name: &OsStr, meta: &fs::Metadata) {
        self.size += self.push_entry(name, meta);
        self.file_count += 1;
    }

    fn add_link(&mut self, name: &OsStr, meta: &fs::Metadata) {
        self.push_entry(name, meta);
    }

    fn record_error(&mut self, error: &io::Error) {
        let text = error.to_string();
        self.errors.push(text.split(" (os error").next().unwrap_or(&text).into());
        self.error_count += 1;
    }

    pub fn collect_errors(&self, path: &Path, into: &mut Vec<ScanError>) {
        let room = MAX_LOGGED_ERRORS.saturating_sub(into.len());
        into.extend(
            self.errors.iter().take(room).map(|message| ScanError { path: path.to_path_buf(), message: message.to_string() }),
        );
        for child in self.children.iter().filter(|child| child.error_count > 0) {
            child.collect_errors(&path.join(&*child.name), into);
        }
    }

    pub fn absorb(&mut self, children: Vec<Arc<DirNode>>) {
        for child in &children {
            self.size += child.size;
            self.file_count += child.file_count;
            self.dir_count += child.dir_count + 1;
            self.error_count += child.error_count;
            self.complete &= child.complete;
        }
        self.children = children;
    }

    pub fn files(&self) -> impl Iterator<Item = (&OsStr, &FileEntry)> {
        let mut start = 0;
        self.entries.iter().map(move |entry| {
            let end = entry.name_end as usize;
            let name = unsafe { OsStr::from_encoded_bytes_unchecked(&self.names[start..end]) };
            start = end;
            (name, entry)
        })
    }

    fn renamed(&self, name: &OsStr) -> Self {
        Self {
            name: name.into(),
            attrs: self.attrs,
            times: self.times,
            size: self.size,
            file_count: self.file_count,
            dir_count: self.dir_count,
            error_count: self.error_count,
            complete: self.complete,
            children: self.children.clone(),
            entries: self.entries.clone(),
            names: self.names.clone(),
            errors: self.errors.clone(),
        }
    }

    fn child(&self, name: &OsStr) -> Option<&Arc<DirNode>> {
        self.children.iter().find(|child| &*child.name == name)
    }
}

#[derive(Clone)]
pub struct SubDir {
    pub name: Box<OsStr>,
    pub path: PathBuf,
    pub attrs: u32,
    pub times: Times,
}

impl SubDir {
    pub fn at(path: &Path) -> io::Result<Self> {
        let meta = fs::metadata(path)?;
        if !meta.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotADirectory, "Not a folder"));
        }
        let name = path.file_name().unwrap_or(path.as_os_str());
        Ok(Self {
            name: name.into(),
            path: path.to_path_buf(),
            attrs: crate::util::fsmeta::attrs(&meta, name),
            times: Times::of(&meta),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

pub struct Level {
    pub node: DirNode,
    pub subdirs: Vec<SubDir>,
}


mod engine;
pub use engine::*;

#[cfg(test)]
pub mod tests;
