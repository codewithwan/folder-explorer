use super::image::{shell_image, to_rgba};
use super::*;
use crate::model::item::{ATTR_DIRECTORY, ItemState};
use crate::model::scan::Times;
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::SIIGBF_ICONONLY;

fn item(name: &str, is_dir: bool, attrs: u32) -> Item {
    Item {
        name: name.into(),
        native_name: None,
        parent: Arc::from(Path::new(r"C:\data")),
        is_dir,
        attrs,
        times: Times::default(),
        size: 0,
        file_count: 0,
        dir_count: 0,
        state: ItemState::Ready,
        hashes: None,
    }
}

fn own_path(item: &Item) -> Source {
    Source::Path(item.path(), item.times.modified)
}

#[test]
fn plain_folders_share_one_icon() {
    assert_eq!(item_source(&item("music", true, ATTR_DIRECTORY), false), Source::Folder);
}

#[test]
fn folders_that_may_be_customized_get_their_own_icon() {
    for attrs in [ATTR_READONLY, ATTR_SYSTEM, ATTR_REPARSE_POINT] {
        let folder = item("Documents", true, ATTR_DIRECTORY | attrs);
        assert_eq!(item_source(&folder, false), own_path(&folder));
    }
}

#[test]
fn files_share_an_icon_per_extension() {
    assert_eq!(item_source(&item("a.TXT", false, 0), false), Source::Extension("txt".into()));
    assert_eq!(item_source(&item("README", false, 0), false), Source::Extension(String::new()));
    assert_eq!(item_source(&item(".gitignore", false, 0), false), Source::Extension(String::new()));
}

#[test]
fn programs_and_shortcuts_keep_their_own_icon() {
    for name in ["setup.exe", "App.LNK", "favicon.ico"] {
        let file = item(name, false, 0);
        assert_eq!(item_source(&file, false), own_path(&file));
    }
}

#[test]
fn thumbnails_are_for_images_in_views_that_show_them() {
    assert!(wants_thumbnail("jpg", 0, true));
    assert!(!wants_thumbnail("jpg", 0, false));
    assert!(!wants_thumbnail("txt", 0, true));
}

#[test]
fn cloud_placeholders_are_not_downloaded_for_a_thumbnail() {
    assert!(!wants_thumbnail("png", ATTR_RECALL_ON_DATA_ACCESS, true));
    assert!(!wants_thumbnail("png", ATTR_OFFLINE, true));
}

#[test]
fn bitmaps_become_rgba() {
    let mut pixels = [10, 20, 30, 40];
    to_rgba(&mut pixels);
    assert_eq!(pixels, [30, 20, 10, 40]);
}

#[test]
fn bitmaps_without_any_alpha_become_opaque() {
    let mut pixels = [1, 2, 3, 0, 4, 5, 6, 0];
    to_rgba(&mut pixels);
    assert_eq!(pixels, [3, 2, 1, 255, 6, 5, 4, 255]);
}

#[test]
fn transparency_is_kept_when_any_pixel_uses_it() {
    let mut pixels = [1, 2, 3, 0, 4, 5, 6, 255];
    to_rgba(&mut pixels);
    assert_eq!(pixels, [3, 2, 1, 0, 6, 5, 4, 255]);
}

#[test]
fn the_shell_returns_a_real_folder_icon() {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let icon = shell_image(r"C:\Windows", 32, SIIGBF_ICONONLY).unwrap();
    assert_eq!((icon.width(), icon.height()), (32, 32));
    assert!(icon.as_bytes().chunks_exact(4).any(|pixel| pixel[3] != 0));
}

#[test]
fn icons_requested_at_the_same_moment_all_arrive() {
    let paths = [r"C:\Windows\System32", r"C:\Windows\Fonts", r"C:\Windows\regedit.exe", r"C:\Program Files"];
    let gate = Arc::new(Barrier::new(paths.len()));
    let loaders: Vec<_> = paths
        .into_iter()
        .map(|path| {
            let gate = gate.clone();
            thread::spawn(move || {
                let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                gate.wait();
                shell_image(path, 32, SIIGBF_ICONONLY).is_some()
            })
        })
        .collect();
    assert!(loaders.into_iter().all(|loader| loader.join().unwrap()));
}

#[test]
fn shell_bitmaps_use_straight_alpha() {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let icon = shell_image(r"C:\Windows\explorer.exe", 48, SIIGBF_ICONONLY).unwrap();
    let brighter_than_alpha = |pixel: &[u8]| pixel[..3].iter().any(|channel| *channel > pixel[3]);
    assert!(icon.as_bytes().chunks_exact(4).any(brighter_than_alpha));
}

#[test]
fn customized_folders_cannot_replace_the_shared_folder_icon() {
    let customized = r"C:\Users\me\Pictures";
    assert_eq!(shell_path(&Source::Folder, customized), PLAIN_FOLDER);
    assert_eq!(shell_path(&Source::Path(customized.into(), 0), customized), customized);
}

#[test]
fn the_shared_folder_icon_matches_a_plain_folder() {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let plain = std::env::temp_dir().join(format!("folder-explorer-plain-{}", std::process::id()));
    std::fs::create_dir_all(&plain).unwrap();
    let expected = shell_image(&plain.to_string_lossy(), 32, SIIGBF_ICONONLY);
    std::fs::remove_dir(&plain).unwrap();
    let shared = shell_image(PLAIN_FOLDER, 32, SIIGBF_ICONONLY).unwrap();
    assert_eq!(shared.as_bytes(), expected.unwrap().as_bytes());
}

#[test]
fn missing_files_have_no_image() {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    assert!(shell_image(r"C:\this\does\not\exist.png", 32, SIIGBF_ICONONLY).is_none());
}
