use crate::util::format::Unit;
use crate::icons::IconStore;
use crate::model::item::{Column, Item, ItemState};
use crate::model::scan::Live;
use crate::ui::FileRow;
use rayon::prelude::*;
use slint::{Image, Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};
use std::cell::{Ref, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering::Relaxed;

const RESET_THRESHOLD: usize = 512;
const NOT_SHOWN: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    ExtraLarge,
    Large,
    Medium,
    Small,
    List,
    #[default]
    Details,
    Tiles,
}

impl View {
    pub const ALL: [View; 7] =
        [View::ExtraLarge, View::Large, View::Medium, View::Small, View::List, View::Details, View::Tiles];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|view| *view == self).unwrap_or(0)
    }

    fn icon_size(self) -> u32 {
        match self {
            View::ExtraLarge => 256,
            View::Large => 96,
            View::Medium | View::Tiles => 48,
            View::Small | View::List | View::Details => 16,
        }
    }

    fn shows_thumbnails(self) -> bool {
        self.icon_size() >= 48
    }
}

fn name_starts_with(name: &str, lowercase_prefix: &str) -> bool {
    let mut name = name.chars().flat_map(char::to_lowercase);
    lowercase_prefix.chars().all(|character| name.next() == Some(character))
}

pub struct ListState {
    pub items: Vec<Item>,
    pub columns: Vec<Column>,
    pub unit: Unit,
    pub sort: Column,
    pub descending: bool,
    pub show_hidden: bool,
    pub view: View,
    order: Vec<u32>,
    rows: Vec<u32>,
    selected: Vec<bool>,
    cut: Vec<bool>,
    anchor: usize,
    cursor: usize,
    total_size: u64,
    largest_size: u64,
}

impl ListState {
    pub fn new(columns: Vec<Column>, unit: Unit, sort: Column, descending: bool, show_hidden: bool) -> Self {
        Self {
            items: Vec::new(),
            columns,
            unit,
            sort,
            descending,
            show_hidden,
            view: View::default(),
            order: Vec::new(),
            rows: Vec::new(),
            selected: Vec::new(),
            cut: Vec::new(),
            anchor: 0,
            cursor: 0,
            total_size: 0,
            largest_size: 0,
        }
    }

    fn set_items(&mut self, items: Vec<Item>) {
        self.selected = vec![false; items.len()];
        self.cut = vec![false; items.len()];
        self.items = items;
        self.order.clear();
        self.anchor = 0;
        self.cursor = 0;
        self.rebuild();
    }

    fn compare(&self, a: u32, b: u32) -> Ordering {
        let (left, right) = (&self.items[a as usize], &self.items[b as usize]);
        let by_column = self.sort.compare(left, right);
        let by_column = if self.descending { by_column.reverse() } else { by_column };
        let folders_first =
            if self.sort.groups_folders_first() { right.is_dir.cmp(&left.is_dir) } else { Ordering::Equal };
        folders_first.then(by_column).then_with(|| Column::Name.compare(left, right)).then(a.cmp(&b))
    }

    fn row_after_rebuild(&self, item: Option<u32>, old_row: usize) -> usize {
        match item.map(|index| self.rows[index as usize]) {
            Some(row) if row != NOT_SHOWN => row as usize,
            _ => old_row.min(self.order.len().saturating_sub(1)),
        }
    }

    pub fn rebuild(&mut self) {
        let anchored = self.order.get(self.anchor).copied();
        let cursored = self.order.get(self.cursor).copied();
        let shows = |item: &Item| self.show_hidden || !item.is_hidden();
        let mut order: Vec<u32> =
            self.items.iter().enumerate().filter(|(_, item)| shows(item)).map(|(index, _)| index as u32).collect();
        order.par_sort_unstable_by(|a, b| self.compare(*a, *b));
        self.rows = vec![NOT_SHOWN; self.items.len()];
        for (row, index) in order.iter().enumerate() {
            self.rows[*index as usize] = row as u32;
        }
        self.order = order;
        self.anchor = self.row_after_rebuild(anchored, self.anchor);
        self.cursor = self.row_after_rebuild(cursored, self.cursor);
        self.recount();
    }

    pub fn recount(&mut self) {
        let sizes = || self.order.iter().map(|index| self.items[*index as usize].size);
        self.total_size = sizes().sum();
        self.largest_size = sizes().max().unwrap_or(0);
    }

    pub fn row_count(&self) -> usize {
        self.order.len()
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    fn item_at(&self, row: usize) -> Option<(usize, &Item)> {
        let index = *self.order.get(row)? as usize;
        Some((index, &self.items[index]))
    }

    pub fn item(&self, row: usize) -> Option<&Item> {
        self.item_at(row).map(|(_, item)| item)
    }

    pub fn visible_items(&self) -> impl Iterator<Item = &Item> {
        self.order.iter().map(|index| &self.items[*index as usize])
    }

    pub fn selected_items(&self) -> impl Iterator<Item = &Item> {
        self.visible_items_indexed().filter(|(index, _)| self.selected[*index]).map(|(_, item)| item)
    }

    fn visible_items_indexed(&self) -> impl Iterator<Item = (usize, &Item)> {
        self.order.iter().map(|index| (*index as usize, &self.items[*index as usize]))
    }

    pub fn only_selected(&self) -> Option<(usize, &Item)> {
        let mut selected = self.order.iter().enumerate().filter(|(_, index)| self.selected[**index as usize]);
        let (row, index) = selected.next()?;
        selected.next().is_none().then(|| (row, &self.items[*index as usize]))
    }

    pub fn select_name(&mut self, name: &str) -> Option<usize> {
        let row = self.visible_items().position(|item| &*item.name == name)?;
        self.press(row, false, false);
        Some(row)
    }

    pub fn select_typed(&mut self, typed: &str) -> Option<usize> {
        let row = self.find_typed(&typed.to_lowercase())?;
        self.press(row, false, false);
        Some(row)
    }

    fn find_typed(&self, typed: &str) -> Option<usize> {
        let first = typed.chars().next()?;
        let cycling = typed.chars().all(|character| character == first);
        let prefix = if cycling { &typed[..first.len_utf8()] } else { typed };
        let start = self.cursor + usize::from(cycling && self.is_row_selected(self.cursor));
        let count = self.order.len();
        (start..start + count)
            .map(|row| row % count)
            .find(|row| self.item(*row).is_some_and(|item| name_starts_with(&item.name, prefix)))
    }

    pub fn focused_row(&self) -> Option<usize> {
        self.is_row_selected(self.cursor).then_some(self.cursor)
    }

    pub fn mark_cut(&mut self, paths: &HashSet<PathBuf>) {
        self.cut = self.items.iter().map(|item| !paths.is_empty() && paths.contains(&item.path())).collect();
    }

    pub fn selected_count(&self) -> usize {
        self.selected_items().count()
    }

    pub fn is_row_selected(&self, row: usize) -> bool {
        self.item_at(row).is_some_and(|(index, _)| self.selected[index])
    }

    pub fn clear_selection(&mut self) {
        self.selected.fill(false);
    }

    pub fn press(&mut self, row: usize, toggle: bool, extend: bool) {
        if row >= self.order.len() {
            return;
        }
        match (toggle, extend) {
            (_, true) => self.select_range(row),
            (true, false) => {
                let index = self.order[row] as usize;
                self.selected[index] = !self.selected[index];
                self.anchor = row;
            }
            (false, false) => {
                self.clear_selection();
                self.selected[self.order[row] as usize] = true;
                self.anchor = row;
            }
        }
        self.cursor = row;
    }

    fn select_range(&mut self, row: usize) {
        self.clear_selection();
        let (first, last) = (self.anchor.min(row), self.anchor.max(row));
        for index in &self.order[first..=last] {
            self.selected[*index as usize] = true;
        }
    }

    pub fn select_all(&mut self) {
        for index in &self.order {
            self.selected[*index as usize] = true;
        }
    }

    pub fn invert_selection(&mut self) {
        for index in &self.order {
            let selected = &mut self.selected[*index as usize];
            *selected = !*selected;
        }
    }

    pub fn move_cursor(&mut self, delta: i32, extend: bool) -> Option<usize> {
        let last = self.order.len().checked_sub(1)?;
        let row = (self.cursor as i64 + delta as i64).clamp(0, last as i64) as usize;
        self.press(row, false, extend);
        Some(row)
    }

    fn file_row(&self, row: usize) -> Option<FileRow> {
        let (index, item) = self.item_at(row)?;
        let cells: Vec<SharedString> =
            self.columns.iter().map(|column| column.cell(item, self.unit).into()).collect();
        let ratio = if self.largest_size == 0 { 0.0 } else { item.size as f32 / self.largest_size as f32 };
        let subtitle = if self.view == View::Tiles { item.type_label().into() } else { SharedString::new() };
        Some(FileRow {
            cells: ModelRc::new(VecModel::from(cells)),
            icon: Image::default(),
            subtitle,
            folder: item.is_dir,
            selected: self.selected[index],
            ratio,
            busy: item.state == ItemState::Pending,
            warn: matches!(item.state, ItemState::Partial | ItemState::Cancelled),
            dim: item.is_hidden() || self.cut[index],
        })
    }
}

mod model;
pub use model::*;

#[cfg(test)]
mod tests;
