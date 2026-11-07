use super::*;
use crate::model::item::ATTR_HIDDEN;
use crate::model::scan::Times;
use std::path::Path;
use std::sync::Arc;

fn item(name: &str, is_dir: bool, size: u64, attrs: u32) -> Item {
    Item {
        name: name.into(),
        native_name: None,
        parent: Arc::from(Path::new(r"C:\data")),
        is_dir,
        attrs,
        times: Times::default(),
        size,
        file_count: 0,
        dir_count: 0,
        state: ItemState::Ready,
        hashes: None,
    }
}

fn state(sort: Column, descending: bool, show_hidden: bool) -> ListState {
    let mut state = ListState::new(vec![Column::Name, Column::Size], Unit::Auto, sort, descending, show_hidden);
    state.set_items(vec![
        item("banana.txt", false, 300, 0),
        item("zoo", true, 100, 0),
        item("Apple.txt", false, 200, 0),
        item("archive", true, 900, 0),
        item("secret.txt", false, 50, ATTR_HIDDEN),
    ]);
    state
}

fn names(state: &ListState) -> Vec<&str> {
    state.visible_items().map(|item| &*item.name).collect()
}

#[test]
fn name_sort_keeps_folders_first_in_both_directions() {
    assert_eq!(names(&state(Column::Name, false, false)), ["archive", "zoo", "Apple.txt", "banana.txt"]);
    assert_eq!(names(&state(Column::Name, true, false)), ["zoo", "archive", "banana.txt", "Apple.txt"]);
}

#[test]
fn size_sort_mixes_files_and_folders() {
    assert_eq!(names(&state(Column::Size, true, false)), ["archive", "banana.txt", "Apple.txt", "zoo"]);
}

#[test]
fn hidden_items_appear_only_when_asked_for() {
    assert_eq!(state(Column::Name, false, false).row_count(), 4);
    assert_eq!(state(Column::Name, false, true).row_count(), 5);
}

#[test]
fn totals_only_count_visible_rows() {
    assert_eq!(state(Column::Name, false, false).total_size(), 1500);
    assert_eq!(state(Column::Name, false, true).total_size(), 1550);
}

#[test]
fn a_plain_press_replaces_the_selection() {
    let mut list = state(Column::Name, false, false);
    list.press(0, false, false);
    list.press(2, false, false);
    assert_eq!(list.selected_items().map(|item| &*item.name).collect::<Vec<_>>(), ["Apple.txt"]);
}

#[test]
fn control_toggles_single_rows() {
    let mut list = state(Column::Name, false, false);
    list.press(0, true, false);
    list.press(2, true, false);
    assert_eq!(list.selected_count(), 2);
    list.press(0, true, false);
    assert_eq!(list.selected_count(), 1);
}

#[test]
fn shift_selects_from_the_anchor_in_either_direction() {
    let mut list = state(Column::Name, false, false);
    list.press(1, false, false);
    list.press(3, false, true);
    assert_eq!(list.selected_count(), 3);
    list.press(0, false, true);
    assert_eq!(list.selected_count(), 2);
}

#[test]
fn hiding_the_anchor_row_keeps_shift_selection_inside_the_list() {
    let mut list = state(Column::Name, false, true);
    list.press(4, false, false);
    list.show_hidden = false;
    list.rebuild();
    list.press(0, false, true);
    assert_eq!(list.selected_count(), 4);
}

#[test]
fn the_cursor_stays_usable_after_its_row_is_hidden() {
    let mut list = state(Column::Name, false, true);
    list.press(4, false, false);
    list.show_hidden = false;
    list.rebuild();
    assert_eq!(list.move_cursor(-1, false), Some(2));
}

#[test]
fn the_anchor_follows_its_item_through_a_resort() {
    let mut list = state(Column::Name, false, false);
    list.press(1, false, false);
    list.sort = Column::Size;
    list.descending = true;
    list.rebuild();
    assert_eq!(names(&list), ["archive", "banana.txt", "Apple.txt", "zoo"]);
    list.press(1, false, true);
    let selected: Vec<_> = list.selected_items().map(|item| &*item.name).collect();
    assert_eq!(selected, ["banana.txt", "Apple.txt", "zoo"]);
}

#[test]
fn replacing_the_items_resets_the_anchor() {
    let mut list = state(Column::Name, false, false);
    list.press(3, false, false);
    list.set_items(vec![item("only.txt", false, 1, 0)]);
    list.press(0, false, true);
    assert_eq!(list.selected_count(), 1);
}

#[test]
fn select_all_skips_hidden_rows_and_inverse_flips() {
    let mut list = state(Column::Name, false, false);
    list.select_all();
    assert_eq!(list.selected_count(), 4);
    list.press(0, false, false);
    list.invert_selection();
    assert_eq!(list.selected_count(), 3);
}

#[test]
fn the_cursor_stops_at_the_ends() {
    let mut list = state(Column::Name, false, false);
    assert_eq!(list.move_cursor(-5, false), Some(0));
    assert_eq!(list.move_cursor(99, false), Some(3));
    assert_eq!(list.selected_count(), 1);
    assert!(list.is_row_selected(3));
}

#[test]
fn an_empty_list_has_no_cursor() {
    let mut list = ListState::new(vec![Column::Name], Unit::Auto, Column::Name, false, false);
    assert_eq!(list.move_cursor(1, false), None);
}

#[test]
fn pressing_past_the_end_is_ignored() {
    let mut list = state(Column::Name, false, false);
    list.press(50, false, false);
    assert_eq!(list.selected_count(), 0);
}

#[test]
fn the_only_selected_row_is_reported_only_when_alone() {
    let mut list = state(Column::Name, false, false);
    assert!(list.only_selected().is_none());
    list.press(2, false, false);
    assert_eq!(list.only_selected().map(|(row, item)| (row, &*item.name)), Some((2, "Apple.txt")));
    list.press(3, true, false);
    assert!(list.only_selected().is_none());
}

#[test]
fn items_can_be_selected_by_name() {
    let mut list = state(Column::Name, false, false);
    assert_eq!(list.select_name("zoo"), Some(1));
    assert!(list.is_row_selected(1));
    assert_eq!(list.select_name("missing"), None);
}

#[test]
fn a_typed_letter_starts_from_the_top_when_nothing_is_selected() {
    let mut list = state(Column::Name, false, false);
    assert_eq!(list.select_typed("a"), Some(0));
    assert!(list.is_row_selected(0));
}

#[test]
fn repeating_a_letter_cycles_through_its_matches_and_wraps() {
    let mut list = state(Column::Name, false, false);
    assert_eq!(list.select_typed("a"), Some(0));
    assert_eq!(list.select_typed("aa"), Some(2));
    assert_eq!(list.select_typed("aaa"), Some(0));
}

#[test]
fn typed_letters_narrow_by_prefix_ignoring_case() {
    let mut list = state(Column::Name, false, false);
    assert_eq!(list.select_typed("A"), Some(0));
    assert_eq!(list.select_typed("AP"), Some(2));
    assert_eq!(list.only_selected().map(|(_, item)| &*item.name), Some("Apple.txt"));
}

#[test]
fn a_failed_match_keeps_the_selection() {
    let mut list = state(Column::Name, false, false);
    list.press(1, false, false);
    assert_eq!(list.select_typed("q"), None);
    assert!(list.is_row_selected(1));
}

#[test]
fn typing_skips_hidden_rows_and_empty_lists() {
    let mut list = state(Column::Name, false, false);
    assert_eq!(list.select_typed("s"), None);
    let mut empty = ListState::new(vec![Column::Name], Unit::Auto, Column::Name, false, false);
    assert_eq!(empty.select_typed("a"), None);
}

#[cfg(windows)]
#[test]
fn cut_items_are_dimmed_and_clearing_the_set_restores_them() {
    let mut list = state(Column::Name, false, false);
    let cut: HashSet<PathBuf> = [PathBuf::from(r"C:\data\zoo")].into();
    list.mark_cut(&cut);
    assert!(list.file_row(1).unwrap().dim);
    assert!(!list.file_row(0).unwrap().dim);
    list.mark_cut(&HashSet::new());
    assert!(!list.file_row(1).unwrap().dim);
}

#[test]
fn tiles_describe_the_item_type() {
    let mut list = state(Column::Name, false, false);
    assert_eq!(list.file_row(0).unwrap().subtitle, "");
    list.view = View::Tiles;
    assert_eq!(list.file_row(0).unwrap().subtitle, "Folder");
    assert_eq!(list.file_row(2).unwrap().subtitle, "TXT File");
}

#[test]
fn only_larger_views_show_thumbnails() {
    assert!(View::ExtraLarge.shows_thumbnails() && View::Medium.shows_thumbnails() && View::Tiles.shows_thumbnails());
    assert!(!View::Small.shows_thumbnails() && !View::List.shows_thumbnails() && !View::Details.shows_thumbnails());
}

#[test]
fn selection_follows_items_through_a_resort() {
    let mut list = state(Column::Name, false, false);
    list.press(2, false, false);
    list.sort = Column::Size;
    list.descending = true;
    list.rebuild();
    assert_eq!(list.selected_items().next().unwrap().name.as_ref(), "Apple.txt");
    assert!(list.is_row_selected(2));
}
