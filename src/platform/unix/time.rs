//! FILETIME helpers and local-day range parsing, via libc.

use crate::util::fsmeta::UNIX_EPOCH_TICKS;
use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};

const TICKS_PER_SECOND: u64 = 10_000_000;

pub fn now_filetime() -> u64 {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    UNIX_EPOCH_TICKS + (since_epoch.as_nanos() / 100) as u64
}

pub fn format_time(ticks: u64) -> String {
    if ticks == 0 || ticks < UNIX_EPOCH_TICKS {
        return String::new();
    }
    let secs = ((ticks - UNIX_EPOCH_TICKS) / TICKS_PER_SECOND) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return String::new();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

/// FILETIME ticks at local midnight starting the given calendar day. `mktime`
/// interprets the broken-down time as local, normalizing out-of-range fields
/// (so day + 1 rolls into the next month) and resolving DST via `tm_isdst = -1`.
fn local_midnight_ticks(year: i32, month: i32, day: i32) -> Option<u64> {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_isdst = -1;
    let secs = unsafe { libc::mktime(&mut tm) };
    if secs == -1 {
        return None;
    }
    Some(UNIX_EPOCH_TICKS + (secs as u64) * TICKS_PER_SECOND)
}

pub fn local_day(text: &str) -> Option<Range<u64>> {
    let mut parts = text.trim().splitn(3, '-').map(|part| part.parse::<i32>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(local_midnight_ticks(year, month, day)?..local_midnight_ticks(year, month, day + 1)?)
}
