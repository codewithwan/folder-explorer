use super::*;
use crate::model::item::HASH_NAMES;
use crate::model::scan::tests::{TempTree, scan};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Key, Platform, WindowAdapter, WindowEvent, set_platform};

struct Headless;

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
    }
}

fn headless_app() -> (AppWindow, Shared) {
    let _ = set_platform(Box::new(Headless));
    let ui = AppWindow::new().unwrap();
    let shared: Shared =
        Rc::new_cyclic(|me| RefCell::new(App::new(&ui, me.clone(), Settings::default(), IconStore::new())));
    (ui, shared)
}

fn items_in(tree: &TempTree) -> Vec<Item> {
    let parent: Arc<Path> = tree.0.as_path().into();
    scan(&tree.0).files().map(|(name, entry)| Item::file(name, entry, &parent)).collect()
}

fn choice_of(column: Column) -> usize {
    Column::ALL.iter().filter(|known| !known.is_fixed()).position(|known| *known == column).unwrap()
}

fn summary_row() -> SummaryEntry {
    SummaryEntry { label: "old".into(), files: 1, size: 1, filter: Query::default(), folder: None }
}

fn md5(text: &str) -> Option<[String; 4]> {
    Some([text.into(), String::new(), String::new(), String::new()])
}

fn press(ui: &AppWindow, keys: &[SharedString]) {
    for key in keys {
        ui.window().dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
    }
    for key in keys.iter().rev() {
        ui.window().dispatch_event(WindowEvent::KeyReleased { text: key.clone() });
    }
}

#[test]
fn shortcuts_reach_their_actions() {
    let (ui, _shared) = headless_app();
    ui.show().unwrap();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let sink = actions.clone();
    ui.on_action(move |action| sink.borrow_mut().push(action.to_string()));
    let [shift, control, alt] = [Key::Shift, Key::Control, Key::Alt].map(SharedString::from);
    let cases: [(&[SharedString], &str); 8] = [
        (&[Key::Delete.into()], "delete"),
        (&[shift.clone(), Key::Delete.into()], "delete-permanently"),
        (&[control.clone(), "d".into()], "delete"),
        (&[alt, Key::Return.into()], "properties"),
        (&[control.clone(), "r".into()], "refresh"),
        (&[control.clone(), shift.clone(), "C".into()], "copy-path"),
        (&[control, "c".into()], "copy"),
        (&[shift, Key::F10.into()], "context-menu"),
    ];
    for (keys, action) in cases {
        press(&ui, keys);
        assert_eq!(actions.borrow_mut().drain(..).collect::<Vec<_>>(), [action], "{keys:?}");
    }
}

#[test]
fn typing_selects_by_name_and_pausing_starts_over() {
    let tree = TempTree::new();
    tree.file("alpha.txt", 1).file("beta.txt", 1).file("bravo.txt", 1);
    let (ui, shared) = headless_app();
    wire(&ui, &shared);
    ui.show().unwrap();
    shared.borrow().list.set_items(items_in(&tree));
    let selected = || shared.borrow().first_selected().unwrap();
    press(&ui, &["b".into()]);
    press(&ui, &["r".into()]);
    assert_eq!(selected(), tree.0.join("bravo.txt"));
    shared.borrow_mut().typed_at -= TYPE_AHEAD_PAUSE * 2;
    press(&ui, &["a".into()]);
    assert_eq!(selected(), tree.0.join("alpha.txt"));
}

#[test]
fn special_keys_never_count_as_typed_text() {
    for key in [Key::Tab, Key::Shift, Key::F5, Key::Menu, Key::UpArrow] {
        assert!(!is_name_text(&SharedString::from(key)), "{key:?}");
    }
    assert!(is_name_text("é") && !is_name_text(""));
}

#[test]
fn a_summary_requested_before_navigating_is_dropped() {
    let (_ui, shared) = headless_app();
    let mut app = shared.borrow_mut();
    let stale = app.summary_id;
    app.show_location();
    app.handle(Message::Summary { id: stale, entries: vec![summary_row()] });
    assert!(app.summary.is_empty());
    let current = app.summary_id;
    app.handle(Message::Summary { id: current, entries: vec![summary_row()] });
    assert_eq!(app.summary.len(), 1);
}

#[test]
fn clearing_the_search_drops_results_still_on_their_way() {
    let (_ui, shared) = headless_app();
    let mut app = shared.borrow_mut();
    app.active_query = Some(Query::new("report"));
    let stale = app.search_id;
    app.apply_search();
    app.handle(Message::Found { search: stale, items: Vec::new(), seconds: 0.0 });
    assert!(!app.showing_results);
}

#[test]
fn hiding_the_last_checksum_column_abandons_the_run() {
    let tree = TempTree::new();
    tree.file("a.txt", 1);
    let (_ui, shared) = headless_app();
    let mut app = shared.borrow_mut();
    app.list.set_items(items_in(&tree));
    app.toggle_column(choice_of(Column::Md5));
    let stale = app.hash_run;
    assert_eq!(app.hashes_left, 1);
    app.toggle_column(choice_of(Column::Md5));
    assert_eq!(app.hashes_left, 0);
    app.handle(Message::Hashed { run: stale, index: 0, hashes: md5("old") });
    assert!(app.list.state().items[0].hashes.is_none());
}

#[test]
fn cancelling_checksums_restores_the_status_text() {
    let (ui, shared) = headless_app();
    let mut app = shared.borrow_mut();
    app.activity = "Scanned 3 files".into();
    ui.set_status_activity("Calculating checksums · 2 files left".into());
    app.hashes_left = 2;
    app.cancel_hashing();
    assert_eq!(ui.get_status_activity(), "Scanned 3 files");
}

#[cfg(windows)]
#[test]
fn a_checksum_sort_settles_when_the_last_hash_arrives() {
    let tree = TempTree::new();
    tree.file("a.txt", 1).file("b.txt", 1);
    let (_ui, shared) = headless_app();
    let mut app = shared.borrow_mut();
    app.list.set_items(items_in(&tree));
    app.list.update(|state| {
        state.sort = Column::Md5;
        state.rebuild();
    });
    let run = app.hash_run;
    app.hashes_left = 2;
    app.handle(Message::Hashed { run, index: 0, hashes: md5("ff") });
    app.handle(Message::Hashed { run, index: 1, hashes: md5("00") });
    let names: Vec<String> = app.list.state().visible_items().map(|item| item.name.to_string()).collect();
    assert_eq!(names, ["b.txt", "a.txt"]);
}

#[cfg(windows)]
#[test]
fn breadcrumb_text_maps_back_to_the_exact_folder() {
    let parent = Path::new(r"C:\data").join(non_unicode(&[0x61, 0xD800]));
    let location = Location::Folder(parent.join("inner"));
    assert_eq!(resolve_target(&location, &parent.to_string_lossy()), parent);
    assert_eq!(resolve_target(&location, r"D:\other"), PathBuf::from(r"D:\other"));
}

#[test]
fn size_filters_survive_extreme_input() {
    assert_eq!(size_range("1", true), Some(1_048_577..u64::MAX));
    assert_eq!(size_range(" 1 ", false), Some(0..1_048_576));
    assert_eq!(size_range("1e300", true), Some(u64::MAX..u64::MAX));
    assert_eq!(size_range("1e300", false), Some(0..u64::MAX));
    for rejected in ["-1", "NaN", "inf", "abc", ""] {
        assert_eq!(size_range(rejected, true), None, "{rejected}");
    }
}

#[test]
fn hash_names_cover_every_checksum_column() {
    let titles: Vec<_> = Column::ALL.iter().filter(|column| column.hash_slot().is_some()).map(|column| column.title()).collect();
    assert_eq!(titles, HASH_NAMES);
}

#[cfg(windows)]
#[test]
fn typed_paths_are_normalized() {
    assert_eq!(normalize(r"C:\Windows\"), PathBuf::from(r"C:\Windows"));
    assert_eq!(normalize("C:"), PathBuf::from(r"C:\"));
    assert_eq!(normalize("\"C:\\Program Files\""), PathBuf::from(r"C:\Program Files"));
    assert_eq!(normalize("C:/Windows/System32"), PathBuf::from(r"C:\Windows\System32"));
}

#[cfg(windows)]
#[test]
fn crumbs_walk_from_this_pc_to_the_folder() {
    let crumbs = crumbs_for(&Location::Folder(PathBuf::from(r"C:\Users\me")));
    let names: Vec<_> = crumbs.iter().map(|crumb| crumb.name.as_str()).collect();
    let targets: Vec<_> = crumbs.iter().map(|crumb| crumb.target.as_str()).collect();
    assert_eq!(names, ["This PC", "C:", "Users", "me"]);
    assert_eq!(targets, ["", r"C:\", r"C:\Users", r"C:\Users\me"]);
}

#[cfg(windows)]
#[test]
fn roots_come_from_the_path() {
    assert_eq!(root_of(Path::new(r"D:\a\b")), PathBuf::from(r"D:\"));
}

#[test]
fn missing_folders_read_plainly() {
    let error = io::Error::from(io::ErrorKind::NotFound);
    assert_eq!(describe_error(Path::new(r"C:\gone"), &error), r"C:\gone does not exist.");
}

/// A file name that is not valid Unicode: an unpaired surrogate on Windows,
/// raw invalid-UTF-8 bytes on Unix. Keeps distinct inputs distinct.
#[cfg(windows)]
fn non_unicode(units: &[u16]) -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    std::ffi::OsString::from_wide(units)
}