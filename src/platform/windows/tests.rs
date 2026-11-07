use super::drives::enumerate_pins;
use super::shell::reveal_argument;
use super::time::{TICKS_PER_DAY, day_bounds};
use super::*;
use crate::model::scan::tests::TempTree;
use std::ffi::OsString;
use std::mem::zeroed;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::Time::TIME_ZONE_INFORMATION;

#[test]
fn permanent_delete_removes_files_and_folders() {
    let tree = TempTree::new();
    tree.file("folder/inner.txt", 1).file("a.txt", 1).file("kept.txt", 1);
    assert!(delete_permanently(&[tree.0.join("folder"), tree.0.join("a.txt")], 0));
    let left: Vec<_> = std::fs::read_dir(&tree.0).unwrap().map(|entry| entry.unwrap().file_name()).collect();
    assert_eq!(left, ["kept.txt"]);
}

fn berlin() -> TIME_ZONE_INFORMATION {
    let last_sunday = |month, hour| SYSTEMTIME { wMonth: month, wDay: 5, wHour: hour, ..unsafe { zeroed() } };
    TIME_ZONE_INFORMATION {
        Bias: -60,
        StandardDate: last_sunday(10, 3),
        DaylightDate: last_sunday(3, 2),
        DaylightBias: -60,
        ..unsafe { zeroed() }
    }
}

fn berlin_day_hours(text: &str) -> u64 {
    let day = day_bounds(text, &berlin()).unwrap();
    (day.end - day.start) / (TICKS_PER_DAY / 24)
}

#[test]
fn days_end_at_the_next_local_midnight_across_clock_changes() {
    assert_eq!(berlin_day_hours("2026-03-29"), 23);
    assert_eq!(berlin_day_hours("2026-10-25"), 25);
    assert_eq!(berlin_day_hours("2026-06-15"), 24);
}

#[test]
fn a_day_ends_where_the_next_one_starts_across_a_month_end() {
    let zone = berlin();
    assert_eq!(day_bounds("2026-01-31", &zone).unwrap().end, day_bounds("2026-02-01", &zone).unwrap().start);
}

#[test]
fn times_round_trip_through_a_local_day() {
    let day = local_day("2024-03-15").unwrap();
    assert_eq!(format_time(day.start), "2024-03-15 00:00");
    assert_eq!(format_time(day.end), "2024-03-16 00:00");
}

#[test]
fn impossible_dates_are_rejected() {
    assert!(local_day("2024-13-40").is_none());
    assert!(local_day("yesterday").is_none());
    assert!(local_day("2024-03").is_none());
}

#[test]
fn a_changed_clipboard_is_left_alone() {
    let before = clipboard_sequence();
    clear_clipboard_if_unchanged(before.wrapping_add(1));
    assert_eq!(clipboard_sequence(), before);
}

#[test]
fn reveal_keeps_names_that_are_not_valid_unicode() {
    let name = OsString::from_wide(&[0x61, 0xD800]);
    let units: Vec<u16> = reveal_argument(&Path::new(r"C:\data").join(&name)).encode_wide().collect();
    assert!(units.contains(&0xD800));
}

#[test]
fn unset_times_render_empty() {
    assert_eq!(format_time(0), "");
}

#[test]
fn now_is_after_2020() {
    assert!(now_filetime() > local_day("2020-01-01").unwrap().start);
}

#[test]
fn the_system_drive_reports_real_space() {
    let usage = disk_usage(Path::new(r"C:\")).unwrap();
    assert!(usage.total > 0 && usage.free <= usage.total);
    assert!((0.0..=1.0).contains(&usage.used_ratio()));
}

#[test]
fn quick_access_opens_and_lists_named_absolute_paths() {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let pins = unsafe { enumerate_pins() }.unwrap();
    assert!(pins.iter().all(|(path, name)| path.is_absolute() && !name.is_empty()));
}

#[test]
fn drives_include_the_system_drive() {
    assert!(drives().iter().any(|drive| drive.root == Path::new(r"C:\")));
}
