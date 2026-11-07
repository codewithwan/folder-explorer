//! Clipboard. macOS has no simple dependency-free way to put real file
//! references on the pasteboard for Finder, so cut/copy/paste works within the
//! app via a process-global store; text and paths are mirrored to the system
//! text clipboard through `arboard`.

use super::ClipboardFiles;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

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

pub fn clipboard_files() -> Option<ClipboardFiles> {
    let guard = CLIPBOARD.lock().unwrap();
    let (paths, is_cut) = guard.as_ref()?;
    Some(ClipboardFiles { paths: paths.clone(), is_cut: *is_cut, sequence: SEQUENCE.load(Ordering::SeqCst) })
}
