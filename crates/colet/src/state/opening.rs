//! Opening a place: the columns along a path, the folders folded above a favorite or a result, and
//! bringing an item to light in its folder.
use super::*;

/// An item known only by its path and whether it is a folder, as one typed or kept in the history.
pub(super) fn bare_entry(path: &Path, directory: bool) -> Entry {
    Entry {
        path: path.into(),
        name: shown(folder_name(path)),
        parent: parent_folder(path).map(|p| shown(p).into()).unwrap_or_default(),
        directory,
        size: None,
        modified: None,
        hidden: Hiding::Shown,
        marks: Marks::default(),
    }
}

/// A folder column to open to the right of a selection.
pub(super) struct Spec {
    pub(super) path: PathBuf,
    pub(super) selected: Option<Vec<PathBuf>>,
    pub(super) highlight: Option<PathBuf>,
}

/// The columns that show a search result or favorite where it lives: each ancestor with the next level
/// selected, ending in the folder that points at a file or the contents of a folder. A folder is pointed
/// at in its parent, as a file is, as well as selected there.
pub(super) fn reveal_chain(entry: &Entry) -> Vec<Spec> {
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
                Spec { path: path.clone(), selected: Some(Vec::new()), highlight: (!entry.directory).then(|| entry.path.clone()) }
            }
        })
        .collect()
}

impl State {
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

    /// A tab of its own for an item: where it lives, with the folders above it folded away, as a search
    /// result opens; the column pointing at a file, or a folder's own, has the keys. There is no way back
    /// from it.
    pub fn opening(path: &Path, directory: bool) -> Self {
        let mut state = Self::new();
        state.reveal(path, directory);
        state.past.clear();
        state.fold_to_last();
        state
    }

    /// A tab of its own for a row of `home`, as picking it there opens it: a place, a favorite narrowed as
    /// it keeps, or a saved search run. There is no way back from it.
    pub fn opening_home(home: &Column, key: &Path) -> Self {
        let mut state = Self::new();
        let id = state.columns[0].id;
        state.columns[0] =
            Column { id, selected: Vec::new(), focused: None, hid: None, anchor: None, base: None, pick: None, ..home.clone() };
        state.select(0, vec![key.to_owned()]);
        state.past.clear();
        state
    }

    pub(super) fn note_folder(&mut self) {
        if let Some(c) = self.columns.get(self.active)
            && c.kind == Kind::Folder
        {
            self.last_folder = Some(c.id);
        }
    }

    /// Brings out the folders above a search result or favorite, for good.
    pub fn unfold(&mut self) {
        if let Some(fold) = &mut self.fold {
            fold.open = true;
        }
    }

    /// The columns `specs` asks for after the one at `index`, and whether those already there show just
    /// that. Columns already showing the same folder are kept, so stepping through results under a shared
    /// ancestor does not reload it.
    pub(super) fn chain_after(&mut self, index: usize, specs: Vec<Spec>) -> (Vec<Column>, bool) {
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
        (next, unchanged)
    }

    /// Shows an item where it lives, as from the history, leaving the search; the column pointing at
    /// it takes the keys.
    pub fn reveal(&mut self, path: &Path, directory: bool) {
        let entry = bare_entry(path, directory);
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
        self.note_folder();
        self.query.clear();
        self.search_filter = None;
        self.fold = None;
        self.remember(here);
    }

    /// Goes to a path typed in the address or opened from the history. A folder open in a column is gone
    /// to as its crumb goes, the columns below kept; what lies under the deepest folder open is opened on
    /// from it, all else open and the search kept; anything else is revealed afresh, the folders above it
    /// folded away as a search result's are.
    pub fn go_to_path(&mut self, path: &Path, directory: bool) {
        let key = path_key(path);
        let folders: Vec<(usize, OsString)> =
            self.columns.iter().enumerate().filter(|(_, c)| c.kind == Kind::Folder).map(|(index, c)| (index, path_key(&c.path))).collect();
        if directory && let Some(&(index, _)) = folders.iter().find(|(_, folder)| *folder == key) {
            self.set_active(index);
            return;
        }
        let deepest = folders.last().cloned();
        let specs = reveal_chain(&bare_entry(path, directory));
        let from = deepest.and_then(|(index, deepest)| Some((index, specs.iter().position(|s| path_key(&s.path) == deepest)?)));
        let Some((index, at)) = from else {
            self.reveal(path, directory);
            self.fold_to_last();
            return;
        };
        let here = self.snapshot();
        let mut specs = specs.into_iter().skip(at);
        // The deepest folder points on toward the path, then the folders below it open.
        if let Some(spec) = specs.next() {
            let c = &mut self.columns[index];
            c.selected = spec.selected.unwrap_or_default();
            c.focused = spec.highlight.clone().or_else(|| c.selected.last().cloned());
            c.highlight = spec.highlight;
        }
        self.columns.truncate(index + 1);
        for spec in specs {
            let mut c = self.column(Kind::Folder, &spec.path, spec.highlight.clone());
            c.selected = spec.selected.unwrap_or_default();
            c.focused = spec.highlight.or_else(|| c.selected.last().cloned());
            self.columns.push(c);
        }
        let at = self.columns.iter().position(|c| c.highlight.as_deref() == Some(path));
        self.active = if directory { self.columns.len() - 1 } else { at.unwrap_or(self.columns.len() - 1) };
        self.note_folder();
        self.remember(here);
    }

    /// The last column stands alone, the folders above it folded away, the keys in it.
    pub(super) fn fold_to_last(&mut self) {
        if self.columns.len() > 2 {
            let last = self.columns.len() - 1;
            self.fold = Some(Fold { anchor: self.columns[last].id, open: false });
            self.active = last;
            self.note_folder();
        }
    }
}
