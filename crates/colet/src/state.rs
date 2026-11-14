//! Where the user is: the columns on screen, what each lists and has picked, and the way back and forward.
//! Knows nothing of drawing or of where listings come from.
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ffi::OsString,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use colet_core::{
    entry::{Entry, Hiding, Marks, Order},
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
    /// Shared, so a filter, a page or a frame takes them up without copying them.
    pub entries: Vec<Arc<Entry>>,
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
    pub narrow: Option<Narrowing>,
}

/// The names a column is narrowed to, those holding `text`, and all it lists, kept aside. A folder column
/// that leaves out the hidden items has one too, its `text` empty when nothing else narrows it.
#[derive(Clone, Debug)]
pub struct Narrowing {
    text: String,
    /// The words of `text`, read once rather than for each name.
    needles: Needles,
    /// Shared with `entries`, which holds those it lets through.
    all: Vec<Arc<Entry>>,
    /// The search results, which Everything narrows, keep nothing aside: how many the search finds
    /// without the filter stands for them.
    pub whole: usize,
    /// How many of `all` are hidden items left out.
    concealed: usize,
}

impl Narrowing {
    fn new(text: impl Into<String>, all: Vec<Arc<Entry>>, whole: usize) -> Self {
        let text = text.into();
        Self { needles: Needles::new(&text), text, all, whole, concealed: 0 }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The same, narrowed to `text` instead.
    fn retext(self, text: impl Into<String>) -> Self {
        let text = text.into();
        Self { needles: Needles::new(&text), text, ..self }
    }
}

/// The words of a filter, each lowered once, to look for in many names.
#[derive(Clone, Debug, Default)]
pub struct Needles(Vec<String>);

impl Needles {
    pub fn new(text: &str) -> Self {
        Self(narrow_words(text).iter().map(|word| word.to_lowercase()).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Where `name` holds each word, in any order, if it holds them all: the ranges of all of them, those
    /// that meet as one. No words, nothing marked, are held by any name.
    pub fn holds(&self, name: &str) -> Option<Vec<Range<usize>>> {
        let mut ranges = Vec::new();
        for word in &self.0 {
            let start = ranges.len();
            found_lowered(name, word, &mut ranges);
            if ranges.len() == start {
                return None;
            }
        }
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for r in ranges {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        Some(merged)
    }
}

/// Where `needle` stands in `name`, case aside, each time it does: byte ranges of `name`.
#[cfg(test)]
pub fn found(name: &str, needle: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    found_lowered(name, &needle.to_lowercase(), &mut ranges);
    ranges
}

/// Where the already lowered `needle` stands in `name`, case aside, each time it does, added to `ranges`. Run over every name in a folder at each
/// key typed, so it allocates nothing of its own.
fn found_lowered(name: &str, needle: &str, ranges: &mut Vec<Range<usize>>) {
    if needle.is_empty() {
        return;
    }
    // An ASCII name lowers byte for byte into ASCII, which only an ASCII word can match: the bytes are
    // compared as they are.
    if name.is_ascii() && needle.is_ascii() {
        let (name, needle) = (name.as_bytes(), needle.as_bytes());
        let mut i = 0;
        while i + needle.len() <= name.len() {
            if name[i..i + needle.len()].eq_ignore_ascii_case(needle) {
                ranges.push(i..i + needle.len());
                i += needle.len();
            } else {
                i += 1;
            }
        }
        return;
    }
    let mut at = 0;
    while let Some(c) = name[at..].chars().next() {
        match lowered_match(&name[at..], needle) {
            Some(len) => {
                ranges.push(at..at + len);
                at += len;
            }
            None => at += c.len_utf8(),
        }
    }
}

/// How long the start of `rest` is that lowers, a letter at a time, into exactly `needle`: a letter may
/// lower into more than one, and one that runs past the needle's end does not match.
fn lowered_match(rest: &str, needle: &str) -> Option<usize> {
    let mut wanted = needle.chars();
    for (at, c) in rest.char_indices() {
        for lower in c.to_lowercase() {
            if wanted.next() != Some(lower) {
                return None;
            }
        }
        if wanted.as_str().is_empty() {
            return Some(at + c.len_utf8());
        }
    }
    None
}

/// The words a filter's `text` asks a name to hold, apart at any space, half or full width, but in quotes,
/// which are taken out. A quote left open is closed at the end, as Everything closes it.
pub fn narrow_words(text: &str) -> Vec<String> {
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

/// A filter's `text` as a favorite or saved search keeps it: the spaces at its ends gone, and each run of
/// them between its words one; those in quotes, part of a word, as they are. None when it holds no word.
pub fn narrowing(text: &str) -> Option<String> {
    if narrow_words(text).is_empty() {
        return None;
    }
    let (mut kept, mut quoted, mut space) = (String::new(), false, false);
    for c in text.trim().chars() {
        if c.is_whitespace() && !quoted {
            space = true;
            continue;
        }
        if std::mem::take(&mut space) {
            kept.push(' ');
        }
        if c == '"' {
            quoted = !quoted;
        }
        kept.push(c);
    }
    Some(kept)
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
/// For one name; many are asked through [`Needles`].
pub fn holds(name: &str, text: &str) -> Option<Vec<Range<usize>>> {
    Needles::new(text).holds(name)
}

impl Column {
    /// Where a row's `name` holds the words its filter narrows it to, found as the row is drawn rather
    /// than kept for every row; `None` when they are the entry's own marks, as Everything marks a
    /// result.
    pub fn narrow_marks(&self, name: &str) -> Option<Vec<Range<usize>>> {
        let filter = self.narrow.as_ref().filter(|f| self.kind != Kind::Search && !f.needles.is_empty())?;
        Some(filter.needles.holds(name).unwrap_or_default())
    }

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
            narrow: None,
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
            Kind::Home => "ホーム".into(),
            Kind::Search => "検索結果".into(),
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

/// A search result or favorite's own column, the anchor, and whether the folders above it are out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fold<A> {
    pub anchor: A,
    pub open: bool,
}

/// A place to go back or forward to.
#[derive(Clone, Debug)]
struct Location {
    query: String,
    search_filter: Option<String>,
    search_order: Order,
    folders: Vec<(PathBuf, Option<PathBuf>)>,
    selections: Vec<Vec<PathBuf>>,
    active: usize,
    /// Its anchor counted among the folders.
    fold: Option<Fold<usize>>,
}

/// A tab as it is kept between starts: what it searched, the folders open, what each column picked where
/// it leads on, the column worked in, and the fold. What is picked besides, the narrowings and the way back
/// and forth are not kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kept {
    pub query: String,
    pub search_filter: Option<String>,
    /// The order its search results are listed in.
    pub search_order: Order,
    /// Each folder column's folder, and the item it points at.
    pub folders: Vec<(PathBuf, Option<PathBuf>)>,
    /// What each column picked where it leads on, home's first.
    pub picks: Vec<Option<PathBuf>>,
    pub active: usize,
    /// The anchor counted among the folders, and whether the folders above it are out.
    pub fold: Option<Fold<usize>>,
}

impl Kept {
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

/// What a row of home stands for, read from the path it is keyed by. Home's rows go through the columns
/// as any other's, by path, so a narrowed favorite and a saved search are keyed by paths no folder has:
/// a favorite narrowed to some names by its folder's path, a NUL and the narrowing, which no path can hold;
/// a saved search by [`SEARCH_KEY`] and its query, its filter's name after a NUL, what its results are
/// narrowed to after another, and their order, unless the default, after a third, none of which can hold
/// one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HomeKey<'a> {
    /// A place, or a favorite: its folder, and what a favorite opens narrowed to.
    Folder { path: Cow<'a, Path>, narrow: Option<Cow<'a, str>> },
    /// A saved search: its query, the filter it runs within, what its results are narrowed to, and the
    /// order they are listed in.
    Search { query: &'a str, filter: Option<&'a str>, narrow: Option<&'a str>, order: Order },
}

/// What a saved search's key starts with; no path starts this way, a drive letter being one character.
const SEARCH_KEY: &str = "search:";

impl<'a> HomeKey<'a> {
    pub fn favorite(path: &'a Path, narrow: Option<&'a str>) -> Self {
        HomeKey::Folder { path: Cow::Borrowed(path), narrow: narrow.map(Cow::Borrowed) }
    }

    pub fn search(query: &'a str, filter: Option<&'a str>, narrow: Option<&'a str>, order: Order) -> Self {
        HomeKey::Search { query, filter, narrow, order }
    }

    /// What `key` stands for.
    pub fn of(key: &'a Path) -> Self {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        if let Some(search) = key.to_str().and_then(|key| key.strip_prefix(SEARCH_KEY)) {
            let mut parts = search.splitn(4, '\0');
            let query = parts.next().unwrap_or_default();
            // A filter's name and a narrowing are never empty: an empty one stands for none, before what
            // follows it.
            let filter = parts.next().filter(|filter| !filter.is_empty());
            let narrow = parts.next().filter(|narrow| !narrow.is_empty());
            let order = parts.next().and_then(|order| serde_json::from_str(order).ok()).unwrap_or_default();
            return HomeKey::Search { query, filter, narrow, order };
        }
        let wide: Vec<u16> = key.as_os_str().encode_wide().collect();
        match wide.iter().position(|&unit| unit == 0) {
            Some(at) => HomeKey::Folder {
                path: Cow::Owned(OsString::from_wide(&wide[..at]).into()),
                narrow: Some(Cow::Owned(String::from_utf16_lossy(&wide[at + 1..]))),
            },
            None => HomeKey::Folder { path: Cow::Borrowed(key), narrow: None },
        }
    }

    /// The path its row is keyed by.
    pub fn key(&self) -> PathBuf {
        match self {
            HomeKey::Folder { path, narrow: None } => path.to_path_buf(),
            HomeKey::Folder { path, narrow: Some(narrow) } => {
                let mut key = path.as_os_str().to_owned();
                key.push("\0");
                key.push(narrow.as_ref());
                key.into()
            }
            HomeKey::Search { query, filter, narrow, order } => {
                // The default order is no order of its own, so a search saved before orders were is the same.
                let order = (*order != Order::default()).then(|| serde_json::to_string(order).unwrap_or_default());
                let mut key = format!("{SEARCH_KEY}{query}");
                if filter.is_some() || narrow.is_some() || order.is_some() {
                    key.push('\0');
                    key.push_str(filter.unwrap_or_default());
                }
                if narrow.is_some() || order.is_some() {
                    key.push('\0');
                    key.push_str(narrow.unwrap_or_default());
                }
                if let Some(order) = order {
                    key.push('\0');
                    key.push_str(&order);
                }
                key.into()
            }
        }
    }

    pub fn is_search(&self) -> bool {
        matches!(self, HomeKey::Search { .. })
    }

    /// What tells home's rows apart: a place whatever its case, a saved search by its query, filter,
    /// narrowing and order exactly, and a narrowed favorite by its folder whatever its case and its narrowing exactly.
    pub fn pin(&self) -> OsString {
        match self {
            HomeKey::Folder { path, narrow } => {
                let mut pin = path_key(path);
                if let Some(narrow) = narrow {
                    pin.push("\0");
                    pin.push(narrow.as_ref());
                }
                pin
            }
            HomeKey::Search { .. } => self.key().into_os_string(),
        }
    }

    /// What the row of `key` stands for on disk: a place's or favorite's folder, a saved search's own key.
    pub fn on_disk(key: &Path) -> Cow<'_, Path> {
        match HomeKey::of(key) {
            HomeKey::Folder { path, .. } => path,
            HomeKey::Search { .. } => Cow::Borrowed(key),
        }
    }
}

/// How far back it goes.
const HISTORY: usize = 50;

#[derive(Clone)]
pub struct State {
    pub query: String,
    /// The search filter the results are narrowed by, by name; with one, they show with no query too.
    pub search_filter: Option<String>,
    /// The order the search results are listed in: this tab's own, kept as it searches again.
    pub search_order: Order,
    pub columns: Vec<Column>,
    /// The column being worked in.
    pub active: usize,
    past: Vec<Location>,
    future: Vec<Location>,
    /// Something to tell the user, once.
    pub notice: Option<String>,
    /// Paths held in another case than the disk's, found so by a listing, each with the disk's: what keeps
    /// them, a favorite or a pin, takes it too.
    pub respelled: Vec<(PathBuf, PathBuf)>,
    /// A search result or favorite opens on its own column with the folders above it folded away until
    /// asked for. Gone with the anchor.
    pub fold: Option<Fold<ColumnId>>,
    /// The folder column worked in last, which → from the results or home goes back to while it stays.
    last_folder: Option<ColumnId>,
    next_id: ColumnId,
    /// Which hidden items the folder columns list; leaving any out, they still show the one on the way to
    /// where the user is: the folder open beside, or the item pointed at.
    showing: Showing,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

/// Which of the hidden items the folder columns list: the protected operating system files only along
/// with the rest, as in Explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Showing {
    pub hidden: bool,
    pub protected: bool,
}

impl Showing {
    fn shows(self, hiding: Hiding) -> bool {
        match hiding {
            Hiding::Shown => true,
            Hiding::Hidden => self.hidden,
            Hiding::Protected => self.hidden && self.protected,
        }
    }
}

/// An item known only by its path and whether it is a folder, as one typed or kept in the history.
fn bare_entry(path: &Path, directory: bool) -> Entry {
    Entry {
        path: path.into(),
        name: shown(folder_name(path)),
        parent: parent_folder(path).map(shown).unwrap_or_default(),
        directory,
        size: None,
        modified: None,
        hidden: Hiding::Shown,
        marks: Marks::default(),
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
                Spec { path: path.clone(), selected: Some(Vec::new()), highlight: (!entry.directory).then(|| entry.path.clone()) }
            }
        })
        .collect()
}

impl State {
    pub fn new() -> Self {
        let mut state = Self {
            query: String::new(),
            search_filter: None,
            search_order: Order::default(),
            columns: Vec::new(),
            active: 0,
            past: Vec::new(),
            future: Vec::new(),
            notice: None,
            respelled: Vec::new(),
            fold: None,
            last_folder: None,
            next_id: 0,
            showing: Showing { hidden: true, protected: true },
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
            search_order: self.search_order,
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
        self.search_order = location.search_order;
        self.active = location.active.min(columns.len() - 1);
        self.columns = columns;
        self.fold = anchor;
    }

    /// What a tab keeps between starts: each column's pick only where it leads on to the next.
    pub fn kept(&self) -> Kept {
        let Location { query, search_filter, search_order, folders, active, fold, .. } = self.snapshot();
        let last = self.columns.len() - 1;
        let picks = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| match c.selected.as_slice() {
                [one] if i < last => Some(one.clone()),
                _ => None,
            })
            .collect();
        Kept { query, search_filter, search_order, folders, picks, active, fold }
    }

    /// A tab as it was kept, its columns yet to be listed.
    pub fn from_kept(kept: Kept) -> Self {
        let mut state = Self::new();
        let Kept { query, search_filter, search_order, folders, picks, active, fold } = kept;
        let selections = picks.into_iter().map(|pick| pick.into_iter().collect()).collect();
        state.restore(Location { query, search_filter, search_order, folders, selections, active, fold });
        state.note_folder();
        state
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
            if location.search_filter.as_deref() == Some(from) {
                location.search_filter = to.map(Into::into);
            }
        }
        if self.search_filter.as_deref() != Some(from) {
            return;
        }
        self.search_filter = to.map(Into::into);
        if to.is_none() {
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
        self.note_folder();
    }

    fn note_folder(&mut self) {
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

    fn select_rows(&mut self, index: usize, paths: Vec<PathBuf>) {
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

    /// A saved search in home runs its query beside it, narrowed as it was kept. Home keeps the pick and
    /// the focus, so the next one is a keystroke away.
    fn run_saved_search(&mut self, index: usize, path: PathBuf, here: Location) {
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
    fn leave_search_for(&mut self, paths: Vec<PathBuf>, here: Location) {
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

    /// Picks `paths` in the column at `index`, opening to its right what a single pick shows.
    fn pick_rows(&mut self, index: usize, paths: Vec<PathBuf>, here: Location) {
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

    /// The columns `specs` asks for after the one at `index`, and whether those already there show just
    /// that. Columns already showing the same folder are kept, so stepping through results under a shared
    /// ancestor does not reload it.
    fn chain_after(&mut self, index: usize, specs: Vec<Spec>) -> (Vec<Column>, bool) {
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
    fn fold_to_last(&mut self) {
        if self.columns.len() > 2 {
            let last = self.columns.len() - 1;
            self.fold = Some(Fold { anchor: self.columns[last].id, open: false });
            self.active = last;
            self.note_folder();
        }
    }

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
                if renamed {
                    e.name = name.clone();
                }
                if let Some(path) = path {
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
            c.entries.push(Arc::new(Entry { parent: shown(parent), ..Entry::folder(path, &shown(folder_name(path))) }));
            c.total += 1;
        }
        // It shows, to be seen picked, even where the filter would hide it: the filter's text goes then.
        let text = text.filter(|text| holds(&shown(folder_name(path)), text).is_some()).unwrap_or_default();
        self.renarrow(index, text);
        self.select(index, vec![path.to_owned()]);
    }

    /// The folder columns list the hidden items `showing` lets through, and leave out the rest: those left
    /// out stay picked, and what they opened open, as a filter leaves what it hides.
    pub fn set_showing(&mut self, showing: Showing) {
        if self.showing == showing {
            return;
        }
        self.showing = showing;
        for index in 0..self.columns.len() {
            if self.columns[index].kind == Kind::Folder {
                let text = self.columns[index].narrow.as_ref().map(|f| f.text.clone());
                self.set_narrowing(index, text.as_deref());
            }
        }
    }

    /// Whether the column at `index` leaves out any hidden items.
    fn conceals(&self, index: usize) -> bool {
        !(self.showing.hidden && self.showing.protected) && self.columns[index].kind == Kind::Folder
    }

    /// Puts all the folder column at `index` lists, in its `entries` with no filter, under one of `text`,
    /// when that or the hidden items left out narrow it.
    fn renarrow(&mut self, index: usize, text: String) {
        if text.is_empty() && !self.conceals(index) {
            return;
        }
        let c = &mut self.columns[index];
        c.narrow = Some(Narrowing::new(text, std::mem::take(&mut c.entries), 0));
        self.narrow(index);
    }

    /// Narrows the column at `index` to the names holding the words of `text`, or with none, or no words in
    /// it, lists them all again. What it hides stays picked, and what that opened open, while another is looked for. The
    /// search results are narrowed by Everything, asked again: they show as they are until it answers.
    pub fn set_narrowing(&mut self, index: usize, text: Option<&str>) {
        let Some(c) = self.columns.get_mut(index) else { return };
        let text = text.filter(|text| !narrow_words(text).is_empty());
        if c.kind == Kind::Search {
            c.narrow = match (text, c.narrow.take()) {
                (None, _) => None,
                (Some(text), Some(filter)) => Some(filter.retext(text)),
                (Some(text), None) => Some(Narrowing::new(text, Vec::new(), c.total)),
            };
            return;
        }
        if let Some(filter) = c.narrow.take() {
            c.entries = filter.all;
            // All show again: the keys go back to the row the filter took them off, unless it hides it
            // still.
            if text.is_none() {
                let hid = c.hid.take();
                if c.focused.is_none() {
                    c.focused = hid;
                }
            }
        }
        self.renarrow(index, text.unwrap_or_default().into());
    }

    /// Lists what the column's filter lets through; the keys let go of a row it hides, kept to go back
    /// to once it shows again, while they have gone to no other.
    fn narrow(&mut self, index: usize) {
        let (conceal, showing) = (self.conceals(index), self.showing);
        // The hidden item on the way to where the user is shows still: the folder open beside, or the item
        // pointed at.
        let beside = self.columns.get(index + 1).filter(|next| next.kind == Kind::Folder).map(|next| next.path.clone());
        let c = &mut self.columns[index];
        if c.kind == Kind::Search {
            return;
        }
        let Some(filter) = c.narrow.as_mut() else { return };
        let on_the_way = |path: &PathBuf| beside.as_ref() == Some(path) || c.highlight.as_ref() == Some(path);
        let mut concealed = 0;
        c.entries = filter
            .all
            .iter()
            .filter_map(|e| {
                if conceal && !showing.shows(e.hidden) && !on_the_way(&e.path) {
                    concealed += 1;
                    return None;
                }
                filter.needles.holds(&e.name).is_some().then(|| e.clone())
            })
            .collect();
        filter.concealed = concealed;
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
        c.narrow.as_ref().is_some_and(|f| f.all.iter().any(|e| e.path == path && f.needles.holds(&e.name).is_none()))
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
    fn respell(&mut self, index: usize) {
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
    fn list(&mut self, index: usize, entries: Vec<Entry>, total: usize, offset: usize, departed: &HashSet<OsString>) {
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
    }
}

#[cfg(test)]
// A mark is a list of ranges, most often holding only one.
#[allow(clippy::single_range_in_vec_init)]
mod tests;
