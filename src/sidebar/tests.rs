use super::*;
use std::path::PathBuf;

fn drive(root: &str, title: &str) -> Drive {
    Drive {
        root: PathBuf::from(root),
        title: title.into(),
        usage: None,
        network: false,
    }
}

fn sidebar_with_pin(path: &str) -> Sidebar {
    let mut sidebar = Sidebar::new(IconStore::new(), TreeMemory::default());
    sidebar.set_drives(&[drive(r"C:\", "Acer (C:)")]);
    sidebar.set_pins(vec![(PathBuf::from(path), "Pinned".into())]);
    sidebar
}

fn two_drives() -> [Drive; 2] {
    [drive(r"C:\", "Acer (C:)"), drive(r"D:\", "Data (D:)")]
}

fn sidebar_with_two_drives(memory: TreeMemory) -> Sidebar {
    let mut sidebar = Sidebar::new(IconStore::new(), memory);
    sidebar.set_drives(&two_drives());
    sidebar
}

const AUTO_EXPAND_OFF: TreeMemory = TreeMemory { auto_expand_off: true, computer_closed: false, quick_access_closed: false };

#[cfg(windows)]
#[test]
fn a_manual_collapse_stops_auto_expanding_on_every_drive() {
    let mut sidebar = sidebar_with_two_drives(TreeMemory::default());
    assert_eq!(sidebar.reveal(Some(Path::new(r"C:\Users"))), Some(PathBuf::from(r"C:\")));
    sidebar.set_children(Path::new(r"C:\"), vec![("Users".into(), 0)]);
    assert_eq!(sidebar.toggle(1), None);
    assert_eq!(sidebar.reveal(Some(Path::new(r"D:\Games"))), None);
    assert!(sidebar.memory().auto_expand_off);
}

#[cfg(windows)]
#[test]
fn a_manual_expand_turns_auto_expanding_back_on() {
    let mut sidebar = sidebar_with_two_drives(AUTO_EXPAND_OFF);
    assert_eq!(sidebar.toggle(2), Some(PathBuf::from(r"D:\")));
    assert_eq!(sidebar.reveal(Some(Path::new(r"C:\Users"))), Some(PathBuf::from(r"C:\")));
    assert!(!sidebar.memory().auto_expand_off);
}

#[test]
fn a_remembered_collapse_still_applies_after_a_restart() {
    let mut sidebar = sidebar_with_two_drives(AUTO_EXPAND_OFF);
    assert_eq!(sidebar.reveal(Some(Path::new(r"C:\Users"))), None);
    assert_eq!(sidebar.model().row_count(), 3);
}

#[test]
fn remembered_sections_start_collapsed() {
    let memory = TreeMemory { auto_expand_off: true, computer_closed: true, quick_access_closed: true };
    let mut sidebar = sidebar_with_two_drives(memory);
    sidebar.set_pins(vec![(PathBuf::from(r"C:\Users"), "Users".into())]);
    assert_eq!(sidebar.model().row_count(), 2);
}

#[test]
fn collapsing_quick_access_leaves_auto_expanding_on() {
    let mut sidebar = sidebar_with_pin(r"C:\Users\Me");
    assert_eq!(sidebar.toggle(0), None);
    assert!(sidebar.memory().quick_access_closed);
    assert!(!sidebar.memory().auto_expand_off);
}

#[test]
fn returning_to_this_pc_keeps_the_tree_expanded() {
    let mut sidebar = sidebar_with_two_drives(TreeMemory::default());
    sidebar.reveal(Some(Path::new(r"C:\Users")));
    sidebar.set_children(Path::new(r"C:\"), vec![("Users".into(), 0)]);
    sidebar.reveal(None);
    assert_eq!(sidebar.set_drives(&two_drives()), None);
    assert!(sidebar.is_expanded(Path::new(r"C:\")));
    assert_eq!(sidebar.model().row_count(), 4);
}

#[test]
fn pins_sit_above_this_pc_and_only_they_open_a_folder() {
    let sidebar = sidebar_with_pin(r"C:\Users\Me");
    assert_eq!(sidebar.model().row_count(), 4);
    assert_eq!(sidebar.path_at(0), None);
    assert_eq!(sidebar.path_at(1), Some(Some(PathBuf::from(r"C:\Users\Me"))));
    assert_eq!(sidebar.path_at(2), Some(None));
    assert_eq!(sidebar.path_at(3), Some(Some(PathBuf::from(r"C:\"))));
}

#[test]
fn no_pins_means_no_quick_access_section() {
    let mut sidebar = sidebar_with_pin(r"C:\Users\Me");
    sidebar.set_pins(Vec::new());
    assert_eq!(sidebar.model().row_count(), 2);
    assert_eq!(sidebar.path_at(0), Some(None));
}

#[cfg(windows)]
#[test]
fn a_pin_does_not_stop_the_tree_expanding_to_the_same_folder() {
    let mut sidebar = sidebar_with_pin(r"C:\Users\Me");
    assert_eq!(sidebar.reveal(Some(Path::new(r"C:\Users\Me"))), Some(PathBuf::from(r"C:\")));
    assert_eq!(
        sidebar.set_children(Path::new(r"C:\"), vec![("Users".into(), 0)]),
        Some(PathBuf::from(r"C:\Users"))
    );
    assert_eq!(sidebar.set_children(Path::new(r"C:\Users"), vec![("Me".into(), 0)]), None);
    assert!(sidebar.is_expanded(Path::new(r"C:\Users")));
}

#[cfg(windows)]
#[test]
fn children_of_a_pinned_folder_go_under_the_tree_row_not_the_pin() {
    let mut sidebar = sidebar_with_pin(r"C:\Users");
    sidebar.reveal(Some(Path::new(r"C:\Users\Me")));
    sidebar.set_children(Path::new(r"C:\"), vec![("Users".into(), 0)]);
    sidebar.set_children(Path::new(r"C:\Users"), vec![("Me".into(), 0)]);
    assert!(!sidebar.nodes[1].expanded);
    assert_eq!(sidebar.nodes[2].kind, KIND_COMPUTER);
    assert_eq!(sidebar.nodes.last().unwrap().depth, 3);
}

#[cfg(windows)]
#[test]
fn a_pinned_folder_highlights_its_pin_and_a_subfolder_highlights_the_tree() {
    let mut sidebar = sidebar_with_pin(r"C:\Users\Me");
    sidebar.reveal(Some(Path::new(r"C:\Users\Me")));
    assert_eq!(sidebar.selected_row(), Some(1));
    sidebar.reveal(Some(Path::new(r"C:\Users\Me\Documents")));
    assert_eq!(sidebar.selected_row(), Some(3));
}

#[test]
fn quick_access_collapses_and_the_tree_keeps_working() {
    let mut sidebar = sidebar_with_pin(r"C:\Users\Me");
    assert_eq!(sidebar.toggle(0), None);
    assert_eq!(sidebar.model().row_count(), 3);
    assert_eq!(sidebar.path_at(1), Some(None));

    assert_eq!(sidebar.toggle(1), None);
    assert_eq!(sidebar.model().row_count(), 2);
    assert_eq!(sidebar.toggle(1), None);
    assert_eq!(sidebar.model().row_count(), 3);

    assert_eq!(sidebar.toggle(0), None);
    assert_eq!(sidebar.model().row_count(), 4);
    assert_eq!(sidebar.path_at(1), Some(Some(PathBuf::from(r"C:\Users\Me"))));
}

#[cfg(windows)]
#[test]
fn closed_branch_is_not_forced_open_when_navigating_inside_it() {
    let mut sidebar = Sidebar::new(IconStore::new(), TreeMemory::default());
    sidebar.set_drives(&[drive(r"C:\", "Acer (C:)")]);

    // Auto-expand when opening C:\Users\Me initially
    assert_eq!(sidebar.reveal(Some(Path::new(r"C:\Users\Me"))), Some(PathBuf::from(r"C:\")));
    assert_eq!(
        sidebar.set_children(Path::new(r"C:\"), vec![("Users".into(), 0)]),
        Some(PathBuf::from(r"C:\Users"))
    );
    assert_eq!(sidebar.set_children(Path::new(r"C:\Users"), vec![("Me".into(), 0)]), None);
    assert!(sidebar.is_expanded(Path::new(r"C:\")));
    assert!(sidebar.is_expanded(Path::new(r"C:\Users")));

    // User collapses C:\ in the sidebar
    assert_eq!(sidebar.toggle(1), None);
    assert!(!sidebar.is_expanded(Path::new(r"C:\")));

    // Opening another folder inside C:\ does NOT force C:\ to expand again
    assert_eq!(sidebar.reveal(Some(Path::new(r"C:\Users\Me\Documents"))), None);
    assert!(!sidebar.is_expanded(Path::new(r"C:\")));

    // Explicitly toggling C:\ open with the chevron re-enables expansion for C:\
    assert_eq!(sidebar.toggle(1), Some(PathBuf::from(r"C:\")));
}

#[test]
fn computer_root_can_be_collapsed_and_stays_closed_on_navigation() {
    let mut sidebar = Sidebar::new(IconStore::new(), TreeMemory::default());
    sidebar.set_drives(&[drive(r"C:\", "Acer (C:)")]);
    assert_eq!(sidebar.model().row_count(), 2);

    // Collapse "This PC"
    assert_eq!(sidebar.toggle(0), None);
    assert_eq!(sidebar.model().row_count(), 1);

    // Navigating to a folder does not force "This PC" open
    assert_eq!(sidebar.reveal(Some(Path::new(r"C:\Users"))), None);
    assert_eq!(sidebar.model().row_count(), 1);

    // Expanding "This PC" restores the drives
    assert_eq!(sidebar.toggle(0), None);
    assert_eq!(sidebar.model().row_count(), 2);
}
