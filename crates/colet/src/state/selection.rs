//! What is picked and where the keys are: the column worked in, picking rows by the keys or the mouse,
//! and what picking a row opens beside it.
use super::*;

impl State {
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
        self.note_folder();
    }

    pub fn select(&mut self, index: usize, paths: Vec<PathBuf>) {
        // A folder picked in home opens narrowed as its favorite keeps it, or not narrowed at all.
        let opened = match (self.columns.get(index), paths.as_slice()) {
            (Some(c), [key]) if c.kind == Kind::Home && c.entry(key).is_some_and(|e| e.directory) => match HomeKey::of(key) {
                HomeKey::Folder { path, narrow } => Some((path.into_owned(), narrow.map(Cow::into_owned))),
                HomeKey::Search { .. } => None,
            },
            _ => None,
        };
        self.select_rows(index, paths);
        if let Some((path, narrow)) = opened
            && let Some(at) = self.columns.iter().rposition(|c| c.kind == Kind::Folder && c.path == path)
        {
            self.set_narrowing(at, narrow.as_deref());
        }
    }

    pub(super) fn select_rows(&mut self, index: usize, paths: Vec<PathBuf>) {
        let Some(c) = self.columns.get(index) else { return };
        let here = self.snapshot();
        if c.kind == Kind::Home
            && let [path] = paths.as_slice()
            && HomeKey::of(path).is_search()
        {
            let path = path.clone();
            self.run_saved_search(index, path, here);
        } else if c.kind == Kind::Home && self.pinned() && paths.len() == 1 && c.entries.iter().any(|e| e.path == paths[0]) {
            self.leave_search_for(paths, here);
        } else {
            self.pick_rows(index, paths, here);
        }
    }

    /// Picks `paths` in the column at `index`, opening to its right what a single pick shows.
    pub(super) fn pick_rows(&mut self, index: usize, paths: Vec<PathBuf>, here: Location) {
        let c = &self.columns[index];
        let kind = c.kind;
        // Like Finder, only a single selection opens a column to the right.
        let entry = (paths.len() == 1).then(|| c.entry(&paths[0]).cloned()).flatten();
        // A favorite narrowed is keyed apart from its folder, which is what opens.
        let entry = entry.map(|e| Entry { path: HomeKey::on_disk(&e.path).into_owned(), ..e });
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
        let (next, unchanged) = self.chain_after(index, specs);
        // A folder reopened from a folder column keeps what was open below it; a search result or
        // favorite always shows exactly its own chain.
        if unchanged && (kind == Kind::Folder || self.columns.len() == index + 1 + next.len()) {
            self.active = index;
            self.note_folder();
            return;
        }
        // Another pick in the results or home shows something else, even in the folders it keeps.
        if kind != Kind::Folder {
            self.last_folder = None;
        }
        let anchor = next.last().map(|c| c.id);
        self.columns.truncate(index + 1);
        self.columns.extend(next);
        self.active = index;
        self.note_folder();
        // Only a pick that opens something is a place to come back to.
        if entry.is_some() {
            // Picked afresh from the results or home, its own column stands alone.
            if kind != Kind::Folder {
                self.fold = anchor.map(|anchor| Fold { anchor, open: false });
            }
            self.remember(here);
        }
    }

    /// The column ← or → leads to from `index`, if any. From search results or home, → goes back to the
    /// folder column worked in last while it stays; before one, it lands where the pick is shown rather
    /// than on the drive at the top of its ancestors, wherever the keys were moved to apart from it: a
    /// folder's own contents, folded above or not, a file's folder. Home beside the results leads to
    /// them. Folded-away folders are passed over, ← going back to the results or home, → on to the
    /// column they fold above.
    pub fn neighbour(&self, index: usize, right: bool) -> Option<usize> {
        let mut next = if right { index + 1 } else { index.checked_sub(1)? };
        let c = &self.columns[index];
        let beside_folders = self.columns.get(index + 1).is_some_and(|c| c.kind == Kind::Folder);
        let last = self.last_folder.and_then(|id| self.index_of(id)).filter(|&at| at > index);
        if right
            && c.kind != Kind::Folder
            && beside_folders
            && let Some(at) = last
        {
            next = at;
        } else if right
            && c.kind != Kind::Folder
            && let [pick] = c.selected.as_slice()
        {
            // A favorite narrowed is keyed apart from its folder, which is what is shown.
            let pick = HomeKey::on_disk(pick);
            let later = || self.columns.iter().enumerate().skip(index + 1);
            let shown = later()
                .find(|(_, c)| c.kind == Kind::Folder && c.path == *pick)
                .or_else(|| later().find(|(_, c)| c.highlight.as_deref() == Some(&*pick) || c.selected.iter().any(|p| *p == *pick)));
            if let Some((at, _)) = shown {
                next = at;
            }
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
        self.arrive(true);
    }

    /// Esc back from a field: the keys arrive in the active column as [`Self::enter`] has them, but with
    /// neither a focused row nor an item pointed at they land on no row, picking nothing; ↓ picks the first.
    pub fn come_back(&mut self) {
        self.arrive(false);
    }

    fn arrive(&mut self, to_first: bool) {
        let index = self.active;
        let c = &self.columns[index];
        let listed = |path: &PathBuf| c.entry(path).is_some();
        if let Some(target) = c.focused.clone().filter(listed).or_else(|| c.highlight.clone().filter(listed)) {
            if c.selected.is_empty() && c.highlight.as_ref() == Some(&target) {
                self.select(index, vec![target]);
            }
        } else if !to_first {
            // Back from a field, the keys land on no row.
        } else if let Some(first) = c.entries.first().map(|e| e.path.clone()) {
            if c.kind == Kind::Home {
                self.columns[index].focused = Some(first);
            } else {
                self.select(index, vec![first]);
            }
        } else if c.loading && c.kind == Kind::Search {
            // Only the results: a folder gone to, as from the address bar, opens with nothing picked.
            self.columns[index].entering = true;
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
    /// none on a saved search, which is no file. A favorite narrowed is its folder.
    pub fn keys_on(&self) -> Option<Cow<'_, Path>> {
        let c = self.columns.get(self.active)?;
        let listed = |p: &&PathBuf| c.entry(p).is_some();
        let path = c.focused.as_ref().filter(listed).or(c.highlight.as_ref().filter(listed))?;
        match HomeKey::of(path) {
            HomeKey::Folder { path, .. } => Some(path),
            HomeKey::Search { .. } => None,
        }
    }

    /// Ctrl+click: adds the row to the selection, or takes it out; in home, which picks one row at a time,
    /// it is picked alone.
    pub fn toggle(&mut self, index: usize, path: &Path) {
        let Some(c) = self.columns.get(index) else { return };
        let mut selected = c.selected.clone();
        if let Some(at) = selected.iter().position(|p| p == path) {
            selected.remove(at);
        } else if c.kind == Kind::Home {
            selected = vec![path.to_owned()];
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

    /// The checkbox heading a column: with every row it shows picked, picks none, the keys staying on
    /// their row; with any left out, picks them all, as Ctrl+A does.
    pub fn toggle_all(&mut self, index: usize) {
        let Some(c) = self.columns.get(index) else { return };
        if !c.all_selected() {
            self.select_all(index);
            return;
        }
        let focused = c.focused.clone();
        self.select(index, Vec::new());
        self.columns[index].focused = focused;
    }
}
