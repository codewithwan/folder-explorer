//! Clipboard: copy text, copy/cut files (Explorer-compatible CF_HDROP) and
//! reading files back off the clipboard.

use super::ClipboardFiles;
use super::wide;
use std::ffi::{OsStr, OsString};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::{copy_nonoverlapping, null_mut};
use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
use windows_sys::Win32::UI::Shell::{DROPFILES, DragQueryFileW, HDROP};

const DROP_EFFECT_COPY: u32 = 1;
const DROP_EFFECT_MOVE: u32 = 2;
const ALL_DROPPED_FILES: u32 = u32::MAX;

unsafe fn store_clipboard_entry(format: u32, bytes: &[u8]) -> bool {
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        if memory.is_null() {
            return false;
        }
        let target = GlobalLock(memory).cast::<u8>();
        if target.is_null() {
            GlobalFree(memory);
            return false;
        }
        copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        GlobalUnlock(memory);
        let stored = !SetClipboardData(format, memory).is_null();
        if !stored {
            GlobalFree(memory);
        }
        stored
    }
}

fn set_clipboard(entries: &[(u32, &[u8])]) -> bool {
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();
        let stored = entries.iter().all(|(format, bytes)| store_clipboard_entry(*format, bytes));
        CloseClipboard();
        stored
    }
}

fn wide_bytes(units: &[u16]) -> impl Iterator<Item = u8> {
    units.iter().flat_map(|unit| unit.to_le_bytes())
}

pub fn copy_text(text: impl AsRef<OsStr>) -> bool {
    let bytes: Vec<u8> = wide_bytes(&wide(text)).collect();
    set_clipboard(&[(CF_UNICODETEXT as u32, &bytes)])
}

fn drop_effect_format() -> u32 {
    unsafe { RegisterClipboardFormatW(wide("Preferred DropEffect").as_ptr()) }
}

fn put_files_on_clipboard(paths: &[PathBuf], drop_effect: u32) -> bool {
    let mut names: Vec<u16> = paths.iter().flat_map(wide).collect();
    names.push(0);
    let mut header: DROPFILES = unsafe { zeroed() };
    header.pFiles = size_of::<DROPFILES>() as u32;
    header.fWide = 1;
    let header_bytes = unsafe {
        std::slice::from_raw_parts((&raw const header).cast::<u8>(), size_of::<DROPFILES>())
    };
    let bytes: Vec<u8> = header_bytes.iter().copied().chain(wide_bytes(&names)).collect();
    set_clipboard(&[(CF_HDROP as u32, &bytes), (drop_effect_format(), &drop_effect.to_le_bytes())])
}

pub fn copy_files(paths: &[PathBuf]) -> bool {
    put_files_on_clipboard(paths, DROP_EFFECT_COPY)
}

pub fn cut_files(paths: &[PathBuf]) -> bool {
    put_files_on_clipboard(paths, DROP_EFFECT_MOVE)
}

pub fn clipboard_has_files() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP as u32) != 0 }
}

pub fn clipboard_sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn clear_clipboard_if_unchanged(sequence: u32) {
    unsafe {
        if OpenClipboard(null_mut()) != 0 {
            if GetClipboardSequenceNumber() == sequence {
                EmptyClipboard();
            }
            CloseClipboard();
        }
    }
}

unsafe fn dropped_paths(drop: HDROP) -> Vec<PathBuf> {
    unsafe {
        let count = DragQueryFileW(drop, ALL_DROPPED_FILES, null_mut(), 0);
        (0..count)
            .map(|index| {
                let length = DragQueryFileW(drop, index, null_mut(), 0) as usize;
                let mut name = vec![0u16; length + 1];
                DragQueryFileW(drop, index, name.as_mut_ptr(), name.len() as u32);
                name.truncate(length);
                PathBuf::from(OsString::from_wide(&name))
            })
            .collect()
    }
}

unsafe fn clipboard_drop_effect() -> u32 {
    unsafe {
        let memory = GetClipboardData(drop_effect_format());
        if memory.is_null() {
            return DROP_EFFECT_COPY;
        }
        let value = GlobalLock(memory).cast::<u32>();
        if value.is_null() {
            return DROP_EFFECT_COPY;
        }
        let effect = value.read_unaligned();
        GlobalUnlock(memory);
        effect
    }
}

pub fn clipboard_files() -> Option<ClipboardFiles> {
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return None;
        }
        let drop = GetClipboardData(CF_HDROP as u32);
        let files = (!drop.is_null()).then(|| {
            let paths = dropped_paths(drop);
            let is_cut = clipboard_drop_effect() & DROP_EFFECT_MOVE != 0;
            ClipboardFiles { paths, is_cut, sequence: GetClipboardSequenceNumber() }
        });
        CloseClipboard();
        files
    }
}
