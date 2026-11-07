use super::*;

impl App {
    pub(crate) fn show_totals(&self) {
        let state = self.list.state();
        let items = plural(state.row_count() as u64, "item");
        let text = format!("{items} · {}", format_size(state.total_size(), state.unit));
        self.ui().set_status_items(text.into());
    }

    pub(crate) fn show_selection(&self) {
        if self.location != Location::Computer {
            self.show_totals();
        }
        let state = self.list.state();
        let count = state.selected_count();
        let size: u64 = state.selected_items().map(|item| item.size).sum();
        let text = if count == 0 {
            String::new()
        } else {
            format!("{count} selected · {}", format_size(size, state.unit))
        };
        let ui = self.ui();
        ui.set_status_selection(text.into());
        ui.set_selected_count(count as i32);
        ui.set_single_selection(count == 1);
    }

    pub(crate) fn show_errors(&self) {
        let lines: Vec<SharedString> = self
            .errors
            .iter()
            .map(|error| format!("{} — {}", error.path.display(), error.message).into())
            .collect();
        let ui = self.ui();
        ui.set_status_errors(match lines.len() {
            0 => SharedString::new(),
            count => plural(count as u64, "unreadable item").into(),
        });
        ui.set_errors(ModelRc::new(VecModel::from(lines)));
    }

    // Drives and sidebar

}
