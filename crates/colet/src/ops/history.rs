//! What each operation is kept as in the history: its request, how each of its items ended, and how it
//! ended as a whole; and the history itself, newest first.
use super::*;
use crate::words;

/// The most operations kept, and the most items of each.
const MAX_ENTRIES: usize = 100;
const MAX_ITEMS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Done,
    /// Deleted outright: asked so, or one the Recycle Bin could not take.
    Destroyed,
    Failed,
    Skipped,
    Cancelled,
    /// Handed to the shell to write out, which tells nothing of how it went.
    Handed,
}

impl ItemStatus {
    pub fn label(self) -> &'static str {
        match self {
            ItemStatus::Done => words::history::DONE,
            ItemStatus::Destroyed => words::history::DESTROYED,
            ItemStatus::Failed => words::history::FAILED,
            ItemStatus::Skipped => words::history::SKIPPED,
            ItemStatus::Cancelled => words::history::CANCELLED,
            ItemStatus::Handed => words::history::HANDED,
        }
    }
}

/// One item of an operation, as the history lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryItem {
    /// Where it is now; where it was, when it failed or was deleted.
    pub path: PathBuf,
    /// Where a paste or a rename took it from.
    pub from: Option<PathBuf>,
    pub status: ItemStatus,
    pub error: Option<String>,
    /// Sent to the Recycle Bin or deleted outright: nothing to open.
    pub gone: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Done,
    Cancelled,
    Error,
}

/// A file operation Colet ran, kept while it is open.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub id: u64,
    /// What was asked: a paste's paths are left empty, read from the clipboard as it runs.
    pub request: Request,
    /// Milliseconds since the Unix epoch.
    pub started: u64,
    pub status: Status,
    /// The operation's own failure, when no item says it.
    pub error: Option<String>,
    pub items: Vec<HistoryItem>,
    /// Items past the most it keeps, counted only.
    pub more: usize,
    /// How many of those failed and how many were skipped: what it says of its problems counts them too.
    more_failed: usize,
    more_skipped: usize,
}

impl HistoryEntry {
    /// An item's end, added to its operation. Where a copy or move put it follows its result.
    pub fn record(&mut self, event: Event) {
        let (path, status, error) = match event {
            // Said by the item's own end.
            Event::Left { .. } => return,
            Event::Placed { path } => {
                if self.more == 0
                    && let Some(last) = self.items.last_mut()
                {
                    last.from = Some(std::mem::replace(&mut last.path, path));
                }
                return;
            }
            Event::Done { path } => (path, ItemStatus::Done, None),
            Event::Failed { path, error } => (path, ItemStatus::Failed, Some(words::windows_failure(error))),
            Event::Skipped { path } => (path, ItemStatus::Skipped, None),
            Event::Cancelled { path } => (path, ItemStatus::Cancelled, None),
            Event::Handed { path } => (path, ItemStatus::Handed, None),
            Event::Destroyed { .. } => {
                if self.more == 0
                    && let Some(last) = self.items.last_mut()
                {
                    last.status = ItemStatus::Destroyed;
                }
                return;
            }
        };
        let gone = matches!(self.request.kind, Kind::Recycle | Kind::Delete) && status == ItemStatus::Done;
        if self.items.len() < MAX_ITEMS {
            self.items.push(HistoryItem { path, from: None, status, error, gone });
        } else {
            self.more += 1;
            match status {
                ItemStatus::Failed => self.more_failed += 1,
                ItemStatus::Skipped => self.more_skipped += 1,
                _ => {}
            }
        }
    }

    /// How many of its items ended as `status`; of those past the most it keeps, only the failed and the
    /// skipped are told apart.
    pub fn count(&self, status: ItemStatus) -> usize {
        let more = match status {
            ItemStatus::Failed => self.more_failed,
            ItemStatus::Skipped => self.more_skipped,
            _ => 0,
        };
        self.items.iter().filter(|i| i.status == status).count() + more
    }

    /// What it acted on, in a few words.
    pub fn target(&self) -> String {
        let Request { kind, paths, destination, name } = &self.request;
        match kind {
            Kind::Paste => words::history::pasted(self.items.len() + self.more, shown(folder_name(destination))),
            Kind::Recycle | Kind::Delete if paths.len() == 1 => shown(folder_name(&paths[0])),
            Kind::Recycle | Kind::Delete => words::history::items(paths.len()),
            Kind::Rename => words::history::renamed(shown(folder_name(paths.first().map_or(Path::new(""), |p| p))), name),
            Kind::NewFolder => words::history::new_folder(name, shown(folder_name(destination))),
        }
    }

    /// How it went: what failed or was skipped, its own failure, or that it ran to the end.
    pub fn outcome(&self) -> String {
        if self.status == Status::Running {
            return words::history::RUNNING.into();
        }
        let mut parts = self.item_problems();
        if self.status == Status::Cancelled {
            parts.push(words::history::CANCELLED.into());
        }
        if self.status == Status::Error && self.count(ItemStatus::Failed) == 0 {
            parts.push(words::history::FAILED.into());
        }
        if parts.is_empty() { words::history::DONE.into() } else { parts.join(words::SEPARATOR) }
    }

    /// How many of its items failed and how many were skipped, those there were: `1 件失敗`, `1 件スキップ`.
    pub(super) fn item_problems(&self) -> Vec<String> {
        let (failed, skipped) = (self.count(ItemStatus::Failed), self.count(ItemStatus::Skipped));
        let mut parts = Vec::new();
        if failed > 0 {
            parts.push(words::history::failed_count(failed));
        }
        if skipped > 0 {
            parts.push(words::history::skipped_count(skipped));
        }
        parts
    }

    pub fn has_problems(&self) -> bool {
        self.status == Status::Error || self.count(ItemStatus::Failed) + self.count(ItemStatus::Skipped) > 0
    }
}

/// The file operations run since Colet opened.
#[derive(Default)]
pub struct History {
    last_id: u64,
    /// Newest first.
    pub entries: Vec<HistoryEntry>,
    /// Operations with problems since the history was last opened.
    pub unseen: usize,
}

impl History {
    /// Keeps an operation just started at the top, running; its id.
    pub fn start(&mut self, request: Request) -> u64 {
        self.last_id += 1;
        let id = self.last_id;
        let entry = HistoryEntry {
            id,
            request,
            started: now(),
            status: Status::Running,
            error: None,
            items: Vec::new(),
            more: 0,
            more_failed: 0,
            more_skipped: 0,
        };
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_ENTRIES);
        id
    }

    pub fn entry_mut(&mut self, id: u64) -> Option<&mut HistoryEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Lets go of the operations that are over.
    pub fn clear(&mut self) {
        self.entries.retain(|e| e.status == Status::Running);
    }
}

pub(super) fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::{Event, HistoryEntry, ItemStatus, Kind, Request, Status};

    fn entry(kind: Kind) -> HistoryEntry {
        HistoryEntry {
            id: 1,
            request: Request { kind, paths: vec![r"D:\a.txt".into()], destination: r"D:\dest".into(), name: String::new() },
            started: 0,
            status: Status::Running,
            error: None,
            items: Vec::new(),
            more: 0,
            more_failed: 0,
            more_skipped: 0,
        }
    }

    #[test]
    fn records_where_a_paste_put_each_item_and_what_failed() {
        let mut paste = entry(Kind::Paste);
        paste.record(Event::Done { path: r"D:\a.txt".into() });
        paste.record(Event::Placed { path: r"D:\dest\a.txt".into() });
        paste.record(Event::Failed { path: r"D:\b.txt".into(), error: windows::Win32::Foundation::E_ACCESSDENIED });
        paste.record(Event::Skipped { path: r"D:\c.txt".into() });
        assert_eq!(paste.items[0].path, std::path::Path::new(r"D:\dest\a.txt"));
        assert_eq!(paste.items[0].from.as_deref(), Some(std::path::Path::new(r"D:\a.txt")));
        assert_eq!(paste.outcome(), "処理中");
        paste.status = Status::Done;
        assert_eq!(paste.outcome(), "1 件失敗・1 件スキップ");
        assert!(paste.has_problems());
        assert_eq!(paste.target(), "3 項目 → dest");
        assert_eq!(paste.count(ItemStatus::Done), 1);
    }

    #[test]
    fn counts_what_failed_or_was_skipped_past_the_items_it_keeps() {
        let mut paste = entry(Kind::Paste);
        for i in 0..super::MAX_ITEMS {
            paste.record(Event::Done { path: format!(r"D:\{i}.txt").into() });
        }
        paste.record(Event::Skipped { path: r"D:\skipped.txt".into() });
        paste.record(Event::Failed { path: r"D:\failed.txt".into(), error: windows::Win32::Foundation::E_ACCESSDENIED });
        assert_eq!((paste.items.len(), paste.more), (super::MAX_ITEMS, 2));
        paste.status = Status::Done;
        assert_eq!(paste.outcome(), "1 件失敗・1 件スキップ");
        assert!(paste.has_problems());
    }

    #[test]
    fn lists_what_was_handed_to_the_shell_by_its_name_without_a_problem() {
        let mut paste = entry(Kind::Paste);
        paste.record(Event::Handed { path: r"D:\dest\添付.txt".into() });
        assert_eq!((paste.items[0].path.to_str().unwrap(), paste.items[0].status), (r"D:\dest\添付.txt", ItemStatus::Handed));
        paste.status = Status::Done;
        assert!(!paste.has_problems());
        assert_eq!(paste.target(), "1 項目 → dest");
    }

    #[test]
    fn marks_what_went_to_the_bin_as_gone() {
        let mut recycle = entry(Kind::Recycle);
        recycle.record(Event::Done { path: r"D:\a.txt".into() });
        assert!(recycle.items[0].gone);
        recycle.status = Status::Done;
        assert_eq!(recycle.outcome(), "完了");
        assert_eq!(recycle.target(), "a.txt");
    }
}
