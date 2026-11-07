use super::*;

impl App {
    pub(crate) fn read_query(&self) -> Query {
        let ui = self.ui();
        let mut query = Query::new(&ui.get_search_text());
        if ui.get_size_enabled() {
            query.size = size_range(&ui.get_size_text(), ui.get_size_rule() == 0);
        }
        if ui.get_date_enabled()
            && let Some(day) = platform::local_day(&ui.get_date_text())
        {
            let field = DATE_FIELDS[ui.get_date_field() as usize];
            let rule = DATE_RULES[ui.get_date_rule() as usize];
            query.date = Some((field, query::date_range(day, rule)));
        }
        query
    }

    pub(crate) fn schedule_search(&mut self) {
        let me = self.me.clone();
        self.search_timer.start(TimerMode::SingleShot, SEARCH_DELAY, move || {
            if let Some(shared) = me.upgrade()
                && let Ok(mut app) = shared.try_borrow_mut()
            {
                app.apply_search();
            }
        });
    }

    pub(crate) fn apply_search(&mut self) {
        let query = self.read_query();
        if !query.is_empty() {
            self.run_query(query);
        } else if self.showing_results {
            self.show_location();
        } else {
            self.active_query = None;
            self.search_id += 1;
        }
    }

    pub(crate) fn run_query(&mut self, query: Query) {
        self.active_query = Some(query);
        self.search_id += 1;
        self.execute_query();
    }

    pub(crate) fn execute_query(&mut self) {
        let (Some(query), Location::Folder(path)) = (self.active_query.clone(), self.location.clone()) else { return };
        if self.scanning {
            return;
        }
        let Some(node) = self.cache.get(&path) else {
            self.start_scan(path);
            return;
        };
        let search = self.search_id;
        self.spawn(move |sender| {
            let started = Instant::now();
            let items = query::search(&node, &path, &query);
            let seconds = started.elapsed().as_secs_f64();
            let _ = sender.send(Message::Found { search, items, seconds });
        });
    }

    pub(crate) fn on_found(&mut self, items: Vec<Item>, seconds: f64) {
        self.showing_results = true;
        let found = plural(items.len() as u64, "result");
        self.list.set_items(items);
        self.sync_columns();
        let ui = self.ui();
        ui.set_in_search_results(true);
        ui.set_can_edit_folder(false);
        ui.set_list_message("Nothing matches this search.".into());
        self.activity = format!("{found} in {:.0} ms", seconds * 1000.0);
        ui.set_status_activity(self.activity.as_str().into());
        ui.invoke_scroll_to_top();
        self.restore_cut_marks();
        self.show_selection();
        self.start_hashing();
        self.apply_pending_selection();
    }

    // Summary pane

}
