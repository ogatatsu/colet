//! A column: what kind it is, what it lists and has picked, where the keys are in it, and what its
//! narrowing shows of it.
use super::*;
use crate::words;

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
    /// Shared, so a filter, a page or a frame takes them up without copying them.
    pub entries: Vec<Arc<Entry>>,
    /// How many there are in all, of which `entries` may be the first pages.
    pub total: usize,
    pub selected: Vec<PathBuf>,
    /// The row the keys are on.
    pub focused: Option<PathBuf>,
    /// The row the keys were on until a filter hid it: they go back to it once it shows again, unless
    /// they have gone to another since.
    pub(super) hid: Option<PathBuf>,
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
    pub(super) base: Option<Vec<PathBuf>>,
    /// What to pick once listed, by path key: the items just pasted here, which the listing that shows
    /// them may come after the paste is done.
    pub pick: Option<Vec<OsString>>,
    /// The results entered while nothing was listed yet, as with Enter in the search field before they
    /// come: the listing that brings rows lands the keys on them, as entering would have.
    pub entering: bool,
    /// A column narrowed to some names: `entries` then holds only those.
    pub narrow: Option<Narrowing>,
}

impl Column {
    /// Where a row's `name` holds the words its filter narrows it to, found as the row is drawn rather
    /// than kept for every row; `None` when they are the entry's own marks, as Everything marks a
    /// result.
    pub fn narrow_marks(&self, name: &str) -> Option<Vec<Range<usize>>> {
        let filter = self.narrow.as_ref().filter(|f| self.kind != Kind::Search && !f.needles.is_empty())?;
        Some(filter.needles.holds(name).unwrap_or_default())
    }

    pub(super) fn new(id: ColumnId, kind: Kind, path: &Path, highlight: Option<PathBuf>) -> Self {
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
            entering: false,
            narrow: None,
        }
    }

    /// The row the keys land on `by` rows from theirs (clamped at the ends); with none yet, from the item
    /// the column points at, or above its first row.
    pub(super) fn landing(&self, by: isize) -> Option<usize> {
        let last = self.entries.len().checked_sub(1)? as isize;
        let from = self.focused.as_ref().or(self.highlight.as_ref()).and_then(|p| self.entries.iter().position(|e| &e.path == p));
        Some(match from {
            Some(from) => (from as isize + by).clamp(0, last),
            None if by > 0 => (by - 1).clamp(0, last),
            None => 0,
        } as usize)
    }

    /// Where a Shift range to `path` starts, and the rows it takes in.
    pub(super) fn range_to(&self, path: &Path) -> Option<(Option<PathBuf>, Vec<PathBuf>)> {
        let position = |p: &PathBuf| self.entries.iter().position(|e| &e.path == p);
        let to = self.entries.iter().position(|e| e.path == path)?;
        let anchor = self.anchor.clone().or_else(|| self.focused.clone()).filter(|a| position(a).is_some());
        let from = anchor.as_ref().and_then(position).unwrap_or(to);
        let range = self.entries[from.min(to)..=from.max(to)].iter().map(|e| e.path.clone()).collect();
        Some((anchor, range))
    }

    /// Every row it shows is picked, and it shows some.
    pub fn all_selected(&self) -> bool {
        if self.entries.is_empty() || self.selected.len() < self.entries.len() {
            return false;
        }
        // Asked on every frame, so a set: a list searched per row is quadratic in a large folder.
        let selected: HashSet<&Path> = self.selected.iter().map(PathBuf::as_path).collect();
        self.entries.iter().all(|e| selected.contains(e.path.as_path()))
    }

    /// Loading with nothing listed yet: its total is not known, not 0.
    pub fn unread(&self) -> bool {
        self.loading && self.entries.is_empty()
    }

    pub fn title(&self) -> Cow<'_, str> {
        match self.kind {
            Kind::Home => words::home().into(),
            Kind::Search => words::search_results().into(),
            Kind::Folder => folder_name(&self.path).to_string_lossy(),
        }
    }

    pub fn entry(&self, path: &Path) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path).map(|e| &**e)
    }

    /// What it is narrowed to, as a favorite or saved search keeps it.
    pub fn narrowing(&self) -> Option<String> {
        self.narrow.as_ref().and_then(|f| narrowing(&f.text))
    }

    /// Narrowed to some names, by its field: not just leaving out the hidden items.
    pub fn is_narrowed(&self) -> bool {
        self.narrow.as_ref().is_some_and(|f| !f.text.is_empty())
    }

    /// How many it lists in all, less the hidden items it leaves out.
    pub fn shown_total(&self) -> usize {
        self.total - self.narrow.as_ref().map_or(0, |f| f.concealed)
    }

    /// The picks it shows, which are what is acted on: those a filter hides stay picked only to keep what
    /// they opened open.
    pub fn shown_selected(&self) -> Cow<'_, [PathBuf]> {
        if self.narrow.is_none() {
            return Cow::Borrowed(&self.selected);
        }
        let listed: HashSet<&Path> = self.entries.iter().map(|e| e.path.as_path()).collect();
        self.selected.iter().filter(|p| listed.contains(p.as_path())).cloned().collect()
    }
}
