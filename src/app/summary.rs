use super::*;

impl App {
    pub(crate) fn refresh_summary(&mut self) {
        self.summary_id += 1;
        if !self.ui().get_summary_open() {
            return;
        }
        let node = match &self.location {
            Location::Folder(path) => self.cache.get(path),
            Location::Computer => None,
        };
        let Some(node) = node else {
            self.summary.clear();
            self.show_summary();
            return;
        };
        let (id, kind) = (self.summary_id, SUMMARY_KINDS[self.summary_kind]);
        self.spawn(move |sender| {
            let _ = sender.send(Message::Summary { id, entries: query::summarize(&node, kind) });
        });
    }

    pub(crate) fn show_summary(&self) {
        let unit = self.settings.unit;
        let (rows, total) = chart::build_rows(&self.summary, unit);
        let treemap = chart::build_treemap(&self.summary, unit);
        let ui = self.ui();
        let donut = ui.get_summary_viz_mode() == 0;
        ui.set_summary_hovered_index(-1);
        ui.set_summary_total_size(format_size(total, unit).into());
        ui.set_summary_chart_image(chart::render_wheel(&self.summary, donut, None));
        ui.set_summary_treemap(ModelRc::new(VecModel::from(treemap)));
        ui.set_summary_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_summary_kind(self.summary_kind as i32);
        let hint = if self.location == Location::Computer {
            "Open a folder to see what fills it."
        } else {
            "Available when the scan finishes."
        };
        ui.set_summary_hint(hint.into());
    }

    pub(crate) fn refresh_summary_wheel(&self, hovered: Option<usize>) {
        let ui = self.ui();
        let mode = ui.get_summary_viz_mode();
        if mode <= 1 {
            ui.set_summary_chart_image(chart::render_wheel(&self.summary, mode == 0, hovered));
        }
    }

    pub(crate) fn hover_summary_row(&self, index: i32) {
        let ui = self.ui();
        let clamped = if index >= 0 && (index as usize) < self.summary.len() { index } else { -1 };
        if ui.get_summary_hovered_index() != clamped {
            ui.set_summary_hovered_index(clamped);
            self.refresh_summary_wheel((clamped >= 0).then_some(clamped as usize));
        }
    }

    pub(crate) fn hover_summary_wheel(&self, nx: f32, ny: f32) {
        let donut = self.ui().get_summary_viz_mode() == 0;
        let hit = chart::hit_test_wheel(&self.summary, donut, nx, ny).map_or(-1, |idx| idx as i32);
        self.hover_summary_row(hit);
    }

    pub(crate) fn choose_summary(&mut self, kind: usize) {
        self.summary_kind = kind.min(SUMMARY_KINDS.len() - 1);
        self.summary.clear();
        self.show_summary();
        self.refresh_summary();
    }

    pub(crate) fn activate_summary(&mut self, row: usize) {
        let Some(entry) = self.summary.get(row).cloned() else { return };
        if let (Some(folder), Location::Folder(parent)) = (entry.folder, &self.location) {
            self.go(Location::Folder(parent.join(folder)));
        } else if !entry.filter.is_empty() {
            self.ui().set_search_text(SharedString::new());
            self.run_query(entry.filter);
        }
    }

    // Checksums

}
