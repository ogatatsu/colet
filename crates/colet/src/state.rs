//! Where the user is: the columns on screen, what each lists and has picked, and the way back and forward.
//! Knows nothing of drawing or of where listings come from.
use colet_core::{
    entry::Entry,
    paths::{ancestors, folder_name},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The drives, always the first column, in its own pane.
    Home,
    /// Everything's results for the query, pinned beside home.
    Search,
    Folder,
}

pub type ColumnId = u64;

#[derive(Clone, Debug)]
pub struct Column {
    pub id: ColumnId,
    pub kind: Kind,
    /// The folder a folder column lists; empty for the others.
    pub path: String,
    pub entries: Vec<Entry>,
    /// How many there are in all, of which `entries` may be the first pages.
    pub total: usize,
    pub selected: Vec<String>,
    /// The row the keys are on.
    pub focused: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
    /// The listing it waits for; any other that arrives is stale.
    pub request: u64,
    /// The search result or favorite it points at, apart from the selection.
    pub highlight: Option<String>,
}

impl Column {
    fn new(id: ColumnId, kind: Kind, path: &str, highlight: Option<String>) -> Self {
        Self {
            id,
            kind,
            path: path.into(),
            entries: Vec::new(),
            total: 0,
            selected: Vec::new(),
            focused: None,
            loading: true,
            error: None,
            request: 0,
            highlight,
        }
    }

    /// Loading with nothing listed yet: its total is not known, not 0.
    pub fn unread(&self) -> bool {
        self.loading && self.entries.is_empty()
    }

    pub fn title(&self) -> &str {
        match self.kind {
            Kind::Home => "ホーム",
            Kind::Search => "検索結果",
            Kind::Folder => folder_name(&self.path),
        }
    }

    pub fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }
}

/// A search result or favorite's own column, the anchor, and whether the folders above it are out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fold<A> {
    pub anchor: A,
    pub open: bool,
}

/// A place to go back or forward to.
#[derive(Clone, Debug)]
struct Location {
    query: String,
    folders: Vec<(String, Option<String>)>,
    selections: Vec<Vec<String>>,
    active: usize,
    /// Its anchor counted among the folders.
    fold: Option<Fold<usize>>,
}

/// The folder columns folded above the anchor: from `from` up to `to`, where it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Folded {
    pub from: usize,
    pub to: usize,
    pub open: bool,
}

/// Home keys a saved search by its query behind this; no path starts this way, a drive letter being one
/// character.
const SEARCH_KEY: &str = "search:";

pub fn search_key(query: &str) -> String {
    format!("{SEARCH_KEY}{query}")
}

/// The query a home row runs, when it is a saved search.
pub fn saved_query(path: &str) -> Option<&str> {
    path.strip_prefix(SEARCH_KEY)
}

/// How far back it goes.
const HISTORY: usize = 50;

pub struct State {
    pub query: String,
    pub columns: Vec<Column>,
    /// The column being worked in.
    pub active: usize,
    past: Vec<Location>,
    future: Vec<Location>,
    /// Something to tell the user, once.
    pub notice: Option<String>,
    /// A search result or favorite opens on its own column with the folders above it folded away until
    /// asked for. Gone with the anchor.
    pub fold: Option<Fold<ColumnId>>,
    next_id: ColumnId,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

/// A folder column to open to the right of a selection.
struct Spec {
    path: String,
    selected: Option<Vec<String>>,
    highlight: Option<String>,
}

/// The columns that show a search result or favorite where it lives: each ancestor with the next level
/// selected, ending in the folder that points at a file or the contents of a folder. A folder is pointed
/// at in its parent, as a file is, as well as selected there.
fn reveal_chain(entry: &Entry) -> Vec<Spec> {
    // A folder's chain runs down to the folder itself, to show its contents.
    let chain = ancestors(if entry.directory { &entry.path } else { &entry.parent });
    let last = chain.len().saturating_sub(1);
    chain
        .iter()
        .enumerate()
        .map(|(k, path)| {
            if k < last {
                Spec {
                    path: path.clone(),
                    selected: Some(vec![chain[k + 1].clone()]),
                    highlight: (entry.directory && k + 2 == chain.len()).then(|| entry.path.clone()),
                }
            } else {
                Spec {
                    path: path.clone(),
                    selected: Some(Vec::new()),
                    highlight: (!entry.directory).then(|| entry.path.clone()),
                }
            }
        })
        .collect()
}

impl State {
    pub fn new() -> Self {
        let mut state = Self {
            query: String::new(),
            columns: Vec::new(),
            active: 0,
            past: Vec::new(),
            future: Vec::new(),
            notice: None,
            fold: None,
            next_id: 0,
        };
        let home = state.column(Kind::Home, "", None);
        state.columns.push(home);
        state
    }

    fn column(&mut self, kind: Kind, path: &str, highlight: Option<String>) -> Column {
        self.next_id += 1;
        Column::new(self.next_id, kind, path, highlight)
    }

    pub fn can_go_back(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn index_of(&self, id: ColumnId) -> Option<usize> {
        self.columns.iter().position(|c| c.id == id)
    }

    /// Whether the search results stand pinned beside home.
    pub fn pinned(&self) -> bool {
        self.columns.get(1).is_some_and(|c| c.kind == Kind::Search)
    }

    pub fn folded(&self) -> Option<Folded> {
        let fold = self.fold?;
        let from = if self.pinned() { 2 } else { 1 };
        let to = self.index_of(fold.anchor)?;
        (to > from).then_some(Folded { from, to, open: fold.open })
    }

    /// How many folder columns are folded out of sight.
    pub fn folded_away(&self) -> usize {
        match self.folded() {
            Some(f) if !f.open => f.to - f.from,
            _ => 0,
        }
    }

    fn snapshot(&self) -> Location {
        let folders: Vec<&Column> = self.columns.iter().filter(|c| c.kind == Kind::Folder).collect();
        Location {
            query: self.query.clone(),
            folders: folders.iter().map(|c| (c.path.clone(), c.highlight.clone())).collect(),
            selections: self.columns.iter().map(|c| c.selected.clone()).collect(),
            active: self.active,
            fold: self.fold.and_then(|fold| {
                let anchor = folders.iter().position(|c| c.id == fold.anchor)?;
                Some(Fold { anchor, open: fold.open })
            }),
        }
    }

    fn remember(&mut self, location: Location) {
        if self.past.len() >= HISTORY {
            self.past.remove(0);
        }
        self.past.push(location);
        self.future.clear();
    }

    /// Home stays on screen through every move, so it keeps what it lists and only takes back its
    /// selection; the rest are made afresh.
    fn restore(&mut self, location: Location) {
        let home = self.columns.swap_remove(0);
        let mut columns = vec![home];
        if !location.query.is_empty() {
            columns.push(self.column(Kind::Search, "", None));
        }
        for (path, highlight) in &location.folders {
            columns.push(self.column(Kind::Folder, path, highlight.clone()));
        }
        for (i, c) in columns.iter_mut().enumerate() {
            c.selected = location.selections.get(i).cloned().unwrap_or_default();
            c.focused = c.selected.last().cloned();
        }
        let anchor = location.fold.and_then(|fold| {
            let column = columns.iter().filter(|c| c.kind == Kind::Folder).nth(fold.anchor)?;
            Some(Fold { anchor: column.id, open: fold.open })
        });
        self.query = location.query;
        self.active = location.active.min(columns.len() - 1);
        self.columns = columns;
        self.fold = anchor;
    }

    /// Home stays beside the results; its pick has nothing to do with them, so it lets go.
    pub fn set_query(&mut self, query: &str) {
        if query == self.query {
            return;
        }
        let leaving = self.columns.len() > if self.query.is_empty() { 1 } else { 2 }
            || (query.is_empty() && !self.query.is_empty());
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
        if !query.is_empty() {
            let search = self.column(Kind::Search, "", None);
            self.columns.push(search);
        }
        self.query = query.into();
        self.active = if query.is_empty() { 0 } else { 1 };
        self.fold = None;
    }

    pub fn set_active(&mut self, index: usize) {
        let active = index.min(self.columns.len() - 1);
        // A folded column brings the others out with it.
        if let Some(f) = self.folded()
            && !f.open
            && active >= f.from
            && active < f.to
        {
            self.unfold();
        }
        self.active = active;
    }

    /// Brings out the folders above a search result or favorite, for good.
    pub fn unfold(&mut self) {
        if let Some(fold) = &mut self.fold {
            fold.open = true;
        }
    }

    pub fn back(&mut self) {
        let Some(target) = self.past.pop() else { return };
        let here = self.snapshot();
        self.future.push(here);
        self.restore(target);
    }

    pub fn forward(&mut self) {
        let Some(target) = self.future.pop() else { return };
        let here = self.snapshot();
        self.past.push(here);
        self.restore(target);
    }

    /// Makes the search afresh, as after connecting to another Everything; home is left be.
    pub fn reset_search(&mut self) {
        if let Some(index) = self.columns.iter().position(|c| c.kind == Kind::Search) {
            let selected = std::mem::take(&mut self.columns[index].selected);
            let mut fresh = self.column(Kind::Search, "", None);
            fresh.selected = selected;
            self.columns[index] = fresh;
        }
    }

    pub fn select(&mut self, index: usize, paths: Vec<String>) {
        let Some(c) = self.columns.get(index) else { return };
        let here = self.snapshot();
        // A saved search in home runs its query beside it. Home keeps the pick and the focus, so the next
        // one is a keystroke away.
        if c.kind == Kind::Home
            && let [path] = paths.as_slice()
            && let Some(query) = saved_query(path)
        {
            if self.query == query && self.columns.len() == 2 && c.selected == paths {
                self.active = index;
                return;
            }
            let path = path.clone();
            let query = query.to_owned();
            let mut home = self.columns.swap_remove(0);
            home.selected = vec![path.clone()];
            home.focused = Some(path);
            let search = self.column(Kind::Search, "", None);
            self.columns = vec![home, search];
            self.query = query;
            self.active = index;
            self.fold = None;
            self.remember(here);
            return;
        }
        // Picking a place in home while searching leaves the search, as a sidebar does in Finder; one
        // step back returns to the results.
        if c.kind == Kind::Home
            && !self.query.is_empty()
            && paths.len() == 1
            && c.entries.iter().any(|e| e.path == paths[0])
        {
            let past = std::mem::take(&mut self.past);
            self.query.clear();
            self.columns.truncate(1);
            self.active = 0;
            self.fold = None;
            self.select(0, paths);
            self.past = past;
            self.remember(here);
            return;
        }
        let kind = c.kind;
        // Like Finder, only a single selection opens a column to the right.
        let entry = (paths.len() == 1).then(|| c.entry(&paths[0]).cloned()).flatten();
        // The pointer from a search result has done its job once the user picks something here; leaving
        // it would read as a second selection.
        {
            let c = &mut self.columns[index];
            c.focused = paths.last().cloned();
            c.selected = paths;
            c.highlight = None;
        }
        let specs = match &entry {
            None => Vec::new(),
            Some(entry) if kind != Kind::Folder => reveal_chain(entry),
            Some(entry) if entry.directory => {
                vec![Spec { path: entry.path.clone(), selected: None, highlight: None }]
            }
            Some(_) => Vec::new(),
        };
        // Columns already showing the same folder are kept, so stepping through results under a shared
        // ancestor does not reload it.
        let mut next = Vec::with_capacity(specs.len());
        let mut unchanged = !specs.is_empty();
        for (k, spec) in specs.into_iter().enumerate() {
            match self.columns.get(index + 1 + k) {
                Some(old) if old.path == spec.path => {
                    let mut old = old.clone();
                    let same_selection = spec.selected.as_ref().is_none_or(|s| *s == old.selected);
                    if !(same_selection && old.highlight == spec.highlight) {
                        unchanged = false;
                        if let Some(selected) = spec.selected {
                            old.selected = selected;
                        }
                        old.focused = spec.highlight.clone().or_else(|| old.selected.last().cloned());
                        old.highlight = spec.highlight;
                    }
                    next.push(old);
                }
                _ => {
                    unchanged = false;
                    let mut fresh = self.column(Kind::Folder, &spec.path, spec.highlight);
                    fresh.selected = spec.selected.unwrap_or_default();
                    fresh.focused = fresh.selected.last().cloned();
                    next.push(fresh);
                }
            }
        }
        // A folder reopened from a folder column keeps what was open below it; a search result or
        // favorite always shows exactly its own chain.
        if unchanged && (kind == Kind::Folder || self.columns.len() == index + 1 + next.len()) {
            self.active = index;
            return;
        }
        let anchor = next.last().map(|c| c.id);
        self.columns.truncate(index + 1);
        self.columns.extend(next);
        self.active = index;
        // Only a pick that opens something is a place to come back to.
        if entry.is_some() {
            // Picked afresh from the results or home, its own column stands alone.
            if kind != Kind::Folder {
                self.fold = anchor.map(|anchor| Fold { anchor, open: false });
            }
            self.remember(here);
        }
    }

    /// The column ← or → leads to from `index`, if any. From search results or home, → lands where the
    /// pick is shown rather than on the drive at the top of its ancestors; folded-away folders are passed
    /// over, ← going back to the results or home, → on to the column they fold above.
    pub fn neighbour(&self, index: usize, right: bool) -> Option<usize> {
        let mut next = if right { index + 1 } else { index.checked_sub(1)? };
        let c = &self.columns[index];
        if right
            && c.kind != Kind::Folder
            && let Some(result) = &c.focused
            && let Some(shown) = self.columns.iter().enumerate().skip(index + 1).position(|(_, c)| {
                c.highlight.as_ref() == Some(result) || c.selected.contains(result)
            })
        {
            next = index + 1 + shown;
        }
        if let Some(f) = self.folded()
            && !f.open
            && next >= f.from
            && next < f.to
        {
            next = if right { f.to } else { f.from - 1 };
        }
        (next < self.columns.len()).then_some(next)
    }

    /// The keys arrive in the active column: they land on its focused row, or the item it points at,
    /// which is picked while nothing else is; with neither, on home's first row, unpicked (a pick there
    /// opens a drive), or any other column's, picked.
    pub fn enter(&mut self) {
        let index = self.active;
        let c = &self.columns[index];
        let listed = |path: &String| c.entry(path).is_some();
        if let Some(target) = c.focused.clone().filter(listed).or_else(|| c.highlight.clone().filter(listed)) {
            if c.selected.is_empty() && c.highlight.as_ref() == Some(&target) {
                self.select(index, vec![target]);
            }
        } else if let Some(first) = c.entries.first().map(|e| e.path.clone()) {
            if c.kind == Kind::Home {
                self.columns[index].focused = Some(first);
            } else {
                self.select(index, vec![first]);
            }
        }
    }

    /// Moves the keys `by` rows in the active column (clamped at its ends), picking the row they land on.
    /// With none focused yet, they start from the item the column points at, or above its first row.
    pub fn step(&mut self, by: isize) {
        let index = self.active;
        let c = &self.columns[index];
        if c.entries.is_empty() {
            return;
        }
        let from = c.focused.as_ref().or(c.highlight.as_ref()).and_then(|p| c.entries.iter().position(|e| &e.path == p));
        let last = c.entries.len() as isize - 1;
        let to = match from {
            Some(from) => (from as isize + by).clamp(0, last),
            None if by > 0 => (by - 1).clamp(0, last),
            None => 0,
        } as usize;
        let path = c.entries[to].path.clone();
        if c.selected.len() == 1 && c.selected[0] == path {
            return;
        }
        self.select(index, vec![path]);
    }

    /// The column with `id` starts loading `request`; `quiet` keeps what it shows meanwhile.
    pub fn load(&mut self, id: ColumnId, request: u64, quiet: bool) {
        let Some(index) = self.index_of(id) else { return };
        let c = &mut self.columns[index];
        c.loading = !quiet;
        c.error = None;
        c.request = request;
    }

    pub fn error(&mut self, id: ColumnId, request: u64, error: String) {
        let Some(index) = self.index_of(id) else { return };
        let c = &mut self.columns[index];
        if c.request != request {
            return;
        }
        c.loading = false;
        c.error = Some(error);
    }

    /// A listing arrived: `entries` from `offset` on, of `total`. A page after the first adds to what is
    /// listed; the first replaces it.
    pub fn page(&mut self, id: ColumnId, request: u64, entries: Vec<Entry>, total: usize, offset: usize) {
        let Some(index) = self.index_of(id) else { return };
        let c = &mut self.columns[index];
        if c.request != request {
            return;
        }
        if offset > 0 {
            let known: std::collections::HashSet<String> = c.entries.iter().map(|e| e.path.clone()).collect();
            c.entries.extend(entries.into_iter().filter(|e| !known.contains(&e.path)));
        } else {
            c.entries = entries;
        }
        let paths: std::collections::HashSet<&str> = c.entries.iter().map(|e| e.path.as_str()).collect();
        c.selected.retain(|p| paths.contains(p.as_str()));
        let focused = match (&c.focused, &c.highlight) {
            (Some(f), _) if paths.contains(f.as_str()) => Some(f.clone()),
            (_, Some(h)) if paths.contains(h.as_str()) => Some(h.clone()),
            _ => None,
        };
        c.focused = focused;
        c.total = total;
        c.loading = false;
        c.error = None;
        let kind = c.kind;
        // The folder it had open beside it is gone: close it and what it opened.
        let vanished = kind == Kind::Folder
            && self.columns.get(index + 1).is_some_and(|child| {
                !self.columns[index].entries.iter().any(|e| e.path == child.path && e.directory)
            });
        if vanished {
            self.columns.truncate(index + 1);
            self.active = self.active.min(index);
            self.notice = Some("選択していたフォルダーがなくなったため、右側のカラムを閉じました。".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colet_core::paths::ancestors;

    fn entry(path: &str, directory: bool) -> Entry {
        let chain = ancestors(path);
        Entry {
            path: path.into(),
            name: path.into(),
            parent: chain.len().checked_sub(2).map(|i| chain[i].clone()).unwrap_or_default(),
            directory,
            size: None,
            modified: None,
            hidden: false,
        }
    }

    fn folder(path: &str) -> Entry {
        entry(path, true)
    }

    fn file(path: &str) -> Entry {
        entry(path, false)
    }

    fn with_entries(s: &mut State, index: usize, entries: Vec<Entry>) {
        s.columns[index].entries = entries;
    }

    fn paths(s: &State) -> Vec<&str> {
        s.columns.iter().map(|c| c.path.as_str()).collect()
    }

    /// Each folder column after home: its path, selection and highlight.
    fn chain(s: &State) -> Vec<(String, Vec<String>, Option<String>)> {
        s.columns[1..].iter().map(|c| (c.path.clone(), c.selected.clone(), c.highlight.clone())).collect()
    }

    fn link(path: &str, selected: &[&str], highlight: Option<&str>) -> (String, Vec<String>, Option<String>) {
        (path.into(), selected.iter().map(|s| s.to_string()).collect(), highlight.map(Into::into))
    }

    fn one(path: &str) -> Vec<String> {
        vec![path.into()]
    }

    #[test]
    fn removes_vanished_selection_and_focus_after_a_refresh() {
        for kind in [Kind::Search, Kind::Home, Kind::Folder] {
            let mut s = State::new();
            let (kept, gone) = (file(r"D:\kept"), file(r"D:\gone"));
            let mut c = s.column(kind, "", None);
            c.entries = vec![kept.clone(), gone.clone()];
            c.selected = vec![kept.path.clone(), gone.path.clone()];
            c.focused = Some(gone.path.clone());
            c.request = 2;
            let id = c.id;
            s.columns = vec![c];
            s.page(id, 2, vec![kept.clone()], 1, 0);
            assert_eq!(s.columns[0].selected, std::slice::from_ref(&kept.path));
            assert_eq!(s.columns[0].focused, None);
            s.page(id, 2, vec![], 0, 0);
            assert!(s.columns[0].selected.is_empty());
        }
    }

    #[test]
    fn keeps_selections_on_appended_pages_but_drops_unloaded_selections_on_refresh() {
        let entries: Vec<Entry> = (0..501).map(|i| file(&format!(r"D:\file{i}"))).collect();
        let (first, last) = (entries[0].path.clone(), entries[500].path.clone());
        let mut s = State::new();
        s.set_query("file");
        let id = s.columns[1].id;
        s.load(id, 2, false);
        s.page(id, 2, entries[..500].to_vec(), 501, 0);
        s.columns[1].selected = one(&first);
        s.columns[1].focused = Some(first.clone());
        s.page(id, 2, entries[500..].to_vec(), 501, 500);
        assert_eq!(s.columns[1].entries.len(), 501);
        assert_eq!(s.columns[1].selected, one(&first));
        assert_eq!(s.columns[1].focused.as_deref(), Some(first.as_str()));
        s.select(1, vec![first.clone(), last.clone()]);
        // A stale listing changes nothing.
        s.page(id, 1, entries[..500].to_vec(), 501, 0);
        assert_eq!(s.columns[1].selected, [first.clone(), last.clone()]);
        s.page(id, 2, entries[..500].to_vec(), 501, 0);
        assert_eq!(s.columns[1].selected, one(&first));
        assert_eq!(s.columns[1].focused, None);
    }

    #[test]
    fn discards_stale_and_removed_column_responses() {
        let mut s = State::new();
        let home = s.columns[0].id;
        s.load(home, 2, false);
        s.page(home, 1, vec![folder("old")], 1, 0);
        assert!(s.columns[0].entries.is_empty());
        s.set_query("old");
        let search = s.columns[1].id;
        s.load(search, 3, false);
        s.set_query("new");
        s.error(search, 3, "old".into());
        assert!(s.columns[1].error.is_none());
    }

    #[test]
    fn replaces_descendants_and_opens_a_search_file_parent_exactly_once() {
        let target = file(r"D:\parent\file");
        let mut s = State::new();
        s.set_query("test");
        with_entries(&mut s, 1, vec![target.clone(), folder(r"D:\other")]);
        s.select(1, one(&target.path));
        assert_eq!(paths(&s), ["", "", r"D:\", r"D:\parent"]);
        assert_eq!(s.columns[3].highlight.as_deref(), Some(target.path.as_str()));
        with_entries(&mut s, 3, vec![target.clone()]);
        s.select(3, one(&target.path));
        assert_eq!(s.columns.len(), 4);
        assert_eq!(s.columns[3].highlight, None);
        s.select(1, one(r"D:\other"));
        assert_eq!(paths(&s), ["", "", r"D:\", r"D:\other"]);
    }

    #[test]
    fn reveals_a_search_file_under_all_of_its_ancestors() {
        let target = file(r"D:\a\b\x.txt");
        let mut s = State::new();
        s.set_query("x");
        with_entries(&mut s, 1, vec![target.clone()]);
        s.select(1, one(&target.path));
        assert_eq!(
            chain(&s)[1..],
            [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\b"], None), link(r"D:\a\b", &[], Some(&target.path))]
        );
        assert_eq!(s.active, 1);
    }

    #[test]
    fn reveals_a_search_folder_and_opens_its_contents() {
        let target = folder(r"D:\a\b");
        let mut s = State::new();
        s.set_query("b");
        with_entries(&mut s, 1, vec![target.clone()]);
        s.select(1, one(&target.path));
        assert_eq!(
            chain(&s)[1..],
            [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\b"], Some(r"D:\a\b")), link(r"D:\a\b", &[], None)]
        );
        // Picking in its parent takes the pointer away, as for a file.
        with_entries(&mut s, 3, vec![target.clone(), folder(r"D:\a\c")]);
        s.select(3, one(r"D:\a\c"));
        assert_eq!(chain(&s)[2..], [link(r"D:\a", &[r"D:\a\c"], None), link(r"D:\a\c", &[], None)]);
    }

    #[test]
    fn keeps_shared_ancestor_columns_when_moving_between_search_results() {
        let (a, b) = (file(r"D:\a\b\one.txt"), file(r"D:\a\c\two.txt"));
        let mut s = State::new();
        s.set_query("t");
        with_entries(&mut s, 1, vec![a.clone(), b.clone()]);
        s.select(1, one(&a.path));
        let ids: Vec<_> = s.columns.iter().map(|c| c.id).collect();
        let past = s.past.len();
        s.select(1, one(&a.path));
        assert_eq!(s.columns.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
        assert_eq!(s.past.len(), past);
        s.select(1, one(&b.path));
        assert_eq!(s.columns[..4].iter().map(|c| c.id).collect::<Vec<_>>(), ids[..4]);
        assert_eq!(
            chain(&s)[1..],
            [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\c"], None), link(r"D:\a\c", &[], Some(&b.path))]
        );
        s.back();
        assert_eq!(
            chain(&s)[1..],
            [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\b"], None), link(r"D:\a\b", &[], Some(&a.path))]
        );
    }

    #[test]
    fn drops_a_deeper_results_columns_when_revealing_its_ancestor_folder() {
        let (deep, top) = (file(r"D:\a\b\x.txt"), folder(r"D:\a"));
        let mut s = State::new();
        s.set_query("x");
        with_entries(&mut s, 1, vec![deep.clone(), top.clone()]);
        s.select(1, one(&deep.path));
        s.select(1, one(&top.path));
        assert_eq!(chain(&s)[1..], [link(r"D:\", &[r"D:\a"], Some(r"D:\a")), link(r"D:\a", &[], None)]);
    }

    #[test]
    fn folds_away_the_folders_above_a_result_until_shown() {
        let (a, c) = (file(r"D:\a\b\one.txt"), folder(r"D:\a\c"));
        let mut s = State::new();
        s.set_query("o");
        with_entries(&mut s, 1, vec![a.clone(), c.clone()]);
        s.select(1, one(&a.path));
        // D: and a, above b.
        assert_eq!(s.folded(), Some(Folded { from: 2, to: 4, open: false }));
        assert_eq!(s.folded_away(), 2);
        // Taking the keys to a folded column brings the folders out, for good.
        s.set_active(3);
        assert_eq!(s.folded_away(), 0);
        s.set_active(4);
        assert_eq!(s.folded_away(), 0);
        let past = s.past.len();
        s.select(1, one(&c.path));
        assert_eq!(s.past.len(), past + 1);
        // A folder opens on its contents alone; picked afresh, folded again.
        assert_eq!(s.folded(), Some(Folded { from: 2, to: 4, open: false }));
        assert_eq!(s.columns[4].path, r"D:\a\c");
        // Gone back to, a place is folded as it was left; going forward too.
        s.back();
        assert_eq!(s.columns[4].path, r"D:\a\b");
        assert!(s.folded().unwrap().open);
        s.forward();
        assert!(!s.folded().unwrap().open);
        // Picking in a folder above it, once out, closes the anchor and the fold.
        s.unfold();
        with_entries(&mut s, 2, vec![folder(r"D:\a"), folder(r"D:\z")]);
        s.select(2, one(r"D:\z"));
        assert_eq!(s.folded(), None);
        // Nothing above a file at a drive's top: nothing to fold.
        let top = file(r"D:\top.txt");
        with_entries(&mut s, 1, vec![top.clone()]);
        s.select(1, one(&top.path));
        assert_eq!(s.folded(), None);
    }

    #[test]
    fn reveals_a_drive_from_home_on_its_own() {
        let drive = Entry::folder(r"D:\", "データ (D:)");
        let mut s = State::new();
        with_entries(&mut s, 0, vec![drive.clone()]);
        s.select(0, one(&drive.path));
        assert_eq!(chain(&s), [link(r"D:\", &[], None)]);
    }

    #[test]
    fn closes_descendants_without_history_on_multiple_or_empty_selection() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"D:\a"), folder(r"D:\b")]);
        s.select(0, one(r"D:\a"));
        assert_eq!(s.columns.len(), 3);
        let past = s.past.len();
        s.select(0, vec![r"D:\a".into(), r"D:\b".into()]);
        assert_eq!(s.columns.len(), 1);
        assert_eq!(s.columns[0].selected.len(), 2);
        assert_eq!(s.columns[0].focused.as_deref(), Some(r"D:\b"));
        assert_eq!(s.past.len(), past);
        s.select(0, vec![]);
        assert!(s.columns[0].selected.is_empty());
        assert_eq!(s.past.len(), past);
    }

    #[test]
    fn restores_selections_when_going_back_and_forward() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"D:\a"), folder(r"D:\b")]);
        s.select(0, one(r"D:\a"));
        s.select(0, one(r"D:\b"));
        s.back();
        assert_eq!(s.columns[0].selected, one(r"D:\a"));
        assert_eq!(s.columns[0].focused.as_deref(), Some(r"D:\a"));
        assert_eq!(s.columns.last().unwrap().path, r"D:\a");
        s.forward();
        assert_eq!(s.columns.last().unwrap().path, r"D:\b");
    }

    #[test]
    fn does_not_add_each_query_keystroke_to_history() {
        let mut s = State::new();
        for query in ["a", "ab", "abc"] {
            s.set_query(query);
        }
        assert!(s.past.is_empty());
        with_entries(&mut s, 1, vec![folder(r"D:\a")]);
        s.select(1, one(r"D:\a"));
        s.back();
        assert_eq!(s.query, "abc");
        assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
        s.forward();
        assert_eq!(s.columns.last().unwrap().path, r"D:\a");
    }

    #[test]
    fn closes_a_vanished_folder_and_what_it_opened() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"D:\a")]);
        s.select(0, one(r"D:\a"));
        let parent = s.columns[1].id;
        s.load(parent, 4, false);
        s.page(parent, 4, vec![folder(r"D:\a\b")], 1, 0);
        s.select(1, one(r"D:\a\b"));
        s.set_active(2);
        s.page(parent, 4, vec![], 0, 0);
        assert_eq!(s.columns.len(), 2);
        assert_eq!(s.active, 1);
        assert!(s.notice.is_some());
    }

    #[test]
    fn keeps_home_beside_the_search_letting_go_of_its_pick() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"D:\")]);
        s.select(0, one(r"D:\"));
        s.set_query("x");
        assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
        assert!(s.columns[0].selected.is_empty());
        assert_eq!(s.active, 1);
    }

    #[test]
    fn opens_a_deep_search_file_on_its_own_folder_and_right_jumps_to_it() {
        let deep = file(r"D:\a\b\zeta.txt");
        let mut s = State::new();
        s.set_query("zeta");
        with_entries(&mut s, 1, vec![deep.clone()]);
        s.select(1, one(&deep.path));
        with_entries(&mut s, 4, vec![deep.clone()]);
        // → passes over the folded folders to the one that shows it, and picks it there.
        let next = s.neighbour(1, true).unwrap();
        assert_eq!(next, 4);
        s.set_active(next);
        s.enter();
        assert_eq!(s.columns[4].selected, one(&deep.path));
        assert_eq!(s.folded_away(), 2);
        // ← passes over them, back to the results.
        assert_eq!(s.neighbour(4, false), Some(1));
        assert_eq!(s.neighbour(0, false), None);
        assert_eq!(s.neighbour(4, true), None);
    }

    #[test]
    fn steps_through_rows_picking_each() {
        let mut s = State::new();
        s.set_query("a");
        with_entries(&mut s, 1, vec![folder(r"D:\a"), folder(r"D:\b"), file(r"D:\c")]);
        s.step(1);
        assert_eq!(s.columns[1].selected, one(r"D:\a"));
        s.step(5);
        assert_eq!(s.columns[1].selected, one(r"D:\c"));
        s.step(-1);
        assert_eq!(s.columns[1].selected, one(r"D:\b"));
        assert_eq!(s.columns.last().unwrap().path, r"D:\b");
    }

    #[test]
    fn enters_home_without_picking_and_results_picking_the_first() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"C:\"), folder(r"D:\")]);
        s.enter();
        assert_eq!(s.columns[0].focused.as_deref(), Some(r"C:\"));
        assert!(s.columns[0].selected.is_empty());
        s.set_query("x");
        with_entries(&mut s, 1, vec![file(r"D:\x.txt")]);
        s.enter();
        assert_eq!(s.columns[1].selected, one(r"D:\x.txt"));
    }

    #[test]
    fn runs_a_saved_search_picked_in_home_keeping_homes_pick() {
        let mut s = State::new();
        let pdf = search_key("ext:pdf");
        with_entries(&mut s, 0, vec![Entry::folder(&pdf, "PDF"), folder(r"D:\")]);
        s.select(0, one(&pdf));
        assert_eq!(s.query, "ext:pdf");
        assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
        assert_eq!(s.columns[0].selected, one(&pdf));
        assert_eq!(s.active, 0);
        // Picked again, nothing changes; a place picked leaves the search, one step back.
        let search = s.columns[1].id;
        s.select(0, one(&pdf));
        assert_eq!(s.columns[1].id, search);
        s.select(0, one(r"D:\"));
        assert_eq!(s.query, "");
        s.back();
        assert_eq!(s.query, "ext:pdf");
    }

    #[test]
    fn makes_the_search_afresh_on_reset_and_leaves_home_be() {
        let mut s = State::new();
        s.set_query("x");
        s.columns[1].selected = one(r"D:\x");
        let (home, search) = (s.columns[0].id, s.columns[1].id);
        s.reset_search();
        assert_eq!(s.columns[0].id, home);
        assert_ne!(s.columns[1].id, search);
        assert_eq!(s.columns[1].selected, one(r"D:\x"));
    }

    #[test]
    fn leaves_the_search_for_a_place_picked_in_home_one_step_back() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"D:\")]);
        s.set_query("x");
        s.select(0, one(r"D:\"));
        assert_eq!(s.query, "");
        assert_eq!(paths(&s), ["", r"D:\"]);
        s.back();
        assert_eq!(s.query, "x");
        assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
    }
}
