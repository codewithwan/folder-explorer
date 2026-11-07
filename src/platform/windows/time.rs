//! FILETIME helpers and local-day range parsing.

use std::mem::zeroed;
use std::ops::Range;
use std::ptr::null;
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::System::Time::{
    FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime, TIME_ZONE_INFORMATION,
    TzSpecificLocalTimeToSystemTime,
};

const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
pub(super) const TICKS_PER_DAY: u64 = 864_000_000_000;

fn to_filetime(ticks: u64) -> FILETIME {
    FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 }
}

pub fn now_filetime() -> u64 {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    UNIX_EPOCH_TICKS + (since_epoch.as_nanos() / 100) as u64
}

pub fn format_time(ticks: u64) -> String {
    if ticks == 0 {
        return String::new();
    }
    let mut utc: SYSTEMTIME = unsafe { zeroed() };
    let mut local: SYSTEMTIME = unsafe { zeroed() };
    let converted = unsafe {
        FileTimeToSystemTime(&to_filetime(ticks), &mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(null(), &utc, &mut local) != 0
    };
    if !converted {
        return String::new();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute
    )
}

fn system_time_ticks(time: &SYSTEMTIME) -> Option<u64> {
    let mut filetime: FILETIME = unsafe { zeroed() };
    let converted = unsafe { SystemTimeToFileTime(time, &mut filetime) != 0 };
    converted.then_some((filetime.dwHighDateTime as u64) << 32 | filetime.dwLowDateTime as u64)
}

fn local_ticks(local: &SYSTEMTIME, zone: *const TIME_ZONE_INFORMATION) -> Option<u64> {
    let mut utc: SYSTEMTIME = unsafe { zeroed() };
    let converted = unsafe { TzSpecificLocalTimeToSystemTime(zone, local, &mut utc) != 0 };
    converted.then(|| system_time_ticks(&utc)).flatten()
}

fn next_date(date: &SYSTEMTIME) -> Option<SYSTEMTIME> {
    let tomorrow = to_filetime(system_time_ticks(date)? + TICKS_PER_DAY);
    let mut next: SYSTEMTIME = unsafe { zeroed() };
    let converted = unsafe { FileTimeToSystemTime(&tomorrow, &mut next) != 0 };
    converted.then_some(next)
}

pub(super) fn day_bounds(text: &str, zone: *const TIME_ZONE_INFORMATION) -> Option<Range<u64>> {
    let mut parts = text.trim().splitn(3, '-').map(|part| part.parse::<u16>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    let date = SYSTEMTIME { wYear: year, wMonth: month, wDay: day, ..unsafe { zeroed() } };
    Some(local_ticks(&date, zone)?..local_ticks(&next_date(&date)?, zone)?)
}

pub fn local_day(text: &str) -> Option<Range<u64>> {
    day_bounds(text, null())
}
