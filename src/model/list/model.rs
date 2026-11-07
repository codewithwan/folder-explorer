use super::*;

pub struct ListModel {
    state: RefCell<ListState>,
    notify: ModelNotify,
    icons: Rc<IconStore>,
    waiting_for_icons: RefCell<BTreeSet<usize>>,
}

impl ListModel {
    pub fn new(state: ListState, icons: Rc<IconStore>) -> Self {
        Self {
            state: RefCell::new(state),
            notify: ModelNotify::default(),
            icons,
            waiting_for_icons: RefCell::default(),
        }
    }

    pub fn refresh_icons(&self) {
        let rows = std::mem::take(&mut *self.waiting_for_icons.borrow_mut());
        for row in rows {
            self.notify.row_changed(row);
        }
    }

    pub fn state(&self) -> Ref<'_, ListState> {
        self.state.borrow()
    }

    pub fn set_items(&self, items: Vec<Item>) {
        self.state.borrow_mut().set_items(items);
        self.notify.reset();
    }

    pub fn update(&self, change: impl FnOnce(&mut ListState)) {
        change(&mut self.state.borrow_mut());
        self.notify.reset();
    }

    pub fn update_item(&self, index: usize, change: impl FnOnce(&mut Item)) {
        let row = {
            let mut state = self.state.borrow_mut();
            let Some(item) = state.items.get_mut(index) else { return };
            change(item);
            state.rows[index]
        };
        if row != NOT_SHOWN {
            self.notify.row_changed(row as usize);
        }
    }

    pub fn apply_live(&self, lives: &[Live]) {
        for (index, live) in lives.iter().enumerate() {
            let size = live.size.load(Relaxed);
            let stale = self.state.borrow().items.get(index).is_some_and(|item| {
                item.state == ItemState::Pending && item.size != size
            });
            if stale {
                self.update_item(index, |item| {
                    item.size = size;
                    item.file_count = live.files.load(Relaxed);
                    item.dir_count = live.dirs.load(Relaxed);
                });
            }
        }
        self.state.borrow_mut().recount();
    }

    pub fn change_selection<R>(&self, change: impl FnOnce(&mut ListState) -> R) -> R {
        let (result, changed) = {
            let mut state = self.state.borrow_mut();
            let before = state.selected.clone();
            let result = change(&mut state);
            let changed: Vec<usize> = state
                .order
                .iter()
                .enumerate()
                .filter(|(_, index)| before[**index as usize] != state.selected[**index as usize])
                .map(|(row, _)| row)
                .collect();
            (result, changed)
        };
        if changed.len() > RESET_THRESHOLD {
            self.notify.reset();
        } else {
            for row in changed {
                self.notify.row_changed(row);
            }
        }
        result
    }
}

impl Model for ListModel {
    type Data = FileRow;

    fn row_count(&self) -> usize {
        self.state.borrow().row_count()
    }

    fn row_data(&self, row: usize) -> Option<FileRow> {
        let state = self.state.borrow();
        let mut file_row = state.file_row(row)?;
        let item = state.item(row)?;
        let (icon, ready) = self.icons.item_icon(item, state.view.icon_size(), state.view.shows_thumbnails());
        file_row.icon = icon;
        if !ready {
            self.waiting_for_icons.borrow_mut().insert(row);
        }
        Some(file_row)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
