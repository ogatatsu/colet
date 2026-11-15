//! Where the user is: the columns on screen, what each lists and has picked, and the way back and forward.
//! Knows nothing of drawing or of where listings come from.
//!
//! [`State`] is a tab's; each of its parts is a module: a column ([`column`]), narrowing one
//! ([`narrowing`]), which hidden items show ([`hidden`]), home's keys ([`home_key`]), the way back and
//! forward ([`navigation`]), opening a place ([`opening`]), the search ([`search`]), picking
//! ([`selection`]), the listings as they come ([`listing`]), and what Colet changed on disk ([`changes`]).
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

mod changes;
mod column;
mod hidden;
mod home_key;
mod listing;
mod narrowing;
mod navigation;
mod opening;
mod search;
mod selection;

pub use column::{Column, ColumnId, Kind};
pub use hidden::Showing;
pub use home_key::HomeKey;
#[cfg(test)]
pub use narrowing::found;
pub use narrowing::{Narrowing, Needles, holds, narrow_words, narrowing, room_for_a_word};
use navigation::Location;
pub use navigation::{Fold, Folded, Kept};
use opening::{Spec, reveal_chain};

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

    pub fn index_of(&self, id: ColumnId) -> Option<usize> {
        self.columns.iter().position(|c| c.id == id)
    }

    /// Whether the search results stand pinned beside home.
    pub fn pinned(&self) -> bool {
        self.columns.get(1).is_some_and(|c| c.kind == Kind::Search)
    }
}
#[cfg(test)]
// A mark is a list of ranges, most often holding only one.
#[allow(clippy::single_range_in_vec_init)]
mod tests;
