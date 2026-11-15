//! The way back and forward: the places gone through, each the columns open and what each picked, and
//! the place a tab is kept as between starts.
use super::*;

/// A search result or favorite's own column, the anchor, and whether the folders above it are out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fold<A> {
    pub anchor: A,
    pub open: bool,
}

/// Where a tab stands: what it searched, the folders open, what each column picked as `P`, the column
/// worked in, and the fold.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place<P> {
    pub query: String,
    pub search_filter: Option<String>,
    /// The order its search results are listed in.
    pub search_order: Order,
    /// Each folder column's folder, and the item it points at.
    pub folders: Vec<(PathBuf, Option<PathBuf>)>,
    /// What each column picked, home's first.
    pub picks: Vec<P>,
    pub active: usize,
    /// The anchor counted among the folders, and whether the folders above it are out.
    pub fold: Option<Fold<usize>>,
}

/// A place to go back or forward to, with each column's whole selection.
pub(super) type Location = Place<Vec<PathBuf>>;

/// A tab as it is kept between starts, with each column's pick only where it leads on. What is picked
/// besides, the narrowings and the way back and forth are not kept.
pub type Kept = Place<Option<PathBuf>>;

impl<P> Place<P> {
    /// The same place with each column's pick, by its index, made into a `Q`.
    pub(super) fn map_picks<Q>(self, mut pick: impl FnMut(usize, P) -> Q) -> Place<Q> {
        let Place { query, search_filter, search_order, folders, picks, active, fold } = self;
        let picks = picks.into_iter().enumerate().map(|(i, p)| pick(i, p)).collect();
        Place { query, search_filter, search_order, folders, picks, active, fold }
    }

    /// The search filter `from` is named `to` now, or is gone, as [`State::rename_search_filter`] has it.
    pub fn rename_search_filter(&mut self, from: &str, to: Option<&str>) {
        if self.search_filter.as_deref() == Some(from) {
            self.search_filter = to.map(Into::into);
        }
    }

    /// A tab kept by an older Colet, which searched with a filter alone, shows home in its place: a filter
    /// searches only with something typed. The results column goes, and what was picked there with it.
    pub fn without_filter_alone(&mut self) {
        if !self.query.is_empty() || self.search_filter.take().is_none() {
            return;
        }
        if self.picks.len() > 1 {
            self.picks.remove(1);
        }
        self.active = self.active.saturating_sub(1);
    }
}

/// The folder columns folded above the anchor: from `from` up to `to`, where it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Folded {
    pub from: usize,
    pub to: usize,
    pub open: bool,
}

/// How far back it goes.
pub(super) const HISTORY: usize = 50;

impl State {
    pub fn can_go_back(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.future.is_empty()
    }

    pub(super) fn snapshot(&self) -> Location {
        self.place(|_, selected| selected.to_vec())
    }

    /// Where the tab is, with each column's pick, by its index, as `pick` makes it of what it picked:
    /// only that is copied, not every column's whole selection.
    pub(super) fn place<P>(&self, mut pick: impl FnMut(usize, &[PathBuf]) -> P) -> Place<P> {
        let folders: Vec<&Column> = self.columns.iter().filter(|c| c.kind == Kind::Folder).collect();
        Place {
            query: self.query.clone(),
            search_filter: self.search_filter.clone(),
            search_order: self.search_order,
            folders: folders.iter().map(|c| (c.path.clone(), c.highlight.clone())).collect(),
            picks: self.columns.iter().enumerate().map(|(i, c)| pick(i, &c.selected)).collect(),
            active: self.active,
            fold: self.fold.and_then(|fold| {
                let anchor = folders.iter().position(|c| c.id == fold.anchor)?;
                Some(Fold { anchor, open: fold.open })
            }),
        }
    }

    pub(super) fn remember(&mut self, location: Location) {
        if self.past.len() >= HISTORY {
            self.past.remove(0);
        }
        self.past.push(location);
        self.future.clear();
    }

    /// Home stays on screen through every move, so it keeps what it lists and only takes back its
    /// selection; the rest are made afresh.
    pub(super) fn restore(&mut self, location: Location) {
        let home = self.columns.swap_remove(0);
        let mut columns = vec![home];
        if !location.query.is_empty() {
            columns.push(self.column(Kind::Search, Path::new(""), None));
        }
        for (path, highlight) in &location.folders {
            columns.push(self.column(Kind::Folder, path, highlight.clone()));
        }
        for (i, c) in columns.iter_mut().enumerate() {
            c.selected = location.picks.get(i).cloned().unwrap_or_default();
            c.focused = c.selected.last().cloned();
        }
        let anchor = location.fold.and_then(|fold| {
            let column = columns.iter().filter(|c| c.kind == Kind::Folder).nth(fold.anchor)?;
            Some(Fold { anchor: column.id, open: fold.open })
        });
        self.query = location.query;
        self.search_filter = location.search_filter;
        self.search_order = location.search_order;
        self.active = location.active.min(columns.len() - 1);
        self.columns = columns;
        self.fold = anchor;
    }

    /// What a tab keeps between starts: each column's pick only where it leads on to the next.
    pub fn kept(&self) -> Kept {
        let last = self.columns.len() - 1;
        self.place(|i, selected| match selected {
            [one] if i < last => Some(one.clone()),
            _ => None,
        })
    }

    /// A tab as it was kept, its columns yet to be listed.
    pub fn from_kept(kept: Kept) -> Self {
        let mut state = Self::new();
        state.restore(kept.map_picks(|_, pick| pick.into_iter().collect()));
        state.note_folder();
        state
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
}
