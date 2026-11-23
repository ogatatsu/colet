//! What the columns are listed from: the listings out for each column, the search's reloads for what
//! Everything reports, and how the search engine answers. Its own entity: Colet says what to list, as a
//! [`Job`], and hears each answer as a [`LoadingEvent`], which tells a late answer from the latest.
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use colet_core::{
    entry::{Entry, Order},
    everything::PAGE,
    paths::{path_key, shown},
    shell::Owner,
    watch::Changes,
};
use gpui_kit::{Context, EventEmitter, Task};

use crate::words;
use crate::{
    app::Colet,
    backend::{self, Search},
    columns::PAGE_AHEAD,
    home::home_entries,
    notices::Tone,
    search_query::{everything_query, narrowed_query},
    settings::Settings,
    state::{ColumnId, Kind},
    wake::Woken,
};

/// How long what changes is gathered, from the first change told, before it is taken in together.
const GATHER: Duration = Duration::from_millis(200);

pub struct Loading {
    backend: backend::Shared,
    /// The window a folder's listing asks for a password over.
    owner: Owner,
    last_request: u64,
    /// The columns listed at least once.
    listed: HashSet<ColumnId>,
    /// The latest listing still out for each column.
    pending: HashMap<ColumnId, u64>,
    /// What stops each folder's listing still out.
    cancels: HashMap<ColumnId, Arc<AtomicBool>>,
    pub search_refresh: SearchRefresh,
    engine: Engine,
    /// A search was sent since the last was let go of: Everything watches it until it is.
    searched: bool,
    /// The request of the last search sent or let go of: what the watch on Everything tells is of it.
    search_request: u64,
    /// The answers come and not yet taken: each the latest for its column or one overtaken, which the
    /// columns tell apart by its request, as a search's warning is said even when it went stale. The
    /// engine has taken in what they say of Everything already.
    answers: Vec<(Asked, Listing)>,
}

/// What to list for a column.
pub enum Job {
    /// Home's rows, from the settings as they are and the drives.
    Home(Settings),
    /// A page of the search; narrowed, `whole` is the query without the narrowing, counted along with the
    /// first page.
    Search { search: Search, whole: Option<String> },
    /// A folder, whole, in `sort`.
    Folder { path: PathBuf, sort: Order },
}

/// What a listing was asked for: the column, from where, and the request it went out as.
#[derive(Clone, Copy)]
pub struct Asked {
    pub id: ColumnId,
    pub kind: Kind,
    pub request: u64,
    pub offset: usize,
}

/// What a listing came back with.
pub struct Listed {
    pub entries: Vec<Entry>,
    /// How many in all, loaded or not.
    pub total: usize,
    /// For a search, what to warn of the Everything that answered it, once for each connection to it.
    pub warning: Option<String>,
    /// For a narrowed search's first page, how many it finds without the filter.
    pub whole: Option<usize>,
    /// For a search, the Everything that answered it: the engine takes it in before Colet hears.
    connection: Option<String>,
}

impl Listed {
    /// A listing loaded whole at once, as home's and a folder's are.
    fn all(entries: Vec<Entry>) -> Self {
        Listed { total: entries.len(), entries, connection: None, warning: None, whole: None }
    }
}

pub type Listing = Result<Listed, String>;

pub enum LoadingEvent {
    /// Answers came, to [`take`](Loading::take_answers) without copying what they list.
    Answered,
}

impl EventEmitter<LoadingEvent> for Loading {}

impl Loading {
    pub fn new(backend: backend::Shared, owner: Owner) -> Self {
        Self {
            backend,
            owner,
            last_request: 0,
            listed: HashSet::new(),
            pending: HashMap::new(),
            cancels: HashMap::new(),
            search_refresh: SearchRefresh::default(),
            engine: Engine::default(),
            searched: false,
            search_request: 0,
            answers: Vec::new(),
        }
    }

    /// The answers come since last taken, in the order they came.
    pub fn take_answers(&mut self) -> Vec<(Asked, Listing)> {
        std::mem::take(&mut self.answers)
    }

    // ---- The listings: each answer is told apart from those it was overtaken by ----

    /// True the first time `id` is asked about: it is to be listed.
    pub fn first(&mut self, id: ColumnId) -> bool {
        self.listed.insert(id)
    }

    /// A listing of `id` is out.
    pub fn is_loading(&self, id: ColumnId) -> bool {
        self.pending.contains_key(&id)
    }

    /// A listing of `id`, overtaking any still out: its number, to [`run`](Self::run) it by.
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

    /// Lists `job` off the window's thread, as [`start`](Self::start) numbered it, and tells its answer.
    pub fn run(&mut self, asked: Asked, job: Job, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task: Task<Listing> = match job {
            Job::Home(settings) => cx.background_executor().spawn(async move { Ok(Listed::all(home_entries(&settings, backend.roots()))) }),
            Job::Search { search, whole } => {
                self.searching(asked.request);
                cx.background_executor().spawn(async move {
                    let found = backend.search(search).map_err(|error| words::search::error(&error))?;
                    let found = found.ok_or_else(|| words::search::stale().to_string())?;
                    let whole = whole.and_then(|query| backend.count(&query).ok());
                    Ok(Listed {
                        entries: found.page.entries,
                        total: found.page.total,
                        connection: found.connection.as_ref().map(words::status::connection),
                        warning: found.warning.as_ref().map(words::search::slow),
                        whole,
                    })
                })
            }
            Job::Folder { path, sort } => {
                let (cancelled, owner) = (self.cancel_before(asked.id), self.owner);
                cx.background_executor().spawn(async move {
                    let entries = backend.list(&path, sort, &cancelled, owner).map_err(|error| words::column::list_error(&error))?;
                    Ok(Listed::all(entries))
                })
            }
        };
        cx.spawn(async move |this, cx| {
            let listing = task.await;
            this.update(cx, |this, cx| this.listed(asked, listing, cx)).ok();
        })
        .detach();
    }

    /// The answer to `asked` came: the engine takes in what it says of Everything, and Colet hears it.
    fn listed(&mut self, asked: Asked, mut listing: Listing, cx: &mut Context<Self>) {
        let current = self.answered(asked.id, asked.request);
        match &mut listing {
            Ok(listed) => self.search_answered(listed.connection.take(), current),
            Err(error) if asked.kind == Kind::Search && current => self.search_failed(error.clone()),
            Err(_) => {}
        }
        self.answers.push((asked, listing));
        cx.emit(LoadingEvent::Answered);
        cx.notify();
    }

    /// What stops the folder listing of `id` just started; the one before it is stopped.
    fn cancel_before(&mut self, id: ColumnId) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(previous) = self.cancels.insert(id, cancelled.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
        cancelled
    }

    /// The answer to `request` came: true when it is the latest for `id`, which is then no longer out.
    fn answered(&mut self, id: ColumnId, request: u64) -> bool {
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

    /// A search is on its way as `request`: connecting, unless already connected.
    fn searching(&mut self, request: u64) {
        self.searched = true;
        self.search_request = request;
        if !matches!(self.engine, Engine::Connected(_)) {
            self.engine = Engine::Connecting;
        }
    }

    /// No search is in front: Everything stops watching the last one sent, and a search asked before
    /// this is not run. It comes back with the next.
    pub fn stop_searching(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.searched) {
            return;
        }
        let request = self.next();
        self.search_request = request;
        let backend = self.backend.clone();
        cx.background_executor().spawn(async move { backend.stop_search(request) }).detach();
    }

    /// A search answered through `connection`, `current` when it is the latest for its column: one left
    /// behind still ends the wait for the connection.
    fn search_answered(&mut self, connection: Option<String>, current: bool) {
        if let Some(name) = connection.filter(|_| current || matches!(self.engine, Engine::Connecting)) {
            self.engine = Engine::Connected(name);
        }
    }

    /// Which search the watch on Everything is of: a number that changes with each sent or let go of.
    pub fn search_request(&self) -> u64 {
        self.search_request
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
            Engine::Unknown => (words::status::engine_unknown().into(), words::status::engine_unknown_tip().into()),
            Engine::Connecting => (words::status::engine_connecting().into(), words::status::engine_connecting_tip().into()),
            Engine::Connected(name) => (name.clone(), words::status::engine_connected(name)),
            Engine::Failed(error) => (words::status::engine_failed().into(), error.clone()),
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

// ---- What Colet does with them: asks for the listings, takes their answers, and follows the watches ----

impl Colet {
    /// Lists the column `id` from `offset` on; `quiet` keeps what it shows meanwhile.
    pub fn load(&mut self, id: ColumnId, offset: usize, quiet: bool, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        if offset > 0 && self.loading.read(cx).is_loading(id) {
            return;
        }
        let request = self.loading.update(cx, |loading, _| loading.start(id));
        self.state.load(id, request, quiet);
        let c = &self.state.columns[index];
        let sort = self.order_of(c);
        let job = match c.kind {
            Kind::Home => Job::Home(self.stored.settings().clone()),
            Kind::Search => {
                let filter = self.state.search_filter.as_deref().and_then(|name| self.stored.settings().filter_query(name));
                let whole = everything_query(&self.state.query, filter);
                // Narrowed, the search is counted without it too, along with the first page.
                let (query, whole) = match &c.narrow {
                    Some(filter) => (narrowed_query(&whole, filter.text()), (offset == 0).then_some(whole)),
                    None => (whole, None),
                };
                // A refresh reloads the pages in sight and those the picked rows and the keys are on, so
                // they stay; the rest load again as the list nears its end.
                let count = if offset > 0 {
                    PAGE
                } else {
                    let in_sight = crate::columns::rows_in_sight(self.scrolls.rows.handle(id), c.entries.len()).unwrap_or(0);
                    let keep: HashSet<&Path> = c.selected.iter().chain(&c.focused).chain(&c.highlight).map(PathBuf::as_path).collect();
                    let last_kept = c.entries.iter().rposition(|e| keep.contains(e.path.as_path()));
                    reread_count(c.entries.len(), last_kept, in_sight)
                };
                Job::Search { search: Search { query, sort, offset, count, refresh: offset == 0, request }, whole }
            }
            Kind::Folder => Job::Folder { path: c.path.clone(), sort },
        };
        let asked = Asked { id, kind: c.kind, request, offset };
        self.loading.update(cx, |loading, cx| loading.run(asked, job, cx));
    }

    /// What the listings tell.
    pub(crate) fn on_loading_event(&mut self, event: &LoadingEvent, cx: &mut Context<Self>) {
        match event {
            LoadingEvent::Answered => {
                for (asked, listing) in self.loading.update(cx, |loading, _| loading.take_answers()) {
                    self.listed(asked, listing, cx);
                }
            }
        }
    }

    /// The answer to `asked` came.
    fn listed(&mut self, asked: Asked, listing: Listing, cx: &mut Context<Self>) {
        let Asked { id, request, offset, .. } = asked;
        match listing {
            Ok(Listed { entries, total, warning, whole, .. }) => {
                // Told even of a search gone stale: it is the connection's, said once.
                if let Some(warning) = warning {
                    self.notify(warning, Tone::Warning, cx);
                }
                if let Some(whole) = whole {
                    self.state.counted(id, request, whole);
                }
                self.state.page(id, request, entries, total, offset, self.operations.read(cx).departed());
                // What the listing showed held in another case than the disk's, as a favorite kept from
                // before a rename elsewhere, is kept in the disk's from now on.
                for (from, to) in std::mem::take(&mut self.state.respelled) {
                    self.follow_move(&from, &to, cx);
                }
            }
            Err(error) => self.state.error(id, request, error),
        }
        self.changed(cx);
    }

    /// The tab come to the front lists all its columns again: what it showed stays meanwhile, and what it
    /// never listed shows as loading. What the tab before it still had out is let go of.
    pub(crate) fn relist(&mut self, cx: &mut Context<Self>) {
        let columns: Vec<(ColumnId, bool)> = self.state.columns.iter().map(|c| (c.id, c.request != 0)).collect();
        self.loading.update(cx, |loading, _| {
            loading.let_go();
            for (id, _) in &columns {
                loading.first(*id);
            }
        });
        for (id, listed) in columns {
            self.load(id, 0, listed, cx);
        }
    }

    /// Reloads every column, keeping what each shows meanwhile.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<ColumnId> = self.state.columns.iter().map(|c| c.id).collect();
        for id in ids {
            self.load(id, 0, true, cx);
        }
        cx.notify();
    }

    /// The search results load their next page once the list nears its end.
    pub fn more(&mut self, id: ColumnId, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        let c = &self.state.columns[index];
        if c.kind == Kind::Search && c.error.is_none() && !c.loading && c.entries.len() < c.total {
            let offset = c.entries.len();
            self.load(id, offset, true, cx);
        }
    }

    // ---- Watching ----

    /// Watches the folders the columns could read, one level deep, as [`Watching::set`] does; a server's
    /// shares, which no change tells of, are read again with F5 only.
    ///
    /// [`Watching::set`]: crate::watching::Watching::set
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) -> Option<mpsc::Receiver<()>> {
        let paths: Vec<PathBuf> = self
            .state
            .columns
            .iter()
            .filter(|c| c.kind == Kind::Folder && c.error.is_none() && !colet_core::paths::is_server(&c.path))
            .map(|c| c.path.clone())
            .collect();
        self.watching.set(paths, &self.wake).unwrap_or_else(|error| {
            self.notify(words::window::cannot_start_watching(&error), Tone::Error, cx);
            None
        })
    }

    /// The tab in front searches no more, as another without a search came to the front, or its search
    /// was cleared or closed: Everything stops watching the last search. It comes back with the next.
    pub(crate) fn stop_search_out_of_sight(&mut self, cx: &mut Context<Self>) {
        if self.searches() {
            return;
        }
        self.loading.update(cx, |loading, cx| loading.stop_searching(cx));
    }

    /// The tab in front has a search.
    fn searches(&self) -> bool {
        self.state.columns.iter().any(|c| c.kind == Kind::Search)
    }

    /// Says which folders the watcher could list but not watch, each once.
    fn take_missed(&mut self, cx: &mut Context<Self>) {
        let fresh = self.watching.take_missed();
        if !fresh.is_empty() {
            let names: String = fresh.iter().map(|p| words::quoted(shown(colet_core::paths::folder_name(p)))).collect();
            self.notify(words::window::cannot_watch(names), Tone::Error, cx);
        }
    }

    /// Takes in what changed whenever `woken` is told, and sleeps in between: folder changes reload the
    /// columns showing them, and what the watch on Everything heard of the results. What comes within
    /// [`GATHER`] of the first is taken in with it, so a burst reloads once.
    pub(crate) fn listen(&mut self, woken: Woken, cx: &mut Context<Self>) {
        let changed = self.watching.changed.clone();
        let backend = self.backend.clone();
        cx.spawn(async move |this, cx| {
            // Asked again without being told, while a search held the watch on it.
            let mut ask_again = false;
            loop {
                if !ask_again && !woken.told().await {
                    return;
                }
                cx.background_executor().timer(GATHER).await;
                if this.update(cx, |this, cx| this.take_changes(cx)).is_err() {
                    return;
                }
                let batch = changed.lock().unwrap_or_else(|e| e.into_inner()).take();
                if batch != Changes::Nothing && this.update(cx, |this, cx| this.files_changed(batch, cx)).is_err() {
                    return;
                }
                let Ok(asked) = this.update(cx, |this, cx| this.loading.read(cx).search_request()) else {
                    return;
                };
                let backend = backend.clone();
                let result = cx.background_executor().spawn(async move { backend.search_changed() }).await;
                ask_again = matches!(result, Ok(None));
                let alive = this
                    .update(cx, |this, cx| match result {
                        Ok(Some(true)) => this.search_changed(cx),
                        Ok(Some(false) | None) => {}
                        // Heard of a search no longer in front, as it was let go of, or of one another was
                        // sent after while it was asked: the next search connects afresh if it must, and
                        // says itself what failed.
                        Err(_) if !this.searches() || this.loading.read(cx).search_request() != asked => {}
                        Err(error) => {
                            let error = words::search::error(&error);
                            this.loading.update(cx, |loading, cx| {
                                loading.search_failed(error.clone());
                                cx.notify();
                            });
                            this.notify(error, Tone::Error, cx);
                        }
                    })
                    .is_ok();
                if !alive {
                    return;
                }
            }
        })
        .detach();
    }

    /// The folders that went unwatched, how the running operation's items ended so far, and what the
    /// clipboard now holds.
    fn take_changes(&mut self, cx: &mut Context<Self>) {
        self.take_missed(cx);
        self.operations.update(cx, |operations, cx| operations.take_changes(cx));
    }

    /// Reloads the folder columns whose listings changed.
    fn files_changed(&mut self, changes: Changes, cx: &mut Context<Self>) {
        let paths = match changes {
            Changes::Nothing => return,
            Changes::Paths(paths) => paths,
            Changes::All => {
                self.refresh(cx);
                return;
            }
        };
        let folders: HashSet<OsString> = paths.iter().map(|path| path_key(path)).collect();
        let ids: Vec<ColumnId> =
            self.state.columns.iter().filter(|c| c.kind == Kind::Folder && folders.contains(&path_key(&c.path))).map(|c| c.id).collect();
        for id in ids {
            self.load(id, 0, true, cx);
        }
    }

    /// A busy disk reports changes each second: the first reloads at once, then at most one every couple
    /// of seconds. One met by a reload in flight waits for it, as it may have read the results before.
    fn search_changed(&mut self, cx: &mut Context<Self>) {
        let Some((reload, wait)) = self.loading.update(cx, |loading, _| loading.search_refresh.wait()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            loop {
                let busy = this
                    .update(cx, |this, cx| {
                        // Cancelled while it waited: the search in front now is another tab's, read afresh
                        // as it came to the front.
                        if !this.loading.read(cx).search_refresh.is_waiting(reload) {
                            return false;
                        }
                        let Some(search) = this.state.columns.iter().find(|c| c.kind == Kind::Search).map(|c| c.id) else {
                            this.loading.update(cx, |loading, _| loading.search_refresh.cancel());
                            return false;
                        };
                        if this.loading.read(cx).is_loading(search) {
                            return true;
                        }
                        this.loading.update(cx, |loading, _| loading.search_refresh.went());
                        this.load(search, 0, true, cx);
                        false
                    })
                    .unwrap_or(false);
                if !busy {
                    return;
                }
                cx.background_executor().timer(Duration::from_millis(200)).await;
            }
        })
        .detach();
    }
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
