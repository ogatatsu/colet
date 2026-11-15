//! The search: its query and filter, running a saved one picked in home, and leaving it.
use super::*;

impl State {
    /// Searches for `query` within the search filter already in use.
    #[cfg(test)]
    pub fn set_query(&mut self, query: &str) {
        let filter = self.search_filter.clone();
        self.set_search(query, filter);
    }

    /// Home stays beside the results; its pick has nothing to do with them, so it lets go. Only what is
    /// typed searches: a filter with nothing typed is no search, and is not kept. Another filter for what
    /// is typed is a step back and forth, as leaving the results is; a key typed is not.
    pub fn set_search(&mut self, query: &str, filter: Option<String>) {
        let searching = !query.is_empty();
        let filter = filter.filter(|_| searching);
        if query == self.query && filter == self.search_filter {
            return;
        }
        let was = self.pinned();
        let leaving = self.columns.len() > if was { 2 } else { 1 } || (was && (!searching || filter != self.search_filter));
        if leaving {
            let location = self.snapshot();
            self.remember(location);
        } else {
            self.future.clear();
        }
        let mut home = self.columns.swap_remove(0);
        home.selected.clear();
        home.focused = None;
        self.columns = vec![home];
        if searching {
            let search = self.column(Kind::Search, Path::new(""), None);
            self.columns.push(search);
        }
        self.query = query.into();
        self.search_filter = filter;
        self.active = if searching { 1 } else { 0 };
        self.fold = None;
    }

    /// The search filter `from` is named `to` now, or is gone: the way back and forth follows it. Gone
    /// from the results, they show without it.
    pub fn rename_search_filter(&mut self, from: &str, to: Option<&str>) {
        for location in self.past.iter_mut().chain(self.future.iter_mut()) {
            location.rename_search_filter(from, to);
        }
        if self.search_filter.as_deref() != Some(from) {
            return;
        }
        self.search_filter = to.map(Into::into);
        if to.is_none() {
            self.reset_search();
        }
    }

    /// Makes the search afresh, as after its filter changes; home is left be.
    pub fn reset_search(&mut self) {
        if let Some(index) = self.columns.iter().position(|c| c.kind == Kind::Search) {
            let selected = std::mem::take(&mut self.columns[index].selected);
            let mut fresh = self.column(Kind::Search, Path::new(""), None);
            fresh.selected = selected;
            self.columns[index] = fresh;
        }
    }

    /// What the search results are narrowed to, as a saved search keeps it.
    pub fn search_narrowing(&self) -> Option<String> {
        self.columns.iter().find(|c| c.kind == Kind::Search).and_then(Column::narrowing)
    }

    /// A saved search in home runs its query beside it, narrowed as it was kept. Home keeps the pick and
    /// the focus, so the next one is a keystroke away.
    pub(super) fn run_saved_search(&mut self, index: usize, path: PathBuf, here: Location) {
        let HomeKey::Search { query, filter, narrow, order } = HomeKey::of(&path) else { return };
        let same = self.query == query
            && self.search_filter.as_deref() == filter
            && self.search_narrowing().as_deref() == narrow
            && self.search_order == order;
        if same && self.columns.len() == 2 && self.columns[index].selected == [path.clone()] {
            self.active = index;
            return;
        }
        let (query, filter, narrow) = (query.to_owned(), filter.map(str::to_owned), narrow.map(str::to_owned));
        let mut home = self.columns.swap_remove(0);
        home.selected = vec![path.clone()];
        home.focused = Some(path);
        let search = self.column(Kind::Search, Path::new(""), None);
        self.columns = vec![home, search];
        self.set_narrowing(1, narrow.as_deref());
        self.query = query;
        self.search_filter = filter;
        self.search_order = order;
        self.active = index;
        self.fold = None;
        self.remember(here);
    }

    /// Picking a place in home while searching leaves the search, as a sidebar does in Finder; one step
    /// back returns to the results.
    pub(super) fn leave_search_for(&mut self, paths: Vec<PathBuf>, here: Location) {
        let past = std::mem::take(&mut self.past);
        self.query.clear();
        self.search_filter = None;
        self.columns.truncate(1);
        self.active = 0;
        self.fold = None;
        self.select(0, paths);
        self.past = past;
        self.remember(here);
    }
}
