//! Where a tab is scrolled: each column's rows up and down, the columns sideways, and home's list. A tab
//! parked out of sight keeps them, and takes them up again as it comes back.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use gpui_kit::{ScrollHandle, UniformListScrollHandle};

use crate::{reveal::Sideways, state::ColumnId};

#[derive(Default)]
pub struct Scrolls {
    pub rows: RowScrolls,
    pub sideways: Sideways,
    /// Home's list, for its scrollbar.
    pub home: ScrollHandle,
}

/// The rows of each column, scrolled up and down apart from the others.
#[derive(Default)]
pub struct RowScrolls {
    pub handles: HashMap<ColumnId, UniformListScrollHandle>,
    /// The row each column was last scrolled to bring in sight.
    pub(crate) scrolled_to: HashMap<ColumnId, PathBuf>,
}

impl RowScrolls {
    pub fn handle(&mut self, id: ColumnId) -> &UniformListScrollHandle {
        self.handles.entry(id).or_default()
    }

    /// The rows of `id` moved: the focused one is brought in sight again.
    pub fn moved(&mut self, id: ColumnId) {
        self.scrolled_to.remove(&id);
    }

    /// Only the columns `live` holds.
    pub fn keep(&mut self, live: &HashSet<ColumnId>) {
        self.handles.retain(|id, _| live.contains(id));
        self.scrolled_to.retain(|id, _| live.contains(id));
    }
}
