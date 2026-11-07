use crate::util::format::{format_size, group_digits, Unit};
use crate::model::item::Item;
use crate::platform;
use crate::model::scan::{DirNode, FileEntry, Times};
use rayon::prelude::*;
use std::collections::HashMap;
use std::ffi::OsString;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

const TICKS_PER_MONTH: u64 = 26_300_160_000_000;
const AGE_MONTHS: [u64; 23] =
    [0, 1, 2, 3, 6, 9, 12, 24, 36, 48, 60, 72, 84, 96, 108, 120, 132, 144, 156, 168, 180, 192, 204];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateField {
    #[default]
    Modified,
    Created,
    Accessed,
}

impl DateField {
    pub fn of(self, times: &Times) -> u64 {
        match self {
            DateField::Modified => times.modified,
            DateField::Created => times.created,
            DateField::Accessed => times.accessed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateRule {
    After,
    Before,
    On,
}

pub fn date_range(day: Range<u64>, rule: DateRule) -> Range<u64> {
    match rule {
        DateRule::After => day.end..u64::MAX,
        DateRule::Before => 0..day.start,
        DateRule::On => day,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub text: String,
    pub extension: Option<String>,
    pub size: Option<Range<u64>>,
    pub date: Option<(DateField, Range<u64>)>,
    pub direct_files: bool,
}

impl Query {
    pub fn new(text: &str) -> Self {
        Self { text: text.trim().to_lowercase(), ..Self::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && !self.files_only()
    }

    fn files_only(&self) -> bool {
        self.extension.is_some() || self.size.is_some() || self.date.is_some() || self.direct_files
    }

    fn matches_name(&self, name: &str) -> bool {
        self.text.is_empty() || contains_ignoring_case(name, &self.text)
    }

    fn matches_file(&self, name: &str, entry: &FileEntry) -> bool {
        self.matches_name(name)
            && !(self.files_only() && entry.is_folder_link())
            && self.extension.as_ref().is_none_or(|wanted| extension_of(name) == *wanted)
            && self.size.as_ref().is_none_or(|range| range.contains(&entry.size))
            && self.date.as_ref().is_none_or(|(field, range)| range.contains(&field.of(&entry.times)))
    }
}

fn contains_ignoring_case(haystack: &str, lowercase_needle: &str) -> bool {
    if lowercase_needle.is_ascii() {
        let needle = lowercase_needle.as_bytes();
        haystack.as_bytes().windows(needle.len()).any(|window| window.eq_ignore_ascii_case(needle))
    } else {
        haystack.to_lowercase().contains(lowercase_needle)
    }
}

pub fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension.to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn search(node: &DirNode, path: &Path, query: &Query) -> Vec<Item> {
    let parent: Arc<Path> = path.into();
    let mut hits: Vec<Item> = node
        .files()
        .filter(|(name, entry)| query.matches_file(&name.to_string_lossy(), entry))
        .map(|(name, entry)| Item::file(name, entry, &parent))
        .collect();
    if query.direct_files {
        return hits;
    }
    if !query.files_only() {
        hits.extend(
            node.children
                .iter()
                .filter(|child| query.matches_name(&child.name.to_string_lossy()))
                .map(|child| Item::folder(child, &parent)),
        );
    }
    let nested: Vec<Item> = node
        .children
        .par_iter()
        .flat_map_iter(|child| search(child, &path.join(&*child.name), query))
        .collect();
    hits.extend(nested);
    hits
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SummaryKind {
    #[default]
    Extension,
    Folders,
    Age(DateField),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SummaryEntry {
    pub label: String,
    pub files: u64,
    pub size: u64,
    pub filter: Query,
    pub folder: Option<OsString>,
}

impl SummaryEntry {
    pub fn detail(&self, unit: Unit) -> String {
        format!("{} · {}", format_size(self.size, unit), group_digits(self.files))
    }
}

fn flatten<'a>(node: &'a DirNode, into: &mut Vec<&'a DirNode>) {
    into.push(node);
    for child in &node.children {
        flatten(child, into);
    }
}

fn tally<K: Eq + std::hash::Hash + Send>(
    node: &DirNode,
    key_of: impl Fn(&str, &FileEntry) -> K + Sync,
) -> HashMap<K, (u64, u64)> {
    let mut nodes = Vec::new();
    flatten(node, &mut nodes);
    nodes
        .par_iter()
        .fold(HashMap::new, |mut totals: HashMap<K, (u64, u64)>, dir| {
            for (name, entry) in dir.files().filter(|(_, entry)| !entry.is_folder_link()) {
                let total = totals.entry(key_of(&name.to_string_lossy(), entry)).or_default();
                total.0 += 1;
                total.1 += entry.size;
            }
            totals
        })
        .reduce(HashMap::new, |mut merged, part| {
            for (key, (files, size)) in part {
                let total = merged.entry(key).or_default();
                total.0 += files;
                total.1 += size;
            }
            merged
        })
}

fn age_bucket(ticks: u64, now: u64) -> usize {
    let months = now.saturating_sub(ticks) / TICKS_PER_MONTH;
    AGE_MONTHS.partition_point(|threshold| *threshold <= months) - 1
}

fn age_label(bucket: usize) -> String {
    match AGE_MONTHS[bucket] {
        0 => "Under 1 month".into(),
        1 => "Over 1 month".into(),
        12 => "Over 1 year".into(),
        months if months < 12 => format!("Over {months} months"),
        months => format!("Over {} years", months / 12),
    }
}

fn age_range(bucket: usize, now: u64) -> Range<u64> {
    let end = match AGE_MONTHS[bucket] {
        0 => u64::MAX,
        months => now.saturating_sub(months * TICKS_PER_MONTH) + 1,
    };
    let start = AGE_MONTHS.get(bucket + 1).map_or(0, |next| now.saturating_sub(next * TICKS_PER_MONTH) + 1);
    start..end
}

pub fn summarize(node: &DirNode, kind: SummaryKind) -> Vec<SummaryEntry> {
    match kind {
        SummaryKind::Extension => {
            let mut entries: Vec<_> = tally(node, |name, _| extension_of(name))
                .into_iter()
                .map(|(extension, (files, size))| SummaryEntry {
                    label: if extension.is_empty() { "No extension".into() } else { format!(".{extension}") },
                    files,
                    size,
                    filter: Query { extension: Some(extension), ..Query::default() },
                    folder: None,
                })
                .collect();
            entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.label.cmp(&b.label)));
            entries
        }
        SummaryKind::Folders => {
            let mut entries: Vec<_> = node
                .children
                .iter()
                .filter(|child| child.size > 0 || child.file_count > 0)
                .map(|child| {
                    let label = child.name.to_string_lossy().into_owned();
                    SummaryEntry {
                        files: child.file_count,
                        size: child.size,
                        filter: Query::new(&label),
                        folder: Some(child.name.to_os_string()),
                        label,
                    }
                })
                .collect();
            let direct_files: Vec<_> = node.files().filter(|(_, entry)| !entry.is_folder_link()).collect();
            if entries.is_empty() {
                entries.extend(direct_files.into_iter().map(|(name, entry)| {
                    let label = name.to_string_lossy().into_owned();
                    SummaryEntry { files: 1, size: entry.size, filter: Query::new(&label), folder: None, label }
                }));
            } else if !direct_files.is_empty() {
                let files = direct_files.len() as u64;
                let size: u64 = direct_files.iter().map(|(_, entry)| entry.size).sum();
                if size > 0 || files > 0 {
                    entries.push(SummaryEntry {
                        label: "[Folder files]".into(),
                        files,
                        size,
                        filter: Query { direct_files: true, ..Query::default() },
                        folder: None,
                    });
                }
            }
            entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.label.cmp(&b.label)));
            entries
        }
        SummaryKind::Age(field) => {
            let now = platform::now_filetime();
            let mut buckets: Vec<_> =
                tally(node, |_, entry| age_bucket(field.of(&entry.times), now)).into_iter().collect();
            buckets.sort_by_key(|(bucket, _)| *bucket);
            buckets
                .into_iter()
                .map(|(bucket, (files, size))| SummaryEntry {
                    label: age_label(bucket),
                    files,
                    size,
                    filter: Query { date: Some((field, age_range(bucket, now))), ..Query::default() },
                    folder: None,
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests;
