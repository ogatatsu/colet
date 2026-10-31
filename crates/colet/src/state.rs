//! Where the user is: the columns on screen, what each lists and has picked, and the way back and forward.
//! Knows nothing of drawing or of where listings come from.
use std::{
    borrow::Cow,
    collections::HashSet,
    ffi::OsString,
    ops::Range,
    path::{Path, PathBuf},
};

use colet_core::{
    entry::{Entry, Marks},
    paths::{self, ancestors, folder_name, parent_folder, path_key, shown},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The drives, always the first column, in its own pane.
    Home,
    /// Everything's results for the query and the search filter, pinned beside home.
    Search,
    Folder,
}

pub type ColumnId = u64;

#[derive(Clone, Debug)]
pub struct Column {
    pub id: ColumnId,
    pub kind: Kind,
    /// The folder a folder column lists; empty for the others.
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    /// How many there are in all, of which `entries` may be the first pages.
    pub total: usize,
    pub selected: Vec<PathBuf>,
    /// The row the keys are on.
    pub focused: Option<PathBuf>,
    /// The row the keys were on until a filter hid it: they go back to it once it shows again, unless
    /// they have gone to another since.
    hid: Option<PathBuf>,
    pub loading: bool,
    pub error: Option<String>,
    /// The listing it waits for; any other that arrives is stale.
    pub request: u64,
    /// The search result or favorite it points at, apart from the selection.
    pub highlight: Option<PathBuf>,
    /// Where a Shift range starts: the row last picked on its own.
    pub anchor: Option<PathBuf>,
    /// What was picked before the first Ctrl+Shift range from the anchor, which the next ones add to
    /// in its place; gone once the anchor moves.
    base: Option<Vec<PathBuf>>,
    /// What to pick once listed, by path key: the items just pasted here, which the listing that shows
    /// them may come after the paste is done.
    pub pick: Option<Vec<OsString>>,
    /// A column narrowed to some names: `entries` then holds only those.
    pub filter: Option<Filter>,
}

/// The names a column is narrowed to, those holding `text`, and all it lists, kept aside.
#[derive(Clone, Debug)]
pub struct Filter {
    pub text: String,
    all: Vec<Entry>,
    /// The search results, which Everything narrows, keep nothing aside: how many the search finds
    /// without the filter stands for them.
    pub whole: usize,
}

/// Where `needle` stands in `name`, case aside, each time it does: byte ranges of `name`.
pub fn found(name: &str, needle: &str) -> Vec<Range<usize>> {
    let needle = needle.to_lowercase();
    let mut ranges = Vec::new();
    if needle.is_empty() {
        return ranges;
    }
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        // A letter may lower into more than one, so the name is lowered a letter at a time from here.
        let (mut lowered, mut j) = (String::new(), i);
        while j < chars.len() && lowered.len() < needle.len() {
            lowered.extend(chars[j].1.to_lowercase());
            j += 1;
        }
        if lowered == needle {
            ranges.push(chars[i].0..chars.get(j).map_or(name.len(), |c| c.0));
            i = j;
        } else {
            i += 1;
        }
    }
    ranges
}

/// The words a filter's `text` asks a name to hold, apart at any space, half or full width, but in quotes,
/// which are taken out. A quote left open is closed at the end, as Everything closes it.
pub fn filter_words(text: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted) = (Vec::new(), String::new(), false);
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => words.extend((!word.is_empty()).then(|| std::mem::take(&mut word))),
            c => word.push(c),
        }
    }
    words.extend((!word.is_empty()).then_some(word));
    words
}

/// What a filter's field holds, made ready for a word typed after it: a space put between, and a quote
/// left open closed first, so that the word is one of its own.
pub fn room_for_a_word(held: &str) -> String {
    let open = held.matches('"').count() % 2 == 1;
    if held.is_empty() || !open && held.ends_with(char::is_whitespace) {
        return held.into();
    }
    format!("{held}{} ", if open { "\"" } else { "" })
}

/// Where `name` holds each of the words of `text`, in any order, if it holds them all: the ranges of all
/// of them, those that meet as one. No words, nothing marked, are held by any name.
pub fn holds(name: &str, text: &str) -> Option<Vec<Range<usize>>> {
    let mut ranges = Vec::new();
    for word in filter_words(text) {
        let at = found(name, &word);
        if at.is_empty() {
            return None;
        }
        ranges.extend(at);
    }
    ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    Some(merged)
}

/// `entry` with its marks on where its name holds the words of `text`, if it holds them all.
fn narrowed(entry: &Entry, text: &str) -> Option<Entry> {
    let name = holds(&entry.name, text)?;
    Some(Entry { marks: Marks { name, parent: Vec::new() }, ..entry.clone() })
}

impl Column {
    fn new(id: ColumnId, kind: Kind, path: &Path, highlight: Option<PathBuf>) -> Self {
        Self {
            id,
            kind,
            path: path.into(),
            entries: Vec::new(),
            total: 0,
            selected: Vec::new(),
            focused: None,
            hid: None,
            loading: true,
            error: None,
            request: 0,
            highlight,
            anchor: None,
            base: None,
            pick: None,
            filter: None,
        }
    }

    /// The row the keys land on `by` rows from theirs (clamped at the ends); with none yet, from the item
    /// the column points at, or above its first row.
    fn landing(&self, by: isize) -> Option<usize> {
        let last = self.entries.len().checked_sub(1)? as isize;
        let from = self.focused.as_ref().or(self.highlight.as_ref()).and_then(|p| self.entries.iter().position(|e| &e.path == p));
        Some(match from {
            Some(from) => (from as isize + by).clamp(0, last),
            None if by > 0 => (by - 1).clamp(0, last),
            None => 0,
        } as usize)
    }

    /// Where a Shift range to `path` starts, and the rows it takes in.
    fn range_to(&self, path: &Path) -> Option<(Option<PathBuf>, Vec<PathBuf>)> {
        let position = |p: &PathBuf| self.entries.iter().position(|e| &e.path == p);
        let to = self.entries.iter().position(|e| e.path == path)?;
        let anchor = self.anchor.clone().or_else(|| self.focused.clone()).filter(|a| position(a).is_some());
        let from = anchor.as_ref().and_then(position).unwrap_or(to);
        let range = self.entries[from.min(to)..=from.max(to)].iter().map(|e| e.path.clone()).collect();
        Some((anchor, range))
    }

    /// Loading with nothing listed yet: its total is not known, not 0.
    pub fn unread(&self) -> bool {
        self.loading && self.entries.is_empty()
    }

    pub fn title(&self) -> Cow<'_, str> {
        match self.kind {
            Kind::Home => "ホーム".into(),
            Kind::Search => "検索結果".into(),
            Kind::Folder => folder_name(&self.path).to_string_lossy(),
        }
    }

    pub fn entry(&self, path: &Path) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// The picks it shows, which are what is acted on: those a filter hides stay picked only to keep what
    /// they opened open.
    pub fn shown_selected(&self) -> Cow<'_, [PathBuf]> {
        if self.filter.is_none() {
            return Cow::Borrowed(&self.selected);
        }
        let listed: HashSet<&Path> = self.entries.iter().map(|e| e.path.as_path()).collect();
        self.selected.iter().filter(|p| listed.contains(p.as_path())).cloned().collect()
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
    search_filter: Option<String>,
    folders: Vec<(PathBuf, Option<PathBuf>)>,
    selections: Vec<Vec<PathBuf>>,
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

/// Home keys a saved search by its query behind this, and its filter's name after a NUL, which neither
/// can hold; no path starts this way, a drive letter being one character.
const SEARCH_KEY: &str = "search:";

pub fn search_key(query: &str, filter: Option<&str>) -> PathBuf {
    match filter {
        Some(filter) => format!("{SEARCH_KEY}{query}\0{filter}").into(),
        None => format!("{SEARCH_KEY}{query}").into(),
    }
}

/// The query a home row runs and the filter it runs within, when it is a saved search.
pub fn saved_query(path: &Path) -> Option<(&str, Option<&str>)> {
    let key = path.to_str()?.strip_prefix(SEARCH_KEY)?;
    Some(match key.split_once('\0') {
        Some((query, filter)) => (query, Some(filter)),
        None => (key, None),
    })
}

/// How far back it goes.
const HISTORY: usize = 50;

pub struct State {
    pub query: String,
    /// The search filter the results are narrowed by, by name; with one, they show with no query too.
    pub search_filter: Option<String>,
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
    path: PathBuf,
    selected: Option<Vec<PathBuf>>,
    highlight: Option<PathBuf>,
}

/// The columns that show a search result or favorite where it lives: each ancestor with the next level
/// selected, ending in the folder that points at a file or the contents of a folder. A folder is pointed
/// at in its parent, as a file is, as well as selected there.
fn reveal_chain(entry: &Entry) -> Vec<Spec> {
    // A folder's chain runs down to the folder itself, to show its contents.
    let chain = ancestors(if entry.directory { &entry.path } else { parent_folder(&entry.path).unwrap_or(Path::new("")) });
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
            search_filter: None,
            columns: Vec::new(),
            active: 0,
            past: Vec::new(),
            future: Vec::new(),
            notice: None,
            fold: None,
            next_id: 0,
        };
        let home = state.column(Kind::Home, Path::new(""), None);
        state.columns.push(home);
        state
    }

    fn column(&mut self, kind: Kind, path: &Path, highlight: Option<PathBuf>) -> Column {
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
            search_filter: self.search_filter.clone(),
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
        if !location.query.is_empty() || location.search_filter.is_some() {
            columns.push(self.column(Kind::Search, Path::new(""), None));
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
        self.search_filter = location.search_filter;
        self.active = location.active.min(columns.len() - 1);
        self.columns = columns;
        self.fold = anchor;
    }

    /// Searches for `query` within the search filter already in use.
    #[cfg(test)]
    pub fn set_query(&mut self, query: &str) {
        let filter = self.search_filter.clone();
        self.set_search(query, filter);
    }

    /// Home stays beside the results; its pick has nothing to do with them, so it lets go. A filter alone
    /// searches too. Another filter is a step back and forth, as leaving the results is; a key typed is not.
    pub fn set_search(&mut self, query: &str, filter: Option<String>) {
        if query == self.query && filter == self.search_filter {
            return;
        }
        let (was, searching) = (self.pinned(), !query.is_empty() || filter.is_some());
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
    /// from the results, they show without it, or go with it when nothing was typed.
    pub fn rename_search_filter(&mut self, from: &str, to: Option<&str>) {
        for location in self.past.iter_mut().chain(self.future.iter_mut()) {
            if location.search_filter.as_deref() == Some(from) {
                location.search_filter = to.map(Into::into);
                // With nothing typed, the results column goes from it, and what was picked there with it.
                if to.is_none() && location.query.is_empty() {
                    if location.selections.len() > 1 {
                        location.selections.remove(1);
                    }
                    location.active = location.active.saturating_sub(1);
                }
            }
        }
        if self.search_filter.as_deref() != Some(from) {
            return;
        }
        self.search_filter = to.map(Into::into);
        if to.is_some() {
            return;
        }
        if self.query.is_empty() {
            self.columns.truncate(1);
            self.active = 0;
            self.fold = None;
        } else {
            self.reset_search();
        }
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
            let mut fresh = self.column(Kind::Search, Path::new(""), None);
            fresh.selected = selected;
            self.columns[index] = fresh;
        }
    }

    pub fn select(&mut self, index: usize, paths: Vec<PathBuf>) {
        let Some(c) = self.columns.get(index) else { return };
        let here = self.snapshot();
        // A saved search in home runs its query beside it. Home keeps the pick and the focus, so the next
        // one is a keystroke away.
        if c.kind == Kind::Home
            && let [path] = paths.as_slice()
            && let Some((query, filter)) = saved_query(path)
        {
            if self.query == query && self.search_filter.as_deref() == filter && self.columns.len() == 2 && c.selected == paths {
                self.active = index;
                return;
            }
            let path = path.clone();
            let (query, filter) = (query.to_owned(), filter.map(str::to_owned));
            let mut home = self.columns.swap_remove(0);
            home.selected = vec![path.clone()];
            home.focused = Some(path);
            let search = self.column(Kind::Search, Path::new(""), None);
            self.columns = vec![home, search];
            self.query = query;
            self.search_filter = filter;
            self.active = index;
            self.fold = None;
            self.remember(here);
            return;
        }
        // Picking a place in home while searching leaves the search, as a sidebar does in Finder; one
        // step back returns to the results.
        if c.kind == Kind::Home
            && self.pinned()
            && paths.len() == 1
            && c.entries.iter().any(|e| e.path == paths[0])
        {
            let past = std::mem::take(&mut self.past);
            self.query.clear();
            self.search_filter = None;
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
            c.anchor = paths.last().cloned();
            c.base = None;
            c.selected = paths;
            c.highlight = None;
            c.pick = None;
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
        let listed = |path: &PathBuf| c.entry(path).is_some();
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
        let Some(to) = c.landing(by) else { return };
        let path = c.entries[to].path.clone();
        if c.selected.len() == 1 && c.selected[0] == path {
            return;
        }
        self.select(index, vec![path]);
    }

    /// Ctrl+↑↓: moves the keys `by` rows in the active column as `step` does, picking nothing.
    pub fn focus_step(&mut self, by: isize) {
        let c = &mut self.columns[self.active];
        if let Some(to) = c.landing(by) {
            c.focused = Some(c.entries[to].path.clone());
        }
    }

    /// Ctrl+Space: adds the row the keys are on to the selection, or takes it out; with none, the row
    /// they would land on.
    pub fn toggle_focused(&mut self) {
        let index = self.active;
        let c = &self.columns[index];
        let listed = |p: &&PathBuf| c.entry(p).is_some();
        let target = c.focused.as_ref().filter(listed).or(c.highlight.as_ref().filter(listed)).or(c.entries.first().map(|e| &e.path));
        if let Some(path) = target.cloned() {
            self.toggle(index, &path);
        }
    }

    /// What QuickLook previews: the row the keys are on in the active column, or the item it points at;
    /// none on a saved search, which is no file.
    pub fn keys_on(&self) -> Option<&Path> {
        let c = self.columns.get(self.active)?;
        let listed = |p: &&PathBuf| c.entry(p).is_some();
        let path = c.focused.as_ref().filter(listed).or(c.highlight.as_ref().filter(listed))?;
        saved_query(path).is_none().then_some(path.as_path())
    }

    /// Ctrl+click: adds the row to the selection, or takes it out.
    pub fn toggle(&mut self, index: usize, path: &Path) {
        let Some(c) = self.columns.get(index) else { return };
        let mut selected = c.selected.clone();
        if let Some(at) = selected.iter().position(|p| p == path) {
            selected.remove(at);
        } else {
            selected.push(path.to_owned());
        }
        self.select(index, selected);
        let c = &mut self.columns[index];
        c.focused = Some(path.to_owned());
        c.anchor = Some(path.to_owned());
    }

    /// Shift+click: picks the rows from the anchor to `path`, in their order.
    pub fn extend(&mut self, index: usize, path: &Path) {
        let Some((anchor, range)) = self.columns.get(index).and_then(|c| c.range_to(path)) else { return };
        self.select(index, range);
        let c = &mut self.columns[index];
        c.focused = Some(path.to_owned());
        c.anchor = anchor.or_else(|| Some(path.to_owned()));
    }

    /// Ctrl+Shift+click: adds the rows from the anchor to `path` to what was picked before the first such
    /// range from the anchor, so a range drawn back lets go of the rows it passed.
    pub fn add_range(&mut self, index: usize, path: &Path) {
        let Some(c) = self.columns.get(index) else { return };
        let Some((anchor, range)) = c.range_to(path) else { return };
        let base = c.base.clone().unwrap_or_else(|| c.selected.clone());
        let kept: HashSet<&PathBuf> = base.iter().collect();
        let mut picked = base.clone();
        picked.extend(range.into_iter().filter(|p| !kept.contains(p)));
        self.select(index, picked);
        let c = &mut self.columns[index];
        c.focused = Some(path.to_owned());
        c.anchor = anchor.or_else(|| Some(path.to_owned()));
        c.base = Some(base);
    }

    /// Shift+↑↓: moves the keys `by` rows, picking the rows from the anchor to there; with Ctrl, adding
    /// them as `add_range` does.
    pub fn extend_step(&mut self, by: isize, add: bool) {
        let index = self.active;
        let c = &self.columns[index];
        if c.entries.is_empty() {
            return;
        }
        let from = c.focused.as_ref().and_then(|f| c.entries.iter().position(|e| &e.path == f));
        let to = match from {
            Some(from) => (from as isize + by).clamp(0, c.entries.len() as isize - 1) as usize,
            None => 0,
        };
        let path = c.entries[to].path.clone();
        if add {
            self.add_range(index, &path);
        } else {
            self.extend(index, &path);
        }
    }

    /// Ctrl+A: picks every row listed so far.
    pub fn select_all(&mut self, index: usize) {
        let Some(c) = self.columns.get(index) else { return };
        let focused = c.focused.clone();
        let all: Vec<PathBuf> = c.entries.iter().map(|e| e.path.clone()).collect();
        self.select(index, all);
        if focused.is_some() {
            self.columns[index].focused = focused;
        }
    }

    /// Shows an item where it lives, as from the history, leaving the search; the column pointing at
    /// it takes the keys.
    pub fn reveal(&mut self, path: &Path, directory: bool) {
        let entry = Entry {
            path: path.into(),
            name: shown(folder_name(path)),
            parent: parent_folder(path).map(shown).unwrap_or_default(),
            directory,
            size: None,
            modified: None,
            hidden: false,
            marks: Marks::default(),
        };
        let here = self.snapshot();
        let mut home = self.columns.swap_remove(0);
        home.selected.clear();
        home.focused = None;
        let mut columns = vec![home];
        for spec in reveal_chain(&entry) {
            let mut c = self.column(Kind::Folder, &spec.path, spec.highlight.clone());
            c.selected = spec.selected.unwrap_or_default();
            c.focused = spec.highlight.or_else(|| c.selected.last().cloned());
            columns.push(c);
        }
        let at = columns.iter().position(|c| c.highlight.as_deref() == Some(path));
        self.active = at.unwrap_or(columns.len() - 1);
        self.columns = columns;
        self.query.clear();
        self.search_filter = None;
        self.fold = None;
        self.remember(here);
    }

    /// Ahead of the listing that shows it, the renamed item keeps its selection and an open folder its
    /// columns, rather than reading as gone.
    pub fn renamed(&mut self, from: &Path, to: &Path) {
        let moved = |path: &Path| -> PathBuf { paths::moved(path, from, to).unwrap_or_else(|| path.to_owned()) };
        let name = shown(folder_name(to));
        for c in &mut self.columns {
            c.path = moved(&c.path);
            for e in c.entries.iter_mut().chain(c.filter.iter_mut().flat_map(|f| f.all.iter_mut())) {
                if e.path == from {
                    e.name = name.clone();
                }
                if let Some(path) = paths::moved(&e.path, from, to) {
                    e.parent = path.parent().map(shown).unwrap_or_default();
                    e.path = path;
                }
            }
            for p in c.selected.iter_mut().chain(c.base.iter_mut().flatten()) {
                *p = moved(p);
            }
            for p in [&mut c.focused, &mut c.highlight, &mut c.anchor].into_iter().flatten() {
                *p = moved(p);
            }
        }
        for location in self.past.iter_mut().chain(self.future.iter_mut()) {
            for (path, highlight) in &mut location.folders {
                *path = moved(path);
                if let Some(h) = highlight {
                    *h = moved(h);
                }
            }
            for selection in &mut location.selections {
                for p in selection {
                    *p = moved(p);
                }
            }
        }
        // Its marks follow the new name; one the filter would hide shows without it.
        for index in 0..self.columns.len() {
            if self.hides(index, to) {
                self.set_filter(index, None);
            } else {
                self.narrow(index);
            }
        }
    }

    /// Like Explorer, a new folder is picked once made: listed ahead of the listing that sorts it in.
    pub fn created(&mut self, parent: &Path, path: &Path) {
        let Some(index) = self.columns.iter().position(|c| c.kind == Kind::Folder && c.path == parent) else {
            return;
        };
        let c = &mut self.columns[index];
        let text = c.filter.take().map(|f| {
            c.entries = f.all;
            f.text
        });
        if c.entry(path).is_none() {
            c.entries.push(Entry { parent: shown(parent), ..Entry::folder(path, &shown(folder_name(path))) });
            c.total += 1;
        }
        // It shows, to be seen picked, even where the filter would hide it: the filter goes then.
        if let Some(text) = text.filter(|text| holds(&shown(folder_name(path)), text).is_some()) {
            c.filter = Some(Filter { text, all: std::mem::take(&mut c.entries), whole: 0 });
            self.narrow(index);
        }
        self.select(index, vec![path.to_owned()]);
    }

    /// Narrows the column at `index` to the names holding the words of `text`, or with none, or no words in
    /// it, lists them all again. What it hides stays picked, and what that opened open, while another is looked for. The
    /// search results are narrowed by Everything, asked again: they show as they are until it answers.
    pub fn set_filter(&mut self, index: usize, text: Option<&str>) {
        let Some(c) = self.columns.get_mut(index) else { return };
        let text = text.filter(|text| !filter_words(text).is_empty());
        if c.kind == Kind::Search {
            c.filter = match (text, c.filter.take()) {
                (None, _) => None,
                (Some(text), Some(filter)) => Some(Filter { text: text.into(), ..filter }),
                (Some(text), None) => Some(Filter { text: text.into(), all: Vec::new(), whole: c.total }),
            };
            return;
        }
        match text {
            None => {
                if let Some(filter) = c.filter.take() {
                    c.entries = filter.all;
                    // All show again: the keys go back to the row the filter took them off.
                    let hid = c.hid.take();
                    if c.focused.is_none() {
                        c.focused = hid;
                    }
                }
            }
            Some(text) => {
                let all = match c.filter.take() {
                    Some(filter) => filter.all,
                    None => std::mem::take(&mut c.entries),
                };
                c.filter = Some(Filter { text: text.into(), all, whole: 0 });
                self.narrow(index);
            }
        }
    }

    /// Lists what the column's filter lets through; the keys let go of a row it hides, kept to go back
    /// to once it shows again, while they have gone to no other.
    fn narrow(&mut self, index: usize) {
        let c = &mut self.columns[index];
        let Some(filter) = c.filter.as_ref().filter(|_| c.kind != Kind::Search) else { return };
        c.entries = filter.all.iter().filter_map(|e| narrowed(e, &filter.text)).collect();
        let shown: HashSet<&Path> = c.entries.iter().map(|e| e.path.as_path()).collect();
        match &c.focused {
            Some(f) if !shown.contains(f.as_path()) => c.hid = c.focused.take(),
            Some(_) => c.hid = None,
            None if c.hid.as_ref().is_some_and(|h| shown.contains(h.as_path())) => c.focused = c.hid.take(),
            None => {}
        }
    }

    /// Whether the folder column at `index` narrowed to its names, holds `path` but hides it.
    fn hides(&self, index: usize, path: &Path) -> bool {
        let c = &self.columns[index];
        c.filter.as_ref().is_some_and(|f| f.all.iter().any(|e| e.path == path && narrowed(e, &f.text).is_none()))
    }

    /// Picked in the folder they went to once it lists them, while it is the column worked in.
    pub fn pasted(&mut self, parent: &Path, paths: &[PathBuf]) {
        let Some(index) = self.columns.iter().position(|c| c.kind == Kind::Folder && c.path == parent) else {
            return;
        };
        if index != self.active {
            return;
        }
        // Listed already where the filter hides them, they show without it.
        if paths.iter().any(|p| self.hides(index, p)) {
            self.set_filter(index, None);
        }
        self.columns[index].pick = Some(paths.iter().map(|p| path_key(p)).collect());
        self.pick_listed(index);
    }

    /// Picks what the column waits to pick, once it lists any of it.
    fn pick_listed(&mut self, index: usize) -> bool {
        let c = &self.columns[index];
        let Some(keys) = &c.pick else { return false };
        let keys: HashSet<&OsString> = keys.iter().collect();
        let paths: Vec<PathBuf> = c.entries.iter().filter(|e| keys.contains(&path_key(&e.path))).map(|e| e.path.clone()).collect();
        if paths.is_empty() {
            return false;
        }
        self.select(index, paths);
        true
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
    /// listed; the first replaces it. `departed` holds the path keys of what Colet's own operations sent
    /// away, which is expected to go.
    pub fn page(
        &mut self,
        id: ColumnId,
        request: u64,
        entries: Vec<Entry>,
        total: usize,
        offset: usize,
        departed: &HashSet<OsString>,
    ) {
        let Some(index) = self.index_of(id) else { return };
        let c = &mut self.columns[index];
        if c.request != request {
            return;
        }
        // Everything narrowed the results already, leaving out what the filter hides; that stays picked, as
        // in the other columns, told by its name.
        if c.kind == Kind::Search {
            let hidden: Vec<PathBuf> = match &c.filter {
                Some(filter) => {
                    c.selected.iter().filter(|p| holds(&shown(folder_name(p)), &filter.text).is_none()).cloned().collect()
                }
                None => Vec::new(),
            };
            let keys = c.focused.clone().or_else(|| c.hid.take());
            self.list(index, entries, total, offset, departed);
            let c = &mut self.columns[index];
            let kept: HashSet<PathBuf> = c.selected.iter().cloned().collect();
            c.selected.extend(hidden.into_iter().filter(|p| !kept.contains(p)));
            // The keys let go of a result the filter hides, as in the other columns, and go back to it
            // once it shows again.
            if let Some(path) = keys {
                if c.entry(&path).is_some() {
                    c.focused = Some(path);
                } else if c.filter.as_ref().is_some_and(|f| holds(&shown(folder_name(&path)), &f.text).is_none()) {
                    c.focused = None;
                    c.hid = Some(path);
                }
            }
            return;
        }
        // The listing stands for all the column lists; the filter narrows it again after.
        let text = c.filter.take().map(|f| {
            c.entries = f.all;
            f.text
        });
        let waiting = c.pick.is_some();
        self.list(index, entries, total, offset, departed);
        let Some(text) = text else { return };
        let c = &mut self.columns[index];
        // What was just pasted here shows picked, as a new folder does: the filter goes when it hides any.
        let picked = waiting && c.pick.is_none() && index == self.active;
        if picked && c.selected.iter().any(|p| c.entry(p).is_some_and(|e| narrowed(e, &text).is_none())) {
            return;
        }
        c.filter = Some(Filter { text, all: std::mem::take(&mut c.entries), whole: 0 });
        self.narrow(index);
    }

    /// How many the search finds without the filter Everything narrows it by, asked along with `request`.
    pub fn counted(&mut self, id: ColumnId, request: u64, whole: usize) {
        let Some(index) = self.index_of(id) else { return };
        let c = &mut self.columns[index];
        if c.request != request {
            return;
        }
        if let Some(filter) = &mut c.filter {
            filter.whole = whole;
        }
    }

    /// What [`page`](Self::page) does with a listing that is the one the column waits for.
    fn list(&mut self, index: usize, entries: Vec<Entry>, total: usize, offset: usize, departed: &HashSet<OsString>) {
        let c = &mut self.columns[index];
        let old = std::mem::take(&mut c.entries);
        if offset > 0 {
            let known: HashSet<&PathBuf> = old.iter().map(|e| &e.path).collect();
            let fresh: Vec<Entry> = entries.into_iter().filter(|e| !known.contains(&e.path)).collect();
            c.entries = old.iter().cloned().chain(fresh).collect();
        } else {
            c.entries = entries;
        }
        let paths: HashSet<&Path> = c.entries.iter().map(|e| e.path.as_path()).collect();
        // The rows the keys were on that are gone; the row now in the topmost one's place takes over, or
        // at the end, the last before it.
        // Looked up rather than searched for: a refresh with thousands picked would compare each with each.
        let picked: HashSet<&Path> = c.selected.iter().map(PathBuf::as_path).collect();
        let lost: Vec<usize> = (0..old.len())
            .filter(|&i| {
                let path = &old[i].path;
                !paths.contains(path.as_path()) && (picked.contains(path.as_path()) || c.focused.as_ref() == Some(path))
            })
            .collect();
        drop(picked);
        let heir = lost.first().and_then(|&at| {
            old[at..]
                .iter()
                .find(|e| paths.contains(e.path.as_path()))
                .or_else(|| old[..at].iter().rev().find(|e| paths.contains(e.path.as_path())))
                .map(|e| e.path.clone())
        });
        let departed_path = |path: &Path| ancestors(path).iter().any(|a| departed.contains(&path_key(a)));
        let all_departed = !lost.is_empty() && lost.iter().all(|&i| departed_path(&old[i].path));
        c.selected.retain(|p| paths.contains(p.as_path()));
        let focused = match (&c.focused, &c.highlight) {
            (Some(f), _) if paths.contains(f.as_path()) => Some(f.clone()),
            (_, Some(h)) if paths.contains(h.as_path()) => Some(h.clone()),
            _ => None,
        };
        c.focused = focused;
        c.total = total;
        c.loading = false;
        c.error = None;
        let kind = c.kind;
        // The folder it had open beside it is gone: close it and what it opened. Said only of a folder
        // gone some other way: one Colet sent away is expected to go.
        let vanished = kind == Kind::Folder
            && self.columns.get(index + 1).is_some_and(|child| {
                !self.columns[index].entries.iter().any(|e| e.path == child.path && e.directory)
            });
        if vanished {
            let child = self.columns[index + 1].path.clone();
            self.columns.truncate(index + 1);
            self.active = self.active.min(index);
            if !departed_path(&child) {
                self.notice = Some("選択していたフォルダーがなくなったため、右側のカラムを閉じました。".into());
            }
        }
        // A column left while it waits to pick lets go of it.
        if self.columns[index].pick.is_some() {
            if index != self.active {
                self.columns[index].pick = None;
            } else if self.pick_listed(index) {
                return;
            }
        }
        // Like Explorer, what Colet itself sent away in the column worked in leaves its place picked, so
        // the next key acts on that. Gone some other way, no folder opens unasked.
        if index == self.active && all_departed {
            self.select(index, heir.into_iter().collect());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colet_core::paths::ancestors;

    fn entry(path: &str, directory: bool) -> Entry {
        let chain = ancestors(Path::new(path));
        Entry {
            path: path.into(),
            name: path.into(),
            parent: chain.len().checked_sub(2).map(|i| shown(&chain[i])).unwrap_or_default(),
            directory,
            size: None,
            modified: None,
            hidden: false,
            marks: Marks::default(),
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
        s.columns.iter().map(|c| c.path.to_str().unwrap()).collect()
    }

    /// Each folder column after home: its path, selection and highlight.
    fn chain(s: &State) -> Vec<(PathBuf, Vec<PathBuf>, Option<PathBuf>)> {
        s.columns[1..].iter().map(|c| (c.path.clone(), c.selected.clone(), c.highlight.clone())).collect()
    }

    fn link(path: &str, selected: &[&str], highlight: Option<&Path>) -> (PathBuf, Vec<PathBuf>, Option<PathBuf>) {
        (path.into(), selected.iter().map(PathBuf::from).collect(), highlight.map(Into::into))
    }

    fn all(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn one(path: impl AsRef<Path>) -> Vec<PathBuf> {
        vec![path.as_ref().to_owned()]
    }

    #[test]
    fn removes_vanished_selection_and_focus_after_a_refresh() {
        for kind in [Kind::Search, Kind::Home, Kind::Folder] {
            let mut s = State::new();
            let (kept, gone) = (file(r"D:\kept"), file(r"D:\gone"));
            let mut c = s.column(kind, Path::new(""), None);
            c.entries = vec![kept.clone(), gone.clone()];
            c.selected = vec![kept.path.clone(), gone.path.clone()];
            c.focused = Some(gone.path.clone());
            c.request = 2;
            let id = c.id;
            s.columns = vec![c];
            s.page(id, 2, vec![kept.clone()], 1, 0, &HashSet::new());
            assert_eq!(s.columns[0].selected, std::slice::from_ref(&kept.path));
            assert_eq!(s.columns[0].focused, None);
            s.page(id, 2, vec![], 0, 0, &HashSet::new());
            assert!(s.columns[0].selected.is_empty());
        }
    }

    #[test]
    fn a_refresh_with_thousands_picked_hands_what_colet_sent_away_to_the_next_row() {
        let entries: Vec<Entry> = (0..20_000).map(|i| file(&format!(r"D:\work\{i:05}"))).collect();
        let mut s = State::new();
        let mut c = s.column(Kind::Folder, Path::new(r"D:\work"), None);
        c.entries = entries.clone();
        c.selected = entries[..10_000].iter().map(|e| e.path.clone()).collect();
        c.request = 2;
        let id = c.id;
        s.columns = vec![c];
        // Pair by pair, these would be 200 million comparisons on the UI thread.
        let departed: HashSet<OsString> = entries[..10_000].iter().map(|e| path_key(&e.path)).collect();
        s.page(id, 2, entries[10_000..].to_vec(), 10_000, 0, &departed);
        assert_eq!(s.columns[0].selected, one(&entries[10_000].path));
    }

    #[test]
    fn keeps_selections_on_appended_pages_but_drops_unloaded_selections_on_refresh() {
        let entries: Vec<Entry> = (0..501).map(|i| file(&format!(r"D:\file{i}"))).collect();
        let (first, last) = (entries[0].path.clone(), entries[500].path.clone());
        let mut s = State::new();
        s.set_query("file");
        let id = s.columns[1].id;
        s.load(id, 2, false);
        s.page(id, 2, entries[..500].to_vec(), 501, 0, &HashSet::new());
        s.columns[1].selected = one(&first);
        s.columns[1].focused = Some(first.clone());
        s.page(id, 2, entries[500..].to_vec(), 501, 500, &HashSet::new());
        assert_eq!(s.columns[1].entries.len(), 501);
        assert_eq!(s.columns[1].selected, one(&first));
        assert_eq!(s.columns[1].focused.as_deref(), Some(first.as_path()));
        s.select(1, vec![first.clone(), last.clone()]);
        // A stale listing changes nothing.
        s.page(id, 1, entries[..500].to_vec(), 501, 0, &HashSet::new());
        assert_eq!(s.columns[1].selected, [first.clone(), last.clone()]);
        s.page(id, 2, entries[..500].to_vec(), 501, 0, &HashSet::new());
        assert_eq!(s.columns[1].selected, one(&first));
        assert_eq!(s.columns[1].focused, None);
    }

    #[test]
    fn discards_stale_and_removed_column_responses() {
        let mut s = State::new();
        let home = s.columns[0].id;
        s.load(home, 2, false);
        s.page(home, 1, vec![folder("old")], 1, 0, &HashSet::new());
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
        assert_eq!(s.columns[3].highlight.as_deref(), Some(target.path.as_path()));
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
            [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\b"], Some(Path::new(r"D:\a\b"))), link(r"D:\a\b", &[], None)]
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
        assert_eq!(chain(&s)[1..], [link(r"D:\", &[r"D:\a"], Some(Path::new(r"D:\a"))), link(r"D:\a", &[], None)]);
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
        assert_eq!(s.columns[4].path, Path::new(r"D:\a\c"));
        // Gone back to, a place is folded as it was left; going forward too.
        s.back();
        assert_eq!(s.columns[4].path, Path::new(r"D:\a\b"));
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
        assert_eq!(s.columns[0].focused.as_deref(), Some(Path::new(r"D:\b")));
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
        assert_eq!(s.columns[0].focused.as_deref(), Some(Path::new(r"D:\a")));
        assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\a"));
        s.forward();
        assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\b"));
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
        assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\a"));
    }

    #[test]
    fn closes_a_vanished_folder_and_what_it_opened() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"D:\a")]);
        s.select(0, one(r"D:\a"));
        let parent = s.columns[1].id;
        s.load(parent, 4, false);
        s.page(parent, 4, vec![folder(r"D:\a\b")], 1, 0, &HashSet::new());
        s.select(1, one(r"D:\a\b"));
        s.set_active(2);
        s.page(parent, 4, vec![], 0, 0, &HashSet::new());
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
        assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\b"));
    }

    #[test]
    fn enters_home_without_picking_and_results_picking_the_first() {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"C:\"), folder(r"D:\")]);
        s.enter();
        assert_eq!(s.columns[0].focused.as_deref(), Some(Path::new(r"C:\")));
        assert!(s.columns[0].selected.is_empty());
        s.set_query("x");
        with_entries(&mut s, 1, vec![file(r"D:\x.txt")]);
        s.enter();
        assert_eq!(s.columns[1].selected, one(r"D:\x.txt"));
    }

    #[test]
    fn runs_a_saved_search_picked_in_home_keeping_homes_pick() {
        let mut s = State::new();
        let pdf = search_key("ext:pdf", None);
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

    /// D:\ listed in its column, beside home.
    fn drive(entries: Vec<Entry>) -> State {
        let mut s = State::new();
        with_entries(&mut s, 0, vec![folder(r"D:\")]);
        s.select(0, one(r"D:\"));
        with_entries(&mut s, 1, entries);
        s.set_active(1);
        s
    }

    #[test]
    fn picks_ranges_and_single_rows_by_ctrl_and_shift() {
        let mut s = drive(vec![file(r"D:\a"), file(r"D:\b"), file(r"D:\c"), file(r"D:\d")]);
        s.select(1, one(r"D:\b"));
        s.extend(1, Path::new(r"D:\d"));
        assert_eq!(s.columns[1].selected, all(&[r"D:\b", r"D:\c", r"D:\d"]));
        s.extend(1, Path::new(r"D:\a"));
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\b"]));
        s.toggle(1, Path::new(r"D:\d"));
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\b", r"D:\d"]));
        s.toggle(1, Path::new(r"D:\a"));
        assert_eq!(s.columns[1].selected, all(&[r"D:\b", r"D:\d"]));
        s.extend_step(-1, false);
        assert_eq!(s.columns[1].selected, all(&[r"D:\a"]));
        s.select_all(1);
        assert_eq!(s.columns[1].selected.len(), 4);
    }

    #[test]
    fn picks_rows_apart_from_the_keys_alone() {
        let entries = ["a", "b", "c", "d", "e", "f"].map(|n| file(&format!(r"D:\{n}")));
        let mut s = drive(entries.to_vec());
        s.select(1, one(r"D:\a"));
        // Ctrl+↓ moves the keys and nothing else; Ctrl+Space picks where they are.
        s.focus_step(2);
        assert_eq!(s.columns[1].selected, one(r"D:\a"));
        assert_eq!(s.columns[1].focused.as_deref(), Some(Path::new(r"D:\c")));
        s.toggle_focused();
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c"]));
        s.focus_step(isize::MAX / 2);
        assert_eq!(s.columns[1].focused.as_deref(), Some(Path::new(r"D:\f")));
        s.focus_step(-1);
        s.toggle_focused();
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\e"]));
        // Again, it lets go.
        s.toggle_focused();
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c"]));
    }

    #[test]
    fn adds_a_range_drawn_back_from_the_anchor_to_what_was_picked() {
        let entries = ["a", "b", "c", "d", "e", "f"].map(|n| file(&format!(r"D:\{n}")));
        let mut s = drive(entries.to_vec());
        s.select(1, one(r"D:\a"));
        s.focus_step(2);
        s.toggle_focused();
        // From c, Ctrl+Shift+↓ twice adds d and e beside a.
        s.extend_step(1, true);
        s.extend_step(1, true);
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\d", r"D:\e"]));
        // Drawn back, e goes; a and c, picked before, stay.
        s.extend_step(-1, true);
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\d"]));
        // Moving the keys alone keeps the anchor, and the next range starts from c again.
        s.focus_step(2);
        s.extend_step(1, true);
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\d", r"D:\e", r"D:\f"]));
        s.extend_step(-3, true);
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c"]));
        // Upward from c takes b in, and a, already picked, once only.
        s.extend_step(-1, true);
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\b"]));
        s.add_range(1, Path::new(r"D:\a"));
        assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\b"]));
        // Shift alone puts the range in the selection's place.
        s.extend_step(1, false);
        assert_eq!(s.columns[1].selected, all(&[r"D:\b", r"D:\c"]));
    }

    #[test]
    fn carries_a_renamed_folders_selection_and_open_columns_to_its_new_name() {
        let mut s = drive(vec![folder(r"D:\work"), file(r"D:\notes.txt")]);
        s.select(1, one(r"D:\work"));
        with_entries(&mut s, 2, vec![file(r"D:\work\a.txt")]);
        s.select(2, one(r"D:\work\a.txt"));
        s.set_active(1);
        s.renamed(Path::new(r"D:\work"), Path::new(r"D:\projects"));
        assert_eq!(s.columns[1].selected, one(r"D:\projects"));
        assert_eq!(s.columns[1].entries[0].name, "projects");
        assert_eq!(s.columns[2].path, Path::new(r"D:\projects"));
        assert_eq!(s.columns[2].selected, one(r"D:\projects\a.txt"));
        assert_eq!(s.columns[2].entries[0].parent, r"D:\projects");
        assert_eq!(s.past.last().unwrap().folders.iter().map(|f| f.0.to_str().unwrap()).collect::<Vec<_>>(), [r"D:\", r"D:\projects"]);
        // The listing finds it under its new name, so nothing closes.
        let parent = s.columns[1].id;
        s.load(parent, 3, true);
        s.page(parent, 3, vec![folder(r"D:\projects"), file(r"D:\notes.txt")], 2, 0, &HashSet::new());
        assert_eq!(s.columns.len(), 3);
        assert!(s.notice.is_none());
    }

    #[test]
    fn picks_a_new_folder_and_opens_it() {
        let mut s = drive(vec![file(r"D:\notes.txt")]);
        s.created(Path::new(r"D:\"), Path::new(r"D:\新しいフォルダー (2)"));
        let made = s.columns[1].entries.last().unwrap();
        assert_eq!((made.name.as_str(), made.directory), ("新しいフォルダー (2)", true));
        assert_eq!(s.columns[1].selected, one(r"D:\新しいフォルダー (2)"));
        assert_eq!(s.columns[2].path, Path::new(r"D:\新しいフォルダー (2)"));
        assert_eq!(s.active, 1);
    }

    #[test]
    fn picks_what_was_pasted_once_listed() {
        let mut s = drive(vec![file(r"D:\a")]);
        let id = s.columns[1].id;
        s.pasted(Path::new(r"D:\"), &[r"D:\a - コピー".into(), r"D:\b".into()]);
        assert!(s.columns[1].selected.is_empty());
        s.load(id, 5, true);
        s.page(id, 5, vec![file(r"D:\a"), file(r"D:\a - コピー"), file(r"D:\b")], 3, 0, &HashSet::new());
        assert_eq!(s.columns[1].selected, all(&[r"D:\a - コピー", r"D:\b"]));
        assert!(s.columns[1].pick.is_none());
    }

    #[test]
    fn picks_the_item_that_takes_a_deleted_ones_place_saying_nothing() {
        let mut s = drive(vec![folder(r"D:\a"), folder(r"D:\b"), file(r"D:\c")]);
        s.select(1, one(r"D:\b"));
        s.set_active(1);
        let id = s.columns[1].id;
        s.load(id, 6, true);
        let departed = HashSet::from([path_key(Path::new(r"D:\b"))]);
        s.page(id, 6, vec![folder(r"D:\a"), file(r"D:\c")], 2, 0, &departed);
        assert_eq!(s.columns[1].selected, one(r"D:\c"));
        assert!(s.notice.is_none());
        // Gone some other way, its column closes with a word, and nothing is picked unasked.
        let mut s = drive(vec![folder(r"D:\a"), folder(r"D:\b")]);
        s.select(1, one(r"D:\b"));
        s.set_active(1);
        let id = s.columns[1].id;
        s.load(id, 7, true);
        s.page(id, 7, vec![folder(r"D:\a")], 1, 0, &HashSet::new());
        assert!(s.columns[1].selected.is_empty());
        assert_eq!(s.columns.len(), 2);
        assert!(s.notice.is_some());
    }

    #[test]
    fn reveals_an_item_where_it_lives_leaving_the_search() {
        let mut s = State::new();
        s.set_query("x");
        s.reveal(Path::new(r"D:\a\b.txt"), false);
        assert_eq!(s.query, "");
        assert_eq!(chain(&s), [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[], Some(Path::new(r"D:\a\b.txt")))]);
        assert_eq!(s.active, 2);
        s.back();
        assert_eq!(s.query, "x");
    }

    fn names(s: &State, index: usize) -> Vec<&str> {
        s.columns[index].entries.iter().map(|e| e.path.to_str().unwrap()).collect()
    }

    #[test]
    fn finds_every_place_a_name_holds_the_text_case_aside() {
        assert_eq!(found("ReadMe readme.TXT", "README"), [0..6, 7..13]);
        assert_eq!(found("資料メモ.txt", "メモ"), [6..12]);
        // A letter lowering into two is matched whole.
        assert_eq!(found("\u{130}x", "\u{69}\u{307}x"), [0..3]);
        assert!(found("abc", "").is_empty());
        assert!(found("abc", "abcd").is_empty());
    }

    #[test]
    fn takes_the_words_apart_at_any_space_but_in_quotes() {
        assert_eq!(filter_words(" rep\u{3000}2024  x"), ["rep", "2024", "x"]);
        assert_eq!(filter_words("\"my doc\" a\"b c\"d"), ["my doc", "ab cd"]);
        // A quote left open is closed at the end.
        assert_eq!(filter_words("x \"my do"), ["x", "my do"]);
        assert!(filter_words(" \u{3000}\"\" ").is_empty());
        // Nothing else is read: a minus or a bar is a letter as any.
        assert_eq!(filter_words("-a|b"), ["-a|b"]);
    }

    #[test]
    fn holds_a_name_with_all_the_words_in_any_order_marked_as_one_where_they_meet() {
        assert_eq!(holds("2024_Report.txt", "rep 2024"), Some(vec![0..4, 5..8]));
        assert_eq!(holds("abcd", "ab bc"), Some(vec![0..3]));
        assert_eq!(holds("abcd", "ab cd"), Some(vec![0..4]));
        assert_eq!(holds("my doc.txt", "\"my doc\""), Some(vec![0..6]));
        assert_eq!(holds("2024_Report.txt", "rep 2025"), None);
        assert_eq!(holds("abc", "  "), Some(vec![]));
    }

    #[test]
    fn narrows_a_folder_to_the_names_holding_the_text_keeping_what_it_hides_picked() {
        let mut s = drive(vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), folder(r"D:\gamma")]);
        s.select(1, one(r"D:\alpha"));
        assert_eq!(s.columns.len(), 3);
        s.set_filter(1, Some("TA"));
        assert_eq!(names(&s, 1), [r"D:\beta.txt"]);
        assert_eq!(s.columns[1].entries[0].marks.name, [5..7]);
        // The folder it had open beside it is hidden: it stays picked, and its column open, but is not
        // what is acted on.
        assert_eq!(s.columns[1].selected, one(r"D:\alpha"));
        assert_eq!(s.columns.len(), 3);
        assert!(s.columns[1].shown_selected().is_empty());
        // Of several picked, what shows is acted on.
        s.select(1, vec![PathBuf::from(r"D:\alpha"), PathBuf::from(r"D:\beta.txt")]);
        s.set_filter(1, Some("t"));
        assert_eq!(s.columns[1].selected.len(), 2);
        assert_eq!(*s.columns[1].shown_selected(), one(r"D:\beta.txt"));
        // Ctrl+A picks only what shows.
        s.set_filter(1, Some("a"));
        s.select_all(1);
        assert_eq!(s.columns[1].selected.len(), 3);
        s.set_filter(1, Some("mm"));
        s.select_all(1);
        assert_eq!(s.columns[1].selected, one(r"D:\gamma"));
        // Lifted, it lists all again, unmarked.
        s.set_filter(1, None);
        assert_eq!(names(&s, 1).len(), 3);
        assert!(s.columns[1].entries.iter().all(|e| e.marks.name.is_empty()));
        assert!(s.columns[1].filter.is_none());
    }

    #[test]
    fn narrows_home_keeping_its_pick_and_what_it_opened() {
        let mut s = drive(vec![folder(r"D:\alpha")]);
        with_entries(&mut s, 0, vec![folder(r"C:\"), folder(r"D:\")]);
        s.select(0, one(r"D:\"));
        s.select(1, one(r"D:\alpha"));
        assert_eq!(s.columns.len(), 3);
        s.set_filter(0, Some("c:"));
        assert_eq!(names(&s, 0), [r"C:\"]);
        assert_eq!(s.columns[0].selected, one(r"D:\"));
        assert_eq!(s.columns.len(), 3);
        // A listing that comes after is narrowed too.
        let id = s.columns[0].id;
        s.load(id, 9, true);
        s.page(id, 9, vec![folder(r"C:\"), folder(r"D:\"), folder(r"E:\")], 3, 0, &HashSet::new());
        assert_eq!(names(&s, 0), [r"C:\"]);
        s.set_filter(0, None);
        assert_eq!(names(&s, 0).len(), 3);
    }

    #[test]
    fn narrows_each_listing_that_comes_after() {
        let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
        let id = s.columns[1].id;
        s.set_filter(1, Some("ta"));
        s.load(id, 3, true);
        s.page(id, 3, vec![file(r"D:\beta.txt"), file(r"D:\delta"), file(r"D:\gamma")], 3, 0, &HashSet::new());
        assert_eq!(names(&s, 1), [r"D:\beta.txt", r"D:\delta"]);
        assert_eq!(s.columns[1].total, 3);
        // Emptied, it narrows no more.
        s.set_filter(1, Some(""));
        assert_eq!(names(&s, 1).len(), 3);
        assert!(s.columns[1].filter.is_none());
    }

    #[test]
    fn narrows_the_results_as_everything_answers_keeping_what_it_hides_picked() {
        let results = vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), file(r"E:\alphabet.txt")];
        let mut s = State::new();
        s.set_query("a");
        let id = s.columns[1].id;
        s.load(id, 1, false);
        s.page(id, 1, results.clone(), 3, 0, &HashSet::new());
        s.select(1, one(r"D:\alpha"));
        let open = s.columns.len();
        assert!(open > 2);
        // The results show as they are until Everything answers, counted from all the search finds.
        s.set_filter(1, Some("bet"));
        assert_eq!(names(&s, 1).len(), 3);
        assert_eq!(s.columns[1].filter.as_ref().map(|f| f.whole), Some(3));
        s.load(id, 2, true);
        s.counted(id, 2, 4);
        s.page(id, 2, results[1..].to_vec(), 2, 0, &HashSet::new());
        assert_eq!(names(&s, 1), [r"D:\beta.txt", r"E:\alphabet.txt"]);
        assert_eq!((s.columns[1].total, s.columns[1].filter.as_ref().map(|f| f.whole)), (2, Some(4)));
        // The one picked, hidden, stays picked, the folder it opened open.
        assert_eq!(s.columns[1].selected, one(r"D:\alpha"));
        assert_eq!(s.columns.len(), open);
        // Of several picked, those hidden stay picked too, but only what shows is acted on; a rename, which
        // narrows the others again, leaves the results be.
        s.select(1, vec![PathBuf::from(r"D:\beta.txt"), PathBuf::from(r"E:\alphabet.txt")]);
        s.set_filter(1, Some("beta."));
        s.load(id, 3, true);
        s.page(id, 3, results[1..2].to_vec(), 1, 0, &HashSet::new());
        assert_eq!(s.columns[1].selected, [PathBuf::from(r"D:\beta.txt"), PathBuf::from(r"E:\alphabet.txt")]);
        assert_eq!(*s.columns[1].shown_selected(), one(r"D:\beta.txt"));
        s.renamed(Path::new(r"D:\gamma"), Path::new(r"D:\delta"));
        assert_eq!(names(&s, 1), [r"D:\beta.txt"]);
        // Emptied, it narrows no more; another search starts with none.
        s.set_filter(1, Some(""));
        assert!(s.columns[1].filter.is_none());
        s.set_filter(1, Some("x"));
        s.set_query("b");
        assert!(s.columns[1].filter.is_none());
    }

    #[test]
    fn keeps_the_result_a_filter_takes_the_keys_off_to_go_back_to_once_it_shows() {
        let results = vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), file(r"E:\alphabet.txt")];
        let mut s = State::new();
        s.set_query("a");
        let id = s.columns[1].id;
        s.load(id, 1, false);
        s.page(id, 1, results.clone(), 3, 0, &HashSet::new());
        s.select(1, one(r"D:\alpha"));
        let mut answer = |s: &mut State, request: u64, entries: &[Entry]| {
            s.load(id, request, true);
            s.page(id, request, entries.to_vec(), entries.len(), 0, &HashSet::new());
        };
        s.set_filter(1, Some("bet"));
        answer(&mut s, 2, &results[1..]);
        assert_eq!(s.columns[1].focused, None);
        // Shown again, loosened or lifted, the keys are back on it.
        s.set_filter(1, Some("al"));
        answer(&mut s, 3, &[results[0].clone(), results[2].clone()]);
        assert_eq!(s.columns[1].focused, Some(PathBuf::from(r"D:\alpha")));
        s.set_filter(1, Some("bet"));
        answer(&mut s, 4, &results[1..]);
        s.set_filter(1, None);
        answer(&mut s, 5, &results);
        assert_eq!(s.columns[1].focused, Some(PathBuf::from(r"D:\alpha")));
        // Gone to another meanwhile, they stay there.
        s.set_filter(1, Some("bet"));
        answer(&mut s, 6, &results[1..]);
        s.select(1, one(r"D:\beta.txt"));
        s.set_filter(1, None);
        answer(&mut s, 7, &results);
        assert_eq!(s.columns[1].focused, Some(PathBuf::from(r"D:\beta.txt")));
    }

    #[test]
    fn lifts_the_filter_for_what_is_made_or_renamed_here_that_it_would_hide() {
        let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
        s.set_filter(1, Some("ta"));
        s.created(Path::new(r"D:\"), Path::new(r"D:\data"));
        assert_eq!(names(&s, 1), [r"D:\beta.txt", r"D:\data"]);
        assert_eq!(s.columns[1].selected, one(r"D:\data"));
        s.created(Path::new(r"D:\"), Path::new(r"D:\new"));
        assert!(s.columns[1].filter.is_none());
        assert_eq!(s.columns[1].selected, one(r"D:\new"));

        let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
        s.set_filter(1, Some("ta"));
        s.renamed(Path::new(r"D:\beta.txt"), Path::new(r"D:\beta2.txt"));
        assert_eq!(s.columns[1].entries[0].marks.name, [2..4]);
        s.renamed(Path::new(r"D:\beta2.txt"), Path::new(r"D:\b.txt"));
        assert!(s.columns[1].filter.is_none());

        // Pasted where it hides them, already listed or listed after.
        let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
        s.set_filter(1, Some("ta"));
        s.pasted(Path::new(r"D:\"), &[r"D:\gamma".into()]);
        assert!(s.columns[1].filter.is_none());
        assert_eq!(s.columns[1].selected, one(r"D:\gamma"));
        let mut s = drive(vec![file(r"D:\beta.txt")]);
        s.set_filter(1, Some("ta"));
        s.pasted(Path::new(r"D:\"), &[r"D:\x".into()]);
        let id = s.columns[1].id;
        s.load(id, 4, true);
        s.page(id, 4, vec![file(r"D:\beta.txt"), file(r"D:\x")], 2, 0, &HashSet::new());
        assert!(s.columns[1].filter.is_none());
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

    fn kinds(s: &State) -> Vec<Kind> {
        s.columns.iter().map(|c| c.kind).collect()
    }

    #[test]
    fn searches_with_a_filter_alone_and_steps_back_to_the_filter_before() {
        let mut s = State::new();
        s.set_search("", Some("画像".into()));
        assert_eq!(kinds(&s), [Kind::Home, Kind::Search]);
        assert_eq!(s.active, 1);
        // Typing keeps the filter, and is no step of its own.
        s.set_query("cat");
        assert_eq!(s.search_filter.as_deref(), Some("画像"));
        assert!(!s.can_go_back());
        s.set_search("cat", Some("動画".into()));
        s.set_search("cat", None);
        s.back();
        assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", Some("動画")));
        s.back();
        assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", Some("画像")));
        s.forward();
        assert_eq!(s.search_filter.as_deref(), Some("動画"));
        // Nothing typed and no filter, the results go.
        s.set_search("", None);
        assert_eq!(kinds(&s), [Kind::Home]);
        s.back();
        assert_eq!(kinds(&s), [Kind::Home, Kind::Search]);
    }

    #[test]
    fn runs_a_saved_search_within_its_filter() {
        let mut s = State::new();
        let images = search_key("cat", Some("画像"));
        let plain = search_key("cat", None);
        assert_ne!(images, plain);
        assert_eq!(saved_query(&images), Some(("cat", Some("画像"))));
        assert_eq!(saved_query(&plain), Some(("cat", None)));
        with_entries(&mut s, 0, vec![Entry::folder(&images, "cat"), Entry::folder(&plain, "cat"), folder(r"D:\")]);
        s.select(0, one(&images));
        assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", Some("画像")));
        s.select(0, one(&plain));
        assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", None));
        // A place picked leaves the search and its filter.
        s.select(0, one(&images));
        s.select(0, one(r"D:\"));
        assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("", None));
        s.back();
        assert_eq!(s.search_filter.as_deref(), Some("画像"));
    }

    #[test]
    fn follows_a_filter_renamed_or_gone_back_and_forth() {
        let mut s = State::new();
        s.set_search("", Some("画像".into()));
        s.set_search("cat", Some("動画".into()));
        s.rename_search_filter("画像", Some("写真"));
        s.rename_search_filter("動画", None);
        // Gone from the results, they show without it.
        assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", None));
        assert_eq!(kinds(&s), [Kind::Home, Kind::Search]);
        s.back();
        assert_eq!(s.search_filter.as_deref(), Some("写真"));
        // With nothing typed, the results go with it.
        s.rename_search_filter("写真", None);
        assert_eq!(kinds(&s), [Kind::Home]);
        assert_eq!(s.active, 0);
    }

    #[test]
    fn a_filter_gone_takes_its_results_pick_from_the_way_back() {
        let mut s = State::new();
        s.set_search("", Some("画像".into()));
        with_entries(&mut s, 1, vec![folder(r"D:\a")]);
        s.select(1, one(r"D:\a"));
        s.set_search("cat", None);
        s.rename_search_filter("画像", None);
        s.back();
        assert_eq!(kinds(&s), [Kind::Home, Kind::Folder, Kind::Folder]);
        // Each folder takes back its own pick, not the one of the column before it: the result's.
        assert_eq!(s.columns[1].selected, one(r"D:\a"));
        assert!(s.columns[2].selected.is_empty());
        assert_eq!(s.active, 0);
    }
}
