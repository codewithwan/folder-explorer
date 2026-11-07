use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use super::*;
use std::process::Command;
use std::sync::atomic::AtomicUsize;

pub struct TempTree(pub PathBuf);

impl TempTree {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = format!("folder-explorer-{}-{}", std::process::id(), NEXT.fetch_add(1, Relaxed));
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    pub fn file(&self, relative: &str, size: usize) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; size]).unwrap();
        self
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = Command::new("cmd").args(["/c", "rmdir", "/s", "/q"]).arg(&self.0).output();
    }
}

fn context() -> ScanContext {
    ScanContext { cancel: Arc::default(), known: HashMap::new() }
}

pub fn scan(path: &Path) -> Arc<DirNode> {
    scan_tree(&SubDir::at(path).unwrap(), &context(), &Live::default())
}

fn sample() -> TempTree {
    let tree = TempTree::new();
    tree.file("a.txt", 10).file("sub/b.txt", 20).file("sub/deep/c.log", 30).file("other/d.bin", 40);
    tree
}

#[test]
fn totals_cover_the_whole_tree() {
    let tree = sample();
    let node = scan(&tree.0);
    assert_eq!((node.size, node.file_count, node.dir_count), (100, 4, 3));
    assert!(node.complete);
}

#[test]
fn file_names_survive_the_packed_storage() {
    let tree = TempTree::new();
    tree.file("one.txt", 1).file("two words.txt", 2).file("日本語.txt", 3);
    let node = scan(&tree.0);
    let mut names: Vec<_> = node.files().map(|(name, entry)| (name.to_string_lossy().into_owned(), entry.size)).collect();
    names.sort();
    assert_eq!(
        names,
        [("one.txt".into(), 1), ("two words.txt".into(), 2), ("日本語.txt".into(), 3)]
    );
}

#[cfg(windows)]
#[test]
fn names_that_are_not_valid_unicode_stay_distinct() {
    let tree = TempTree::new();
    let names = [non_unicode(&[0x61, 0xD800]), non_unicode(&[0x61, 0xD801])];
    for (index, name) in names.iter().enumerate() {
        fs::write(tree.0.join(name), vec![0; index + 1]).unwrap();
    }
    let node = scan(&tree.0);
    let mut found: Vec<_> = node.files().map(|(name, entry)| (name.to_os_string(), entry.size)).collect();
    found.sort();
    assert_eq!(found, [(names[0].clone(), 1), (names[1].clone(), 2)]);
}

#[cfg(windows)]
#[test]
fn neighbouring_names_do_not_merge_their_surrogates() {
    let tree = TempTree::new();
    let names = [non_unicode(&[0x61, 0xD83D]), non_unicode(&[0xDE00, 0x62])];
    for name in &names {
        fs::write(tree.0.join(name), [0]).unwrap();
    }
    let node = scan(&tree.0);
    let mut found: Vec<_> = node.files().map(|(name, _)| name.to_os_string()).collect();
    found.sort();
    assert_eq!(found, names);
}

#[cfg(windows)]
#[test]
fn folders_with_invalid_names_are_found_in_the_cache() {
    let tree = TempTree::new();
    let folder = tree.0.join(non_unicode(&[0x61, 0xD800]));
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("inside.txt"), [0; 3]).unwrap();
    let mut cache = Cache::default();
    cache.store(&tree.0, &scan(&tree.0));
    assert_eq!(cache.get(&folder).unwrap().size, 3);
}

#[cfg(windows)]
#[test]
fn junctions_are_listed_but_not_followed() {
    let tree = sample();
    let target = tree.0.join("sub");
    let junction = tree.0.join("link");
    let made = Command::new("cmd").args(["/c", "mklink", "/J"]).arg(&junction).arg(&target).output().unwrap();
    assert!(made.status.success());
    let node = scan(&tree.0);
    assert_eq!((node.size, node.file_count, node.dir_count), (100, 4, 3));
    assert!(node.files().any(|(name, _)| name == "link"));
}

#[test]
fn cancelled_scans_are_marked_incomplete() {
    let tree = sample();
    let context = ScanContext { cancel: Arc::new(AtomicBool::new(true)), ..context() };
    let node = scan_tree(&SubDir::at(&tree.0).unwrap(), &context, &Live::default());
    assert!(!node.complete);
}

#[test]
fn known_subtrees_are_reused_without_reading_them() {
    let tree = sample();
    let sub = tree.0.join("sub");
    let mut cache = Cache::default();
    cache.store(&sub, &scan(&sub));
    fs::remove_dir_all(&sub).unwrap();
    let context = ScanContext { known: cache.known_below(&tree.0), ..context() };
    let node = scan_tree(&SubDir::at(&tree.0).unwrap(), &context, &Live::default());
    assert_eq!(node.size, 50);
}

#[test]
fn cache_finds_folders_inside_a_stored_tree() {
    let tree = sample();
    let mut cache = Cache::default();
    cache.store(&tree.0, &scan(&tree.0));
    let deep = cache.get(&tree.0.join("sub").join("deep")).unwrap();
    assert_eq!(deep.size, 30);
    assert!(cache.get(&tree.0.join("missing")).is_none());
}

#[test]
fn invalidating_keeps_siblings_but_drops_the_path_and_its_ancestors() {
    let tree = sample();
    let mut cache = Cache::default();
    cache.store(&tree.0, &scan(&tree.0));
    cache.invalidate(&tree.0.join("sub").join("deep"));
    assert!(cache.get(&tree.0).is_none());
    assert!(cache.get(&tree.0.join("sub")).is_none());
    assert!(cache.get(&tree.0.join("sub").join("deep")).is_none());
    assert_eq!(cache.get(&tree.0.join("other")).unwrap().size, 40);
}

#[test]
fn invalidating_reaches_every_stale_ancestor() {
    let tree = sample();
    let sub = tree.0.join("sub");
    let mut cache = Cache::default();
    cache.store(&tree.0, &scan(&tree.0));
    cache.store(&sub, &scan(&sub));
    cache.invalidate(&sub.join("deep"));
    assert!(cache.get(&tree.0).is_none());
    assert!(cache.get(&sub).is_none());
}

#[test]
fn renaming_keeps_the_scanned_subtree_under_its_new_name() {
    let tree = sample();
    let mut cache = Cache::default();
    cache.store(&tree.0, &scan(&tree.0));
    let (from, to) = (tree.0.join("sub"), tree.0.join("renamed"));
    cache.rename(&from, &to);
    assert!(cache.get(&from).is_none());
    assert!(cache.get(&tree.0).is_none());
    let moved = cache.get(&to).unwrap();
    assert_eq!((&*moved.name, moved.size, moved.file_count), (OsStr::new("renamed"), 50, 2));
    assert_eq!(cache.get(&to.join("deep")).unwrap().size, 30);
    assert_eq!(moved.files().next().unwrap().0, "b.txt");
    assert_eq!(cache.get(&tree.0.join("other")).unwrap().size, 40);
}

#[test]
fn renaming_an_unscanned_path_only_drops_stale_ancestors() {
    let tree = sample();
    let mut cache = Cache::default();
    cache.store(&tree.0, &scan(&tree.0));
    cache.rename(&tree.0.join("a.txt"), &tree.0.join("b.txt"));
    assert!(cache.get(&tree.0).is_none());
    assert_eq!(cache.get(&tree.0.join("sub")).unwrap().size, 50);
}

fn subdir(name: &str) -> SubDir {
    SubDir { name: OsStr::new(name).into(), path: PathBuf::from(name), attrs: 0, times: Times::default() }
}

fn sized_folder(name: &str, size: u64) -> Arc<DirNode> {
    Arc::new(DirNode { size, ..DirNode::new(&subdir(name)) })
}

fn cached_folders(root: &Path, children: Vec<Arc<DirNode>>) -> Cache {
    let mut parent = DirNode::new(&subdir("root"));
    parent.absorb(children);
    let mut cache = Cache::default();
    cache.store(root, &Arc::new(parent));
    cache
}

#[test]
fn siblings_differing_only_by_case_stay_apart() {
    let root = PathBuf::from(r"C:\root");
    let cache = cached_folders(&root, vec![sized_folder("foo", 1), sized_folder("Foo", 2)]);
    assert_eq!(cache.get(&root.join("foo")).unwrap().size, 1);
    assert_eq!(cache.get(&root.join("Foo")).unwrap().size, 2);
    assert!(cache.get(&root.join("FOO")).is_none());
}

#[test]
fn invalidating_a_differently_cased_path_keeps_nothing_it_may_name() {
    let root = PathBuf::from(r"C:\root");
    let mut cache = cached_folders(&root, vec![sized_folder("foo", 1), sized_folder("other", 2)]);
    cache.invalidate(&root.join("FOO").join("inner"));
    assert!(cache.get(&root.join("foo")).is_none());
    assert_eq!(cache.get(&root.join("other")).unwrap().size, 2);
}

#[test]
fn scans_started_before_an_invalidation_are_not_stored() {
    let tree = sample();
    let mut cache = Cache::default();
    let started = cache.generation();
    let node = scan(&tree.0);
    cache.invalidate(&tree.0);
    cache.store_scan(started, &tree.0, &node);
    assert!(cache.get(&tree.0).is_none());
}

#[test]
fn scans_with_no_invalidation_since_they_started_are_stored() {
    let tree = sample();
    let mut cache = Cache::default();
    cache.invalidate(&tree.0);
    cache.store_scan(cache.generation(), &tree.0, &scan(&tree.0));
    assert_eq!(cache.get(&tree.0).unwrap().size, 100);
}

#[test]
fn reused_subtrees_keep_their_error_messages() {
    let tree = sample();
    let sub = tree.0.join("sub");
    let dir = SubDir::at(&sub).unwrap();
    fs::remove_dir_all(&sub).unwrap();
    let failed = scan_tree(&dir, &context(), &Live::default());
    fs::create_dir(&sub).unwrap();
    let mut cache = Cache::default();
    cache.store(&sub, &failed);
    let context = ScanContext { known: cache.known_below(&tree.0), ..context() };
    let node = scan_tree(&SubDir::at(&tree.0).unwrap(), &context, &Live::default());
    let mut errors = Vec::new();
    node.collect_errors(&tree.0, &mut errors);
    assert_eq!(node.error_count, 1);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, sub);
}

#[test]
fn logged_errors_stop_at_the_limit() {
    let mut node = DirNode::new(&subdir("root"));
    let error = io::Error::from(io::ErrorKind::NotFound);
    (0..MAX_LOGGED_ERRORS + 5).for_each(|_| node.record_error(&error));
    let mut errors = Vec::new();
    node.collect_errors(Path::new(r"C:\root"), &mut errors);
    assert_eq!(errors.len(), MAX_LOGGED_ERRORS);
}

#[test]
fn incomplete_trees_only_store_their_finished_parts() {
    let tree = sample();
    let finished = scan(&tree.0.join("other"));
    let root = SubDir::at(&tree.0).unwrap();
    let mut partial = DirNode::new(&root);
    let unfinished = Arc::new(DirNode { complete: false, ..DirNode::new(&SubDir::at(&tree.0.join("sub")).unwrap()) });
    partial.absorb(vec![finished, unfinished]);
    let mut cache = Cache::default();
    cache.store(&tree.0, &Arc::new(partial));
    assert!(cache.get(&tree.0).is_none());
    assert!(cache.get(&tree.0.join("sub")).is_none());
    assert_eq!(cache.get(&tree.0.join("other")).unwrap().size, 40);
}

/// A file name that is not valid Unicode: an unpaired surrogate on Windows,
/// raw invalid-UTF-8 bytes on Unix. Keeps distinct inputs distinct.
#[cfg(windows)]
fn non_unicode(units: &[u16]) -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    std::ffi::OsString::from_wide(units)
}