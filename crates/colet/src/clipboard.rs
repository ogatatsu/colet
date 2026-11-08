//! What Colet knows of the clipboard: the files on it, and its own writes still under way.
use std::{
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use colet_core::{paths::path_key, shell::clipboard::ClipboardFiles};

use crate::lane::Lane;

pub struct Clipboard {
    /// The files on it, marked in their rows until it changes.
    pub files: ClipboardFiles,
    /// Their paths by path key: another app may write them in another case.
    held: HashSet<OsString>,
    /// Set by the watch on it, and while it could not be read: read it again.
    changed: Arc<AtomicBool>,
    /// Copies and cuts still being put on it.
    writes: usize,
    /// Puts them on it one after another in the order asked, so an earlier copy never lands over a later.
    lane: Lane,
    /// A paste asked for while they were, into this folder: run once they are done, so it pastes them.
    paste_waiting: Option<PathBuf>,
}

impl Default for Clipboard {
    /// Read once at start.
    fn default() -> Self {
        Self {
            files: ClipboardFiles::default(),
            held: HashSet::new(),
            changed: Arc::new(AtomicBool::new(true)),
            writes: 0,
            lane: Lane::default(),
            paste_waiting: None,
        }
    }
}

impl Clipboard {
    /// What the watch on it tells of a change.
    pub fn changed_flag(&self) -> Arc<AtomicBool> {
        self.changed.clone()
    }

    /// It changed since last asked, or could not be read then.
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }

    /// Another app held it: read it again next time.
    pub fn read_again(&self) {
        self.changed.store(true, Ordering::Relaxed);
    }

    pub fn set_files(&mut self, files: ClipboardFiles) {
        self.held = files.paths.iter().map(|p| path_key(p)).collect();
        self.files = files;
    }

    /// `path` is among the files on it, whatever its case.
    pub fn holds(&self, path: &Path) -> bool {
        !self.held.is_empty() && self.held.contains(&path_key(path))
    }

    /// Queues `work` behind the writes before it; the receiver hears how it went.
    pub fn write(&mut self, work: impl FnOnce() -> Result<(), String> + Send + 'static) -> mpsc::Receiver<Result<(), String>> {
        self.writes += 1;
        self.lane.run(work)
    }

    /// A write ended, `written` or not: the paste waiting on the last of them, unless it failed, as the
    /// clipboard then still holds what was there before, never what it was meant to paste.
    pub fn write_ended(&mut self, written: bool) -> Option<PathBuf> {
        self.writes -= 1;
        if self.writes > 0 {
            return None;
        }
        self.paste_waiting.take().filter(|_| written)
    }

    /// A paste into `destination`, to run now; none while writes are under way, as it waits for them.
    pub fn paste_into(&mut self, destination: PathBuf) -> Option<PathBuf> {
        if self.writes > 0 {
            self.paste_waiting = Some(destination);
            return None;
        }
        Some(destination)
    }
}
