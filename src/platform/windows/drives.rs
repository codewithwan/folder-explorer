//! Drives, disk usage and the Quick Access pinned folders.

use super::{Drive, DiskUsage, QUICK_ACCESS, wide};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::thread;
use windows::Win32::Storage::EnhancedStorage::PKEY_Home_IsPinned;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::SystemServices::SFGAO_FOLDER;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, IEnumShellItems, IShellItem, IShellItem2, SHCreateItemFromParsingName, SIGDN, SIGDN_FILESYSPATH,
    SIGDN_NORMALDISPLAY,
};
use windows::core::{HSTRING, Interface};
use windows_sys::Win32::Foundation::MAX_PATH;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;

pub fn disk_usage(root: &Path) -> Option<DiskUsage> {
    let path = wide(root);
    let (mut total, mut free) = (0, 0);
    let found = unsafe { GetDiskFreeSpaceExW(path.as_ptr(), null_mut(), &mut total, &mut free) };
    (found != 0).then_some(DiskUsage { total, free })
}

fn volume_label(root: &[u16]) -> Option<String> {
    let mut label = [0u16; MAX_PATH as usize + 1];
    let found = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            0,
        )
    };
    let length = label.iter().position(|unit| *unit == 0)?;
    (found != 0 && length > 0).then(|| String::from_utf16_lossy(&label[..length]))
}

fn describe_drive(letter: char) -> Drive {
    let root = PathBuf::from(format!("{letter}:\\"));
    let path = wide(&root);
    let drive_type = unsafe { GetDriveTypeW(path.as_ptr()) };
    let fallback = match drive_type {
        DRIVE_REMOVABLE => "Removable Disk",
        DRIVE_REMOTE => "Network Drive",
        DRIVE_CDROM => "Optical Drive",
        _ => "Local Disk",
    };
    let label = volume_label(&path).unwrap_or_else(|| fallback.into());
    Drive {
        title: format!("{label} ({letter}:)"),
        usage: disk_usage(&root),
        root,
        network: drive_type == DRIVE_REMOTE,
    }
}

unsafe fn enumerate_network_shortcuts() -> windows::core::Result<Vec<Drive>> {
    unsafe {
        let nethood: IShellItem = SHCreateItemFromParsingName(&HSTRING::from("shell:NetHood"), None)?;
        let items: IEnumShellItems = nethood.BindToHandler(None, &BHID_EnumItems)?;
        let next = || {
            let mut item = [None];
            items.Next(&mut item, None).ok()?;
            item[0].take()
        };
        Ok(std::iter::from_fn(next)
            .filter_map(|item| {
                let root = PathBuf::from(shell_name(&item, SIGDN_FILESYSPATH)?);
                let title = shell_name(&item, SIGDN_NORMALDISPLAY)?;
                Some(Drive { usage: disk_usage(&root), root, title, network: true })
            })
            .collect())
    }
}

pub fn drives() -> Vec<Drive> {
    let mask = unsafe { GetLogicalDrives() };
    let letters: Vec<char> = ('A'..='Z').enumerate().filter(|(bit, _)| mask >> bit & 1 == 1).map(|(_, letter)| letter).collect();
    let mut list: Vec<Drive> = thread::scope(|scope| {
        let queries: Vec<_> = letters.iter().map(|letter| scope.spawn(|| describe_drive(*letter))).collect();
        queries.into_iter().filter_map(|query| query.join().ok()).collect()
    });
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        if let Ok(shortcuts) = enumerate_network_shortcuts() {
            for shortcut in shortcuts {
                if !list.iter().any(|drive| drive.root == shortcut.root) {
                    list.push(shortcut);
                }
            }
        }
        if initialized {
            CoUninitialize();
        }
    }
    list
}

unsafe fn shell_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    unsafe {
        let name = item.GetDisplayName(kind).ok()?;
        let text = name.to_string().ok();
        CoTaskMemFree(Some(name.0.cast()));
        text
    }
}

unsafe fn pinned_folder(item: &IShellItem) -> Option<(PathBuf, String)> {
    unsafe {
        let properties: IShellItem2 = item.cast().ok()?;
        let pinned = properties.GetBool(&PKEY_Home_IsPinned).ok()?.as_bool();
        if !pinned || item.GetAttributes(SFGAO_FOLDER).ok()? != SFGAO_FOLDER {
            return None;
        }
        Some((PathBuf::from(shell_name(item, SIGDN_FILESYSPATH)?), shell_name(item, SIGDN_NORMALDISPLAY)?))
    }
}

pub(super) unsafe fn enumerate_pins() -> windows::core::Result<Vec<(PathBuf, String)>> {
    unsafe {
        let quick_access: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(QUICK_ACCESS), None)?;
        let items: IEnumShellItems = quick_access.BindToHandler(None, &BHID_EnumItems)?;
        let next = || {
            let mut item = [None];
            items.Next(&mut item, None).ok()?;
            item[0].take()
        };
        Ok(std::iter::from_fn(next).filter_map(|item| pinned_folder(&item)).collect())
    }
}

pub fn quick_access_pins() -> Vec<(PathBuf, String)> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let pins = enumerate_pins().unwrap_or_default();
        if initialized {
            CoUninitialize();
        }
        pins
    }
}
