use super::*;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

fn is_link(entry: &fs::DirEntry, file_type: fs::FileType, attrs: u32) -> bool {
    file_type.is_symlink()
        || attrs & ATTR_REPARSE_POINT != 0
            && fs::symlink_metadata(entry.path()).is_ok_and(|meta| meta.file_type().is_symlink())
}

pub fn read_level(dir: &SubDir) -> Result<Level, io::Error> {
    let mut node = DirNode::new(dir);
    let mut subdirs = Vec::new();
    for entry in fs::read_dir(&dir.path)? {
        match entry.and_then(|entry| {
            let file_type = entry.file_type()?;
            Ok((entry.metadata()?, file_type, entry))
        }) {
            Ok((meta, file_type, entry)) => {
                let name = entry.file_name();
                let attrs = crate::util::fsmeta::attrs(&meta, &name);
                if is_link(&entry, file_type, attrs) {
                    node.add_link(&name, &meta);
                } else if file_type.is_dir() {
                    subdirs.push(SubDir {
                        name: name.into_boxed_os_str(),
                        path: entry.path(),
                        attrs,
                        times: Times::of(&meta),
                    });
                } else {
                    node.add_file(&name, &meta);
                }
            }
            Err(error) => node.record_error(&error),
        }
    }
    Ok(Level { node, subdirs })
}

#[derive(Default)]
pub struct Live {
    pub size: AtomicU64,
    pub files: AtomicU64,
    pub dirs: AtomicU64,
}

impl Live {
    fn add(&self, size: u64, files: u64, dirs: u64) {
        self.size.fetch_add(size, Relaxed);
        self.files.fetch_add(files, Relaxed);
        self.dirs.fetch_add(dirs, Relaxed);
    }
}

pub struct ScanContext {
    pub cancel: Arc<AtomicBool>,
    pub known: HashMap<PathBuf, Arc<DirNode>>,
}

pub fn scan_tree(dir: &SubDir, context: &ScanContext, live: &Live) -> Arc<DirNode> {
    if let Some(known) = context.known.get(&dir.path) {
        live.add(known.size, known.file_count, known.dir_count);
        return known.clone();
    }
    let mut unread = DirNode::new(dir);
    if context.cancel.load(Relaxed) {
        unread.complete = false;
        return Arc::new(unread);
    }
    let Level { mut node, subdirs } = match read_level(dir) {
        Ok(level) => level,
        Err(error) => {
            unread.record_error(&error);
            return Arc::new(unread);
        }
    };
    live.add(node.size, node.file_count, subdirs.len() as u64);
    let children = subdirs.par_iter().map(|subdir| scan_tree(subdir, context, live)).collect();
    node.absorb(children);
    Arc::new(node)
}

#[derive(Default)]
pub struct Cache {
    roots: HashMap<PathBuf, Arc<DirNode>>,
    generation: u64,
}

impl Cache {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn store_scan(&mut self, generation: u64, path: &Path, node: &Arc<DirNode>) {
        if generation == self.generation {
            self.store(path, node);
        }
    }

    pub fn store(&mut self, path: &Path, node: &Arc<DirNode>) {
        if node.complete {
            self.roots.insert(path.to_path_buf(), node.clone());
            return;
        }
        for child in &node.children {
            self.store(&path.join(&*child.name), child);
        }
    }

    pub fn get(&self, path: &Path) -> Option<Arc<DirNode>> {
        let (root_path, root) = path.ancestors().find_map(|at| self.roots.get_key_value(at))?;
        let below = path.strip_prefix(root_path).ok()?;
        below.iter().try_fold(root.clone(), |node, part| node.child(part).cloned())
    }

    pub fn known_below(&self, path: &Path) -> HashMap<PathBuf, Arc<DirNode>> {
        let mut known: HashMap<_, _> = self
            .roots
            .iter()
            .filter(|(root, _)| root.starts_with(path) && root.as_path() != path)
            .map(|(root, node)| (root.clone(), node.clone()))
            .collect();
        if let Some(node) = self.get(path) {
            for child in &node.children {
                known.insert(path.join(&*child.name), child.clone());
            }
        }
        known
    }

    pub fn invalidate(&mut self, path: &Path) -> Vec<Arc<DirNode>> {
        self.generation += 1;
        let mut retired = Vec::new();
        self.roots.retain(|root, node| {
            let keep = !root.starts_with(path);
            if !keep {
                retired.push(node.clone());
            }
            keep
        });
        for ancestor in path.ancestors().skip(1) {
            if let Some(root) = self.roots.remove(ancestor) {
                self.keep_siblings_of(path, ancestor, root.clone());
                retired.push(root);
            }
        }
        retired
    }

    pub fn rename(&mut self, from: &Path, to: &Path) -> Vec<Arc<DirNode>> {
        let moved = self.get(from);
        let retired = self.invalidate(from);
        if let (Some(node), Some(name)) = (moved, to.file_name()) {
            self.store(to, &Arc::new(node.renamed(name)));
        }
        retired
    }

    fn keep_siblings_of(&mut self, path: &Path, root_path: &Path, root: Arc<DirNode>) {
        let mut node = root;
        let mut at = root_path.to_path_buf();
        let below = path.strip_prefix(root_path).expect("root is an ancestor");
        for part in below {
            let mut next = None;
            for child in &node.children {
                if &*child.name == part {
                    next = Some(child.clone());
                } else if !child.name.eq_ignore_ascii_case(part) {
                    self.roots.insert(at.join(&*child.name), child.clone());
                }
            }
            at.push(part);
            match next {
                Some(child) => node = child,
                None => return,
            }
        }
    }
}

