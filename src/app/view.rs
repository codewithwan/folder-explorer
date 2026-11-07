use super::*;

impl App {
    pub(crate) fn shown_columns(&self) -> Vec<Column> {
        let mut columns = self.settings.columns.clone();
        if self.showing_results && !columns.contains(&Column::Location) {
            columns.insert(2, Column::Location);
        }
        columns
    }

    pub(crate) fn sync_columns(&self) {
        let columns = self.shown_columns();
        let (sort, descending) = {
            let state = self.list.state();
            (state.sort, state.descending)
        };
        let info: Vec<ColumnInfo> = columns
            .iter()
            .map(|column| ColumnInfo {
                title: column.title().into(),
                width: self.settings.width_of(*column),
                right_aligned: column.right_aligned(),
                sort_direction: match (*column == sort, descending) {
                    (false, _) => 0,
                    (true, false) => 1,
                    (true, true) => -1,
                },
            })
            .collect();
        let ui = self.ui();
        ui.set_columns_width(info.iter().map(|column| column.width).sum());
        ui.set_columns(ModelRc::new(VecModel::from(info)));
        let choices: Vec<ColumnChoice> = Column::ALL
            .iter()
            .filter(|column| !column.is_fixed())
            .map(|column| ColumnChoice {
                title: column.title().into(),
                shown: self.settings.columns.contains(column),
            })
            .collect();
        ui.set_column_choices(ModelRc::new(VecModel::from(choices)));
        let sort_choices: Vec<ColumnChoice> = Column::DEFAULT
            .iter()
            .map(|column| ColumnChoice { title: column.title().into(), shown: *column == sort })
            .collect();
        ui.set_sort_choices(ModelRc::new(VecModel::from(sort_choices)));
        if self.list.state().columns != columns {
            self.list.update(|state| state.columns = columns);
        }
    }

    pub(crate) fn toggle_column(&mut self, choice: usize) {
        let Some(column) = Column::ALL.iter().filter(|column| !column.is_fixed()).nth(choice).copied() else { return };
        let columns = &mut self.settings.columns;
        if columns.contains(&column) {
            columns.retain(|shown| *shown != column);
        } else {
            columns.push(column);
            columns.sort_by_key(|shown| Column::ALL.iter().position(|known| known == shown));
        }
        self.sync_columns();
        self.start_hashing();
    }

    pub(crate) fn sort_by(&mut self, index: usize) {
        let Some(column) = self.list.state().columns.get(index).copied() else { return };
        self.sort_column(column);
    }

    pub(crate) fn sort_column(&mut self, column: Column) {
        self.list.update(|state| {
            state.descending = if state.sort == column { !state.descending } else { column.starts_descending() };
            state.sort = column;
            state.rebuild();
        });
        let state = self.list.state();
        self.settings.sort = state.sort;
        self.settings.descending = state.descending;
        drop(state);
        self.sync_columns();
    }

    pub(crate) fn resize_column(&mut self, index: usize, width: f32) {
        let Some(column) = self.list.state().columns.get(index).copied() else { return };
        self.settings.set_width(column, width);
        self.sync_columns();
    }

    pub(crate) fn choose_unit(&mut self, index: usize) {
        let unit = Unit::ALL[index];
        self.settings.unit = unit;
        self.ui().set_unit_index(index as i32);
        self.list.update(|state| state.unit = unit);
        self.show_selection();
        self.show_summary();
        self.show_drives();
    }

    pub(crate) fn choose_view(&mut self, index: usize) {
        let view = View::ALL[index];
        if let Location::Folder(path) = &self.location {
            self.settings.set_view_of(path, view);
        }
        self.show_view(view);
    }

    pub(crate) fn show_view(&self, view: View) {
        self.ui().set_view_index(view.index() as i32);
        self.icons.cancel_queued();
        self.list.update(|state| state.view = view);
    }

    pub(crate) fn refresh_icons(&self) {
        self.sidebar.refresh_icons();
        if self.drive_icons_missing.get() {
            self.show_drives();
        }
    }

    pub(crate) fn toggle_hidden(&mut self) {
        let show_hidden = self.ui().get_show_hidden();
        self.settings.show_hidden = show_hidden;
        self.list.update(|state| {
            state.show_hidden = show_hidden;
            state.rebuild();
        });
        self.show_selection();
    }

}
