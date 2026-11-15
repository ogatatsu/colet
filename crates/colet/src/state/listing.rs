//! The columns' listings as they come: loading, a page or the whole, a count, an error, and keeping the
//! picks and the keys on what is still there.
use super::*;

impl State {
    /// Picks what the column waits to pick, once it lists any of it.
    pub(super) fn pick_listed(&mut self, index: usize) -> bool {
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
    pub fn page(&mut self, id: ColumnId, request: u64, entries: Vec<Entry>, total: usize, offset: usize, departed: &HashSet<OsString>) {
        let Some(index) = self.index_of(id) else { return };
        let c = &mut self.columns[index];
        if c.request != request {
            return;
        }
        // Everything narrowed the results already, leaving out what the filter hides; that stays picked, as
        // in the other columns, told by its name.
        if c.kind == Kind::Search {
            let hidden: Vec<PathBuf> = match &c.narrow {
                Some(filter) => c.selected.iter().filter(|p| filter.needles.holds(&shown(folder_name(p))).is_none()).cloned().collect(),
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
                } else if c.narrow.as_ref().is_some_and(|f| f.needles.holds(&shown(folder_name(&path))).is_none()) {
                    c.focused = None;
                    c.hid = Some(path);
                }
            }
            return;
        }
        // The listing stands for all the column lists; the filter narrows it again after.
        let text = c.narrow.take().map(|f| {
            c.entries = f.all;
            f.text
        });
        let waiting = c.pick.is_some();
        self.list(index, entries, total, offset, departed);
        let mut text = text.unwrap_or_default();
        let c = &self.columns[index];
        // What was just pasted here shows picked, as a new folder does: the filter's text goes when it hides
        // any.
        let picked = waiting && c.pick.is_none() && index == self.active;
        let needles = Needles::new(&text);
        if picked && c.selected.iter().any(|p| c.entry(p).is_some_and(|e| needles.holds(&e.name).is_none())) {
            text.clear();
        }
        self.renarrow(index, text);
    }

    /// How many the search finds without the filter Everything narrows it by, asked along with `request`.
    pub fn counted(&mut self, id: ColumnId, request: u64, whole: usize) {
        let Some(index) = self.index_of(id) else { return };
        let c = &mut self.columns[index];
        if c.request != request {
            return;
        }
        if let Some(filter) = &mut c.narrow {
            filter.whole = whole;
        }
    }

    /// What the folder column at `index` holds in another case than it lists, as a favorite kept from
    /// before a rename elsewhere opens it, takes the case listed: the folder open beside it, and what is
    /// picked or pointed at there. A name listed as it is held wins over one only its case sets apart.
    pub(super) fn respell(&mut self, index: usize) {
        let c = &self.columns[index];
        let listed: HashSet<&Path> = c.entries.iter().map(|e| e.path.as_path()).collect();
        let held =
            self.columns.get(index + 1).map(|child| &child.path).into_iter().chain(&c.selected).chain(&c.focused).chain(&c.highlight);
        let mut strays: Vec<&PathBuf> = held.filter(|p| !listed.contains(p.as_path())).collect();
        if strays.is_empty() {
            return;
        }
        strays.dedup();
        // Looked up rather than searched for: a folder may list thousands.
        let keyed: HashMap<OsString, &PathBuf> = c.entries.iter().map(|e| (path_key(&e.path), &e.path)).collect();
        let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
        for from in strays {
            if let Some(&to) = keyed.get(&path_key(from))
                && !found.iter().any(|(f, _)| f == from)
            {
                found.push((from.clone(), to.clone()));
            }
        }
        for (from, to) in found {
            self.renamed(&from, &to);
            self.respelled.push((from, to));
        }
    }

    /// What [`page`](Self::page) does with a listing that is the one the column waits for.
    pub(super) fn list(&mut self, index: usize, entries: Vec<Entry>, total: usize, offset: usize, departed: &HashSet<OsString>) {
        let c = &mut self.columns[index];
        let old = std::mem::take(&mut c.entries);
        if offset > 0 {
            let known: HashSet<&PathBuf> = old.iter().map(|e| &e.path).collect();
            let fresh: Vec<Arc<Entry>> = entries.into_iter().filter(|e| !known.contains(&e.path)).map(Arc::new).collect();
            c.entries = old.iter().cloned().chain(fresh).collect();
        } else {
            c.entries = entries.into_iter().map(Arc::new).collect();
        }
        if c.kind == Kind::Folder {
            // Listed under the folder as asked for, which may since have taken the disk's case.
            let folder = c.path.clone();
            if c.entries.iter().any(|e| !e.path.starts_with(&folder)) {
                for e in &mut c.entries {
                    if let Some(path) = paths::moved(&e.path, &folder, &folder) {
                        Arc::make_mut(e).path = path;
                    }
                }
            }
            self.respell(index);
        }
        let c = &mut self.columns[index];
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
            && self
                .columns
                .get(index + 1)
                .is_some_and(|child| !self.columns[index].entries.iter().any(|e| e.path == child.path && e.directory));
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
        // Entered before it listed anything: the keys land now, unless they have gone to another column or
        // a row since.
        let c = &mut self.columns[index];
        if std::mem::take(&mut c.entering) && index == self.active && c.focused.is_none() {
            self.enter();
        }
    }
}
