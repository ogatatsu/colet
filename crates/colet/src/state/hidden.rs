//! Which hidden items show: the setting, and what it hides of each column.
use super::*;

/// Which of the hidden items the folder columns list: the protected operating system files only along
/// with the rest, as in Explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Showing {
    pub hidden: bool,
    pub protected: bool,
}

impl Showing {
    pub(super) fn shows(self, hiding: Hiding) -> bool {
        match hiding {
            Hiding::Shown => true,
            Hiding::Hidden => self.hidden,
            Hiding::Protected => self.hidden && self.protected,
        }
    }
}

impl State {
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
    pub(super) fn conceals(&self, index: usize) -> bool {
        !(self.showing.hidden && self.showing.protected) && self.columns[index].kind == Kind::Folder
    }

    /// Whether the folder column at `index` narrowed to its names, holds `path` but hides it.
    pub(super) fn hides(&self, index: usize, path: &Path) -> bool {
        let c = &self.columns[index];
        c.narrow.as_ref().is_some_and(|f| f.all.iter().any(|e| e.path == path && f.needles.holds(&e.name).is_none()))
    }
}
