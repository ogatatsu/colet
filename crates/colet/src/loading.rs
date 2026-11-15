//! What the columns are listed from and kept fresh by: the listings out for each column, the search's
//! reloads for what Everything reports, the watch on the folders shown, and how the search engine
//! answers. Colet asks for listings and takes their answers through here, which tells a late answer
//! from the latest.
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use colet_core::everything::PAGE;

use crate::{columns::PAGE_AHEAD, state::ColumnId, watching::Watching};

#[derive(Default)]
pub struct Loading {
    last_request: u64,
    /// The columns listed at least once.
    listed: HashSet<ColumnId>,
    /// The latest listing still out for each column.
    pending: HashMap<ColumnId, u64>,
    /// What stops each folder's listing still out.
    cancels: HashMap<ColumnId, Arc<AtomicBool>>,
    pub search_refresh: SearchRefresh,
    pub watching: Watching,
    engine: Engine,
    /// A search was sent since the last was let go of: Everything watches it until it is.
    searched: bool,
}

impl Loading {
    // ---- The listings: each answer is told apart from those it was overtaken by ----

    /// True the first time `id` is asked about: it is to be listed.
    pub fn first(&mut self, id: ColumnId) -> bool {
        self.listed.insert(id)
    }

    /// A listing of `id` is out.
    pub fn is_loading(&self, id: ColumnId) -> bool {
        self.pending.contains_key(&id)
    }

    /// A listing of `id`, overtaking any still out: its number.
    pub fn start(&mut self, id: ColumnId) -> u64 {
        let request = self.next();
        self.pending.insert(id, request);
        request
    }

    /// A request number after every one before it.
    fn next(&mut self) -> u64 {
        self.last_request += 1;
        self.last_request
    }

    /// What stops the folder listing of `id` just started; the one before it is stopped.
    pub fn cancel_before(&mut self, id: ColumnId) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(previous) = self.cancels.insert(id, cancelled.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
        cancelled
    }

    /// The answer to `request` came: true when it is the latest for `id`, which is then no longer out.
    pub fn answered(&mut self, id: ColumnId, request: u64) -> bool {
        let current = self.pending.get(&id) == Some(&request);
        if current {
            self.pending.remove(&id);
        }
        current
    }

    /// Only the columns `live` holds: a column gone stops its listing, and a late answer finds no column
    /// and is dropped.
    pub fn keep(&mut self, live: &HashSet<ColumnId>) {
        self.cancels.retain(|id, cancelled| {
            let keep = live.contains(id);
            if !keep {
                cancelled.store(true, Ordering::Relaxed);
            }
            keep
        });
        self.listed.retain(|id| live.contains(id));
        self.pending.retain(|id, _| live.contains(id));
    }

    /// Lets go of every listing still out and the search's reload waiting, as another tab comes to the
    /// front.
    pub fn let_go(&mut self) {
        self.keep(&HashSet::new());
        self.search_refresh.cancel();
    }

    // ---- The search engine ----

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// A search is on its way: connecting, unless already connected.
    pub fn searching(&mut self) {
        self.searched = true;
        if !matches!(self.engine, Engine::Connected(_)) {
            self.engine = Engine::Connecting;
        }
    }

    /// No search is in front: the request number to let go of the last one sent with, so a search
    /// asked before it is not run; none when there is none to let go of.
    pub fn stop_searching(&mut self) -> Option<u64> {
        std::mem::take(&mut self.searched).then(|| self.next())
    }

    /// A search answered through `connection`, `current` when it is the latest for its column: one left
    /// behind still ends the wait for the connection.
    pub fn search_answered(&mut self, connection: Option<String>, current: bool) {
        if let Some(name) = connection.filter(|_| current || matches!(self.engine, Engine::Connecting)) {
            self.engine = Engine::Connected(name);
        }
    }

    /// The search engine could not be reached, or stopped answering.
    pub fn search_failed(&mut self, error: String) {
        self.engine = Engine::Failed(error);
    }
}

/// Whether a search has reached Everything; unknown until one has, since home needs none.
#[derive(Clone, Default)]
pub enum Engine {
    #[default]
    Unknown,
    /// A search is on its way while not connected, and may wait out a busy Everything.
    Connecting,
    Connected(String),
    Failed(String),
}

impl Engine {
    /// What the status bar shows, and what it says in full.
    pub fn describe(&self) -> (String, String) {
        match self {
            Engine::Unknown => ("Everything · 検索時に接続".into(), "検索エンジン: Everything（検索すると接続します）".into()),
            Engine::Connecting => ("Everything · 接続中…".into(), "検索エンジン: Everythingに接続しています".into()),
            Engine::Connected(name) => (name.clone(), format!("検索エンジン: {name}（接続済み）")),
            Engine::Failed(error) => ("Everything · 要確認".into(), error.clone()),
        }
    }
}

/// Reloads of the search for the changes Everything reports: one waiting at a time, and at most one every
/// couple of seconds.
#[derive(Default)]
pub struct SearchRefresh {
    /// The reload waiting, by its number; none when none is.
    waiting: Option<u64>,
    /// The number the next reload to wait takes.
    next: u64,
    /// When the last one went.
    last: Option<Instant>,
}

impl SearchRefresh {
    const EVERY: Duration = Duration::from_secs(2);

    /// A reload is to wait its turn: its number and how long until it may go, or none when one is already
    /// waiting.
    pub fn wait(&mut self) -> Option<(u64, Duration)> {
        if self.waiting.is_some() {
            return None;
        }
        let reload = self.next;
        self.next += 1;
        self.waiting = Some(reload);
        Some((reload, self.last.map_or(Duration::ZERO, |at| Self::EVERY.saturating_sub(at.elapsed()))))
    }

    /// Whether `reload` is still the one waiting: one cancelled since, as another tab came to the front,
    /// is not to go, even with another waiting after it.
    pub fn is_waiting(&self, reload: u64) -> bool {
        self.waiting == Some(reload)
    }

    /// The reload waiting went now.
    pub fn went(&mut self) {
        self.waiting = None;
        self.last = Some(Instant::now());
    }

    /// The reload waiting is no longer to go.
    pub fn cancel(&mut self) {
        self.waiting = None;
    }
}

/// How many of the `loaded` results a search reads again from the first: the `in_sight` rows and a little
/// past them, and up to `last_kept`, the last row picked or holding the keys, in whole pages. What it had
/// loaded below them is let go of and read again as the list nears its end, so a change Everything reports
/// does not read thousands over.
pub fn reread_count(loaded: usize, last_kept: Option<usize>, in_sight: usize) -> usize {
    // Room for results that come in above the last kept row, so it is not pushed past the end.
    let needed = in_sight.max(last_kept.map_or(0, |at| at + 1)) + PAGE_AHEAD;
    needed.div_ceil(PAGE).max(1).saturating_mul(PAGE).min(PAGE.max(loaded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reload_cancelled_while_it_waits_stays_cancelled_with_another_waiting() {
        let mut refresh = SearchRefresh::default();
        let (first, wait) = refresh.wait().unwrap();
        assert_eq!(wait, Duration::ZERO);
        assert!(refresh.wait().is_none());
        refresh.cancel();
        let (second, _) = refresh.wait().unwrap();
        assert!(!refresh.is_waiting(first));
        assert!(refresh.is_waiting(second));
        refresh.went();
        assert!(!refresh.is_waiting(second));
        // The next waits out the rest of the couple of seconds.
        assert!(refresh.wait().unwrap().1 > Duration::from_secs(1));
    }

    #[test]
    fn rereads_only_the_pages_in_sight_or_picked() {
        let loaded = PAGE * 20;
        // At the top: one page, however many were loaded.
        assert_eq!(reread_count(loaded, None, 30), PAGE);
        // Scrolled near a page's end: the next is read too, for what comes in above.
        assert_eq!(reread_count(loaded, None, PAGE * 3 - 10), PAGE * 4);
        // A row picked far down keeps every page up to it.
        assert_eq!(reread_count(loaded, Some(PAGE * 12), 30), PAGE * 13);
        // Never more than was loaded, and never less than a page.
        assert_eq!(reread_count(loaded, None, PAGE * 40), loaded);
        assert_eq!(reread_count(10, None, 0), PAGE);
    }
}
