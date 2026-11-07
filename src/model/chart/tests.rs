use super::*;
use crate::model::query::Query;

fn entries(count: usize) -> Vec<SummaryEntry> {
    (0..count)
        .map(|index| SummaryEntry {
            label: format!("entry {index}"),
            files: 1,
            size: 100,
            filter: Query::default(),
            folder: None,
        })
        .collect()
}

fn point_at(angle: f32) -> (f32, f32) {
    (0.8 * angle.sin(), -0.8 * angle.cos())
}

#[test]
fn the_combined_treemap_tile_points_at_no_row() {
    let entries = entries(14);
    let tiles = build_treemap(&entries, Unit::Auto);
    let other = tiles.iter().find(|tile| tile.label == "Other").unwrap();
    assert_eq!(other.index as usize, entries.len());
    assert!(tiles.iter().filter(|tile| tile.label != "Other").all(|tile| (tile.index as usize) < MAX_TREEMAP_TILES));
}

#[test]
fn the_combined_wheel_slice_selects_no_row() {
    let entries = entries(14);
    let (first_x, first_y) = point_at(0.1);
    assert_eq!(hit_test_wheel(&entries, false, first_x, first_y), Some(0));
    let (other_x, other_y) = point_at(TAU - 0.1);
    assert_eq!(hit_test_wheel(&entries, false, other_x, other_y), None);
}

#[test]
fn short_lists_have_no_combined_area() {
    let entries = entries(MAX_SLICES);
    let (x, y) = point_at(TAU - 0.1);
    assert_eq!(hit_test_wheel(&entries, false, x, y), Some(MAX_SLICES - 1));
}
