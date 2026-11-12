//! What Colet keeps between starts: the settings, and the orders the columns are given apart from the
//! default, each in a file of its own. Colet changes them through here, which counts the changes so only
//! the last of a burst is written, and writes both at once as the window closes.
use crate::{
    settings::{KeptTab, Queued, Saves, Settings},
    sorts::Sorts,
};

pub struct Stored {
    settings: Settings,
    settings_saves: Saves<Settings>,
    /// The orders the columns are given apart from the default, written as the settings are.
    sorts: Sorts,
    sorts_saves: Saves<Sorts>,
    /// The settings could not be written as the window closed: the next close goes without them.
    unsaved_close: bool,
}

/// The change of a save to send a moment later, once a burst of changes is over; see
/// [`Stored::settings_save`] and [`Stored::sorts_save`].
pub type Change = u64;

impl Stored {
    pub fn new(settings: Settings, settings_saves: Saves<Settings>, sorts: Sorts, sorts_saves: Saves<Sorts>) -> Self {
        Self { settings, settings_saves, sorts, sorts_saves, unsaved_close: false }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn sorts(&self) -> &Sorts {
        &self.sorts
    }

    /// Changes the settings, settled as they are kept: the change to save a moment later, if they are
    /// written at all. Never over a file that could not be read.
    pub fn change_settings(&mut self, change: impl FnOnce(&mut Settings)) -> Option<Change> {
        change(&mut self.settings);
        self.settings.normalize();
        self.settings_saves.changed()
    }

    /// Changes the columns' own orders: the change to save a moment later, if they are written at all.
    pub fn change_sorts(&mut self, change: impl FnOnce(&mut Sorts)) -> Option<Change> {
        change(&mut self.sorts);
        self.sorts_saves.changed()
    }

    /// The save of the settings as they are, if `change` is still the last.
    pub fn settings_save(&self, change: Change) -> Option<Queued> {
        self.settings_saves.save_if_last(change, &self.settings)
    }

    /// The save of the columns' orders as they are, if `change` is still the last.
    pub fn sorts_save(&self, change: Change) -> Option<Queued> {
        self.sorts_saves.save_if_last(change, &self.sorts)
    }

    /// Writes the settings and the columns' orders now, once the saves before are written; the saves still
    /// waiting out a burst are dropped.
    pub fn flush(&mut self) -> Result<(), String> {
        let settings = self.settings_saves.save_now(&self.settings);
        let sorts = self.sorts_saves.save_now(&self.sorts);
        let settings = settings.map_or(Ok(()), Queued::written);
        let sorts = sorts.map_or(Ok(()), Queued::written);
        settings.and(sorts)
    }

    /// The tabs as they were kept, and the one in front, taken up once at start: they are kept again from
    /// the tabs open as the settings are next written.
    pub fn take_kept_tabs(&mut self) -> (Vec<KeptTab>, usize) {
        (std::mem::take(&mut self.settings.tabs), self.settings.tab)
    }

    /// The last close could not write the settings: this one goes without them.
    pub fn is_unsaved_close(&self) -> bool {
        self.unsaved_close
    }

    /// A close could not write the settings: the next goes without them.
    pub fn give_up_saving_on_close(&mut self) {
        self.unsaved_close = true;
    }
}
