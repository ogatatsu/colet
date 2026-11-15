//! What changed on disk by Colet's own hand: an item renamed or moved, a folder made, items pasted.
use super::*;

impl State {
    /// Ahead of the listing that shows it, the renamed item keeps its selection and an open folder its
    /// columns, rather than reading as gone.
    pub fn renamed(&mut self, from: &Path, to: &Path) {
        let moved = |path: &Path| -> PathBuf { paths::moved(path, from, to).unwrap_or_else(|| path.to_owned()) };
        let name = shown(folder_name(to));
        for c in &mut self.columns {
            c.path = moved(&c.path);
            for e in c.entries.iter_mut().chain(c.narrow.iter_mut().flat_map(|f| f.all.iter_mut())) {
                let renamed = e.path == from;
                let path = paths::moved(&e.path, from, to);
                if !renamed && path.is_none() {
                    continue;
                }
                // Shared with the filter's list, where it is changed alike.
                let e = Arc::make_mut(e);
                // The search's marks are byte ranges of the old text: they would fall on the wrong
                // letters, or mid-letter, of the new, so it shows unmarked till the search comes back.
                if renamed {
                    e.name = name.clone();
                    e.marks.name.clear();
                }
                if let Some(path) = path {
                    let parent = colet_core::entry::parent_shown(&path);
                    if parent != e.parent {
                        e.marks.parent.clear();
                    }
                    e.parent = parent;
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
            for selection in &mut location.picks {
                for p in selection {
                    *p = moved(p);
                }
            }
        }
        // Its marks follow the new name; one the filter would hide shows without it.
        for index in 0..self.columns.len() {
            if self.hides(index, to) {
                self.set_narrowing(index, None);
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
        let text = c.narrow.take().map(|f| {
            c.entries = f.all;
            f.text
        });
        if c.entry(path).is_none() {
            c.entries.push(Arc::new(Entry { parent: shown(parent).into(), ..Entry::folder(path, &shown(folder_name(path))) }));
            c.total += 1;
        }
        // It shows, to be seen picked, even where the filter would hide it: the filter's text goes then.
        let text = text.filter(|text| holds(&shown(folder_name(path)), text).is_some()).unwrap_or_default();
        self.renarrow(index, text);
        self.select(index, vec![path.to_owned()]);
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
            self.set_narrowing(index, None);
        }
        self.columns[index].pick = Some(paths.iter().map(|p| path_key(p)).collect());
        self.pick_listed(index);
    }
}
