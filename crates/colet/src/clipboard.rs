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

use crate::{lane::Lane, tabs::TabId};

pub struct Clipboard {
    /// The files on it, marked in their rows until it changes.
    pub files: ClipboardFiles,
    /// Their paths by path key: another app may write them in another case.
    held: HashSet<OsString>,
    /// Set by the watch on it, and while it could not be read: read it again.
    changed: Arc<AtomicBool>,
    /// Copies and cuts still being put on it.
    writes: usize,
    /// The writes asked for so far, the last of them the one that lands last.
    asked: u64,
    /// Whether that last one was written, once it ended: told in whatever order they end, a paste
    /// waiting on them goes by it alone.
    last_written: bool,
    /// Puts them on it one after another in the order asked, so an earlier copy never lands over a later.
    lane: Lane,
    /// A paste asked for while they were: run once they are done, so it pastes them.
    paste_waiting: Option<Paste>,
}

/// A paste into `destination`, asked for in the tab `tab`, which picks what it pastes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paste {
    pub destination: PathBuf,
    pub tab: TabId,
}

impl Default for Clipboard {
    /// Read once at start.
    fn default() -> Self {
        Self {
            files: ClipboardFiles::default(),
            held: HashSet::new(),
            changed: Arc::new(AtomicBool::new(true)),
            writes: 0,
            asked: 0,
            last_written: false,
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

    /// Queues `work` behind the writes before it: which write it is, and a receiver that hears how it
    /// went.
    pub fn write(&mut self, work: impl FnOnce() -> Result<(), String> + Send + 'static) -> (u64, mpsc::Receiver<Result<(), String>>) {
        self.writes += 1;
        self.asked += 1;
        (self.asked, self.lane.run(work))
    }

    /// The write `which` ended, `written` or not: once all have, the paste waiting on them, unless the
    /// last asked failed, as the clipboard then holds what was there before it, never what it was meant
    /// to paste. They may be told of in another order than they ran.
    pub fn write_ended(&mut self, which: u64, written: bool) -> Option<Paste> {
        self.writes -= 1;
        if which == self.asked {
            self.last_written = written;
        }
        if self.writes > 0 {
            return None;
        }
        self.paste_waiting.take().filter(|_| self.last_written)
    }

    /// `paste`, to run now; none while writes are under way, as it waits for them, in place of one
    /// asked before it.
    pub fn paste(&mut self, paste: Paste) -> Option<Paste> {
        if self.writes > 0 {
            self.paste_waiting = Some(paste);
            return None;
        }
        Some(paste)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two writes, the first and then the second written or not as given, with a paste into `D:\to`
    /// asked while they run; they are told of the second first. What paste runs once both are told of.
    fn pasted_told_backwards(first: bool, second: bool) -> Option<Paste> {
        let mut clipboard = Clipboard::default();
        let (a, _) = clipboard.write(|| Ok(()));
        let (b, _) = clipboard.write(|| Ok(()));
        assert_eq!(clipboard.paste(to(r"D:\to", 1)), None);
        assert_eq!(clipboard.write_ended(b, second), None);
        clipboard.write_ended(a, first)
    }

    fn to(destination: &str, tab: TabId) -> Paste {
        Paste { destination: destination.into(), tab }
    }

    #[test]
    fn pastes_by_the_last_write_asked_not_the_last_told_of() {
        assert_eq!(pasted_told_backwards(false, true), Some(to(r"D:\to", 1)));
        assert_eq!(pasted_told_backwards(true, false), None);
    }

    #[test]
    fn keeps_the_last_paste_asked_while_writing_with_the_tab_it_was_asked_in() {
        let mut clipboard = Clipboard::default();
        let (a, _) = clipboard.write(|| Ok(()));
        assert_eq!(clipboard.paste(to(r"D:\a", 1)), None);
        assert_eq!(clipboard.paste(to(r"D:\b", 2)), None);
        assert_eq!(clipboard.write_ended(a, true), Some(to(r"D:\b", 2)));
        assert_eq!(clipboard.paste(to(r"D:\c", 3)), Some(to(r"D:\c", 3)));
    }
}
