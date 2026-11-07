use super::*;
use crate::model::scan::tests::{TempTree, scan};
use std::fs;

const TICKS_PER_DAY: u64 = 864_000_000_000;

fn sample() -> TempTree {
    let tree = TempTree::new();
    tree.file("Readme.TXT", 10)
        .file("notes.txt", 20)
        .file("photos/holiday.jpg", 300)
        .file("photos/raw/holiday.cr2", 4000)
        .file("Makefile", 50);
    tree
}

fn names(mut hits: Vec<Item>) -> Vec<String> {
    hits.sort_by(|a, b| a.name.cmp(&b.name));
    hits.iter().map(|hit| hit.name.to_string()).collect()
}

fn run(tree: &TempTree, query: &Query) -> Vec<Item> {
    search(&scan(&tree.0), &tree.0, query)
}

#[test]
fn text_matches_names_ignoring_case_at_any_depth() {
    let tree = sample();
    assert_eq!(names(run(&tree, &Query::new("HOLIDAY"))), ["holiday.cr2", "holiday.jpg"]);
    assert_eq!(names(run(&tree, &Query::new("readme"))), ["Readme.TXT"]);
}

#[test]
fn folders_match_by_name_too() {
    let tree = sample();
    assert_eq!(names(run(&tree, &Query::new("photos"))), ["photos"]);
}

#[test]
fn non_ascii_names_match_ignoring_case() {
    let tree = TempTree::new();
    tree.file("ÉTÉ.txt", 1);
    assert_eq!(names(run(&tree, &Query::new("été"))), ["ÉTÉ.txt"]);
}

#[test]
fn hits_know_where_they_live() {
    let tree = sample();
    let hits = run(&tree, &Query::new("holiday.cr2"));
    assert_eq!(hits[0].path(), tree.0.join("photos").join("raw").join("holiday.cr2"));
}

#[cfg(windows)]
#[test]
fn hits_reach_the_exact_file_when_names_are_not_valid_unicode() {
    let tree = TempTree::new();
    let names = [non_unicode(&[0x61, 0xD800]), non_unicode(&[0x61, 0xD801])];
    for (index, name) in names.iter().enumerate() {
        let folder = tree.0.join(name);
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join(&names[1 - index]), vec![0; index + 1]).unwrap();
    }
    let hits = run(&tree, &Query::new("a"));
    assert_eq!(hits.len(), 4);
    for hit in hits.iter().filter(|hit| !hit.is_dir) {
        assert_eq!(fs::metadata(hit.path()).unwrap().len(), hit.size);
    }
    let folders: Vec<_> = hits.iter().filter(|hit| hit.is_dir).map(Item::path).collect();
    assert!(folders.iter().all(|folder| folder.is_dir()));
    assert_ne!(folders[0], folders[1]);
}

#[cfg(windows)]
#[test]
fn folder_rows_keep_the_exact_name_for_opening() {
    let tree = TempTree::new();
    let name = non_unicode(&[0x61, 0xD800]);
    tree.file("seed.txt", 1);
    fs::create_dir(tree.0.join(&name)).unwrap();
    fs::write(tree.0.join(&name).join("inside.txt"), [0]).unwrap();
    let summary = summarize(&scan(&tree.0), SummaryKind::Folders);
    assert!(summary.iter().any(|row| row.folder.as_ref() == Some(&name)));
}

#[test]
fn size_filters_return_files_only() {
    let tree = sample();
    let query = Query { size: Some(100..u64::MAX), ..Query::default() };
    assert_eq!(names(run(&tree, &query)), ["holiday.cr2", "holiday.jpg"]);
}

#[test]
fn extension_filters_match_exactly_ignoring_case() {
    let tree = sample();
    let query = Query { extension: Some("txt".into()), ..Query::default() };
    assert_eq!(names(run(&tree, &query)), ["Readme.TXT", "notes.txt"]);
    let query = Query { extension: Some(String::new()), ..Query::default() };
    assert_eq!(names(run(&tree, &query)), ["Makefile"]);
}

#[cfg(windows)]
#[test]
fn folder_links_are_not_files() {
    let tree = sample();
    let junction = tree.0.join("link");
    let target = tree.0.join("photos");
    let made = Command::new("cmd").args(["/c", "mklink", "/J"]).arg(&junction).arg(&target).output().unwrap();
    assert!(made.status.success());
    let node = scan(&tree.0);
    let everything = Query { size: Some(0..u64::MAX), ..Query::default() };
    assert_eq!(search(&node, &tree.0, &everything).len(), 5);
    let counted: u64 = summarize(&node, SummaryKind::Extension).iter().map(|row| row.files).sum();
    assert_eq!(counted, 5);
    assert_eq!(names(run(&tree, &Query::new("link"))), ["link"]);
}

#[test]
fn date_rules_treat_a_day_as_a_whole() {
    let start = 1_000 * TICKS_PER_DAY;
    let day = start..start + TICKS_PER_DAY;
    assert!(date_range(day.clone(), DateRule::On).contains(&(start + 5)));
    assert!(!date_range(day.clone(), DateRule::On).contains(&day.end));
    assert!(date_range(day.clone(), DateRule::After).contains(&day.end));
    assert!(!date_range(day.clone(), DateRule::After).contains(&(start + 5)));
    assert!(date_range(day.clone(), DateRule::Before).contains(&(start - 1)));
    assert!(!date_range(day, DateRule::Before).contains(&start));
}

#[test]
fn date_rules_use_the_end_of_the_day_they_are_given() {
    let start = 1_000 * TICKS_PER_DAY;
    let short_day = start..start + TICKS_PER_DAY / 24 * 23;
    assert_eq!(date_range(short_day.clone(), DateRule::After).start, short_day.end);
    assert_eq!(date_range(short_day.clone(), DateRule::On), short_day);
}

#[test]
fn date_filters_compare_the_chosen_field() {
    let tree = sample();
    let today = platform::now_filetime();
    let recent = Query { date: Some((DateField::Modified, today - TICKS_PER_DAY..u64::MAX)), ..Query::default() };
    assert_eq!(run(&tree, &recent).len(), 5);
    let old = Query { date: Some((DateField::Created, 0..today - TICKS_PER_DAY)), ..Query::default() };
    assert!(run(&tree, &old).is_empty());
}

#[test]
fn extensions_are_summarized_largest_first() {
    let tree = sample();
    let summary = summarize(&scan(&tree.0), SummaryKind::Extension);
    let rows: Vec<_> = summary.iter().map(|row| (row.label.as_str(), row.files, row.size)).collect();
    assert_eq!(
        rows,
        [(".cr2", 1, 4000), (".jpg", 1, 300), ("No extension", 1, 50), (".txt", 2, 30)]
    );
}

#[test]
fn summary_filters_find_the_same_files_they_counted() {
    let tree = sample();
    let node = scan(&tree.0);
    for kind in [SummaryKind::Extension, SummaryKind::Age(DateField::Modified)] {
        for row in summarize(&node, kind) {
            let hits = search(&node, &tree.0, &row.filter);
            assert_eq!(hits.len() as u64, row.files, "{}", row.label);
        }
    }
}

#[cfg(windows)]
#[test]
fn the_folder_files_row_finds_only_the_files_directly_inside() {
    let tree = sample();
    let junction = tree.0.join("link");
    let made = Command::new("cmd").args(["/c", "mklink", "/J"]).arg(&junction).arg(tree.0.join("photos")).output().unwrap();
    assert!(made.status.success());
    let node = scan(&tree.0);
    let summary = summarize(&node, SummaryKind::Folders);
    let row = summary.iter().find(|row| row.label == "[Folder files]").unwrap();
    assert!(!row.filter.is_empty());
    assert_eq!(names(search(&node, &tree.0, &row.filter)), ["Makefile", "Readme.TXT", "notes.txt"]);
    assert_eq!(row.files, 3);
}

#[test]
fn ages_land_in_the_bucket_they_have_reached() {
    let now = 500 * TICKS_PER_MONTH;
    let bucket_for_age = |months: u64| age_bucket(now - months * TICKS_PER_MONTH, now);
    assert_eq!(AGE_MONTHS[bucket_for_age(0)], 0);
    assert_eq!(AGE_MONTHS[bucket_for_age(1)], 1);
    assert_eq!(AGE_MONTHS[bucket_for_age(5)], 3);
    assert_eq!(AGE_MONTHS[bucket_for_age(13)], 12);
    assert_eq!(AGE_MONTHS[bucket_for_age(400)], 204);
    assert_eq!(age_bucket(now + 5, now), 0);
}

#[test]
fn age_ranges_tile_time_without_gaps() {
    let now = 500 * TICKS_PER_MONTH;
    for bucket in 0..AGE_MONTHS.len() {
        let range = age_range(bucket, now);
        let inside = if bucket == 0 { now } else { range.end - 1 };
        assert_eq!(age_bucket(inside, now), bucket);
        if let Some(next) = AGE_MONTHS.get(bucket + 1).map(|_| bucket + 1) {
            assert_eq!(age_range(next, now).end, range.start);
        }
    }
}

#[test]
fn labels_read_naturally() {
    assert_eq!(age_label(0), "Under 1 month");
    assert_eq!(age_label(1), "Over 1 month");
    assert_eq!(age_label(3), "Over 3 months");
    assert_eq!(age_label(6), "Over 1 year");
    assert_eq!(age_label(7), "Over 2 years");
}

#[test]
fn empty_folders_summarize_to_nothing() {
    let tree = TempTree::new();
    fs::create_dir(tree.0.join("empty")).unwrap();
    assert!(summarize(&scan(&tree.0), SummaryKind::Extension).is_empty());
}

/// A file name that is not valid Unicode: an unpaired surrogate on Windows,
/// raw invalid-UTF-8 bytes on Unix. Keeps distinct inputs distinct.
#[cfg(windows)]
fn non_unicode(units: &[u16]) -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    std::ffi::OsString::from_wide(units)
}