//! Searching through Everything 1.5's SDK, which is compiled in from `vendor/everything-sdk3`.
use std::{
    ffi::c_void,
    path::PathBuf,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::{
    entry::{Entry, Hiding, Marks, Order, Sort, marked},
    paths::{from_wide, shown},
    wide::Wide,
};

type Handle = *mut c_void;

#[link(name = "everything3", kind = "static")]
unsafe extern "system" {
    fn Everything3_ConnectW(instance: *const u16) -> Handle;
    fn Everything3_DestroyClient(client: Handle) -> i32;
    fn Everything3_ShutdownClient(client: Handle) -> i32;
    fn Everything3_IsDBLoaded(client: Handle) -> i32;
    fn Everything3_GetMajorVersion(client: Handle) -> u32;
    fn Everything3_GetMinorVersion(client: Handle) -> u32;
    fn Everything3_GetRevision(client: Handle) -> u32;
    fn Everything3_GetBuildNumber(client: Handle) -> u32;
    fn Everything3_GetLastError() -> u32;
    fn Everything3_CreateSearchState() -> Handle;
    fn Everything3_DestroySearchState(state: Handle) -> i32;
    fn Everything3_SetSearchTextW(state: Handle, text: *const u16) -> i32;
    fn Everything3_SetSearchViewportOffset(state: Handle, offset: usize) -> i32;
    fn Everything3_SetSearchViewportCount(state: Handle, count: usize) -> i32;
    fn Everything3_AddSearchPropertyRequest(state: Handle, property: u32) -> i32;
    fn Everything3_AddSearchPropertyRequestHighlighted(state: Handle, property: u32) -> i32;
    fn Everything3_AddSearchSort(state: Handle, property: u32, ascending: i32) -> i32;
    fn Everything3_SetSearchFoldersFirst(state: Handle, mode: u32) -> i32;
    fn Everything3_Search(client: Handle, state: Handle) -> Handle;
    fn Everything3_GetResults(client: Handle, state: Handle) -> Handle;
    fn Everything3_DestroyResultList(result: Handle) -> i32;
    fn Everything3_GetResultListCount(result: Handle) -> usize;
    fn Everything3_GetResultListViewportCount(result: Handle) -> usize;
    fn Everything3_GetResultFullPathNameW(result: Handle, index: usize, buf: *mut u16, size: usize) -> usize;
    fn Everything3_IsFolderResult(result: Handle, index: usize) -> i32;
    fn Everything3_GetResultSize(result: Handle, index: usize) -> u64;
    fn Everything3_GetResultDateModified(result: Handle, index: usize) -> u64;
    fn Everything3_GetResultPropertyTextHighlightedW(result: Handle, index: usize, property: u32, buf: *mut u16, size: usize) -> usize;
    fn Everything3_WaitForResultListChange(client: Handle) -> i32;
}

// The properties it asks for, as `EVERYTHING3_PROPERTY_ID_*`.
const NAME: u32 = 0;
const PATH: u32 = 1;
const SIZE: u32 = 2;
const DATE_MODIFIED: u32 = 5;
const FULL_PATH: u32 = 240;

/// `EVERYTHING3_SEARCH_FOLDERS_FIRST_ALWAYS`.
const FOLDERS_FIRST_ALWAYS: u32 = 1;

/// How many results a page holds.
pub const PAGE: usize = 500;

const WATCH_RETRY: Duration = Duration::from_secs(5);

/// The oldest Everything it asks, 1.5.0.1424b: the one that names its pipes by session.
const OLDEST: [u32; 4] = [1, 5, 0, 1424];

/// A run of results: `entries` from `offset` on, of `total` in all.
#[derive(Debug)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub total: usize,
    pub offset: usize,
}

/// Connect to the unnamed instance, or to `instance` when given. The SDK waits out a busy pipe itself,
/// for up to 30 seconds.
unsafe fn open(instance: Option<&str>) -> Handle {
    let name = instance.map(Wide::new);
    unsafe { Everything3_ConnectW(name.as_ref().map_or(ptr::null(), |n| n.get().0)) }
}

/// The most index roots it reads.
const ROOTS: usize = 1000;

/// The folders Everything's index starts from (`root:`): its volumes, as `C:`, and the folders and shares
/// added to it. Asked on a client of its own and let go, so the last search and its watch are left alone.
pub fn roots() -> Result<Vec<PathBuf>, String> {
    unsafe {
        let client = open(None);
        if client.is_null() {
            return Err("Everything 1.5に接続できません".into());
        }
        let roots = indexed_roots(client);
        Everything3_DestroyClient(client);
        roots
    }
}

/// How many results `query` finds, none of them read. Asked on a client of its own, as [`roots`] are.
pub fn count(query: &str) -> Result<usize, String> {
    if query.contains('\0') {
        return Err("入力に使用できない文字が含まれています".into());
    }
    unsafe {
        let client = open(None);
        if client.is_null() {
            return Err("Everything 1.5に接続できません".into());
        }
        let state = Everything3_CreateSearchState();
        let count = if state.is_null() {
            Err("検索状態を作成できません".into())
        } else {
            Everything3_SetSearchTextW(state, Wide::new(query).get().0);
            Everything3_SetSearchViewportCount(state, 0);
            let result = Everything3_Search(client, state);
            let count = if result.is_null() {
                Err(format!("Everything検索に失敗しました (0x{:08X})", Everything3_GetLastError()))
            } else {
                let total = Everything3_GetResultListCount(result);
                Everything3_DestroyResultList(result);
                Ok(total)
            };
            Everything3_DestroySearchState(state);
            count
        };
        Everything3_DestroyClient(client);
        count
    }
}

unsafe fn indexed_roots(client: Handle) -> Result<Vec<PathBuf>, String> {
    unsafe {
        if Everything3_IsDBLoaded(client) == 0 {
            return Err("Everythingの索引を準備中です".into());
        }
        let state = Everything3_CreateSearchState();
        if state.is_null() {
            return Err("検索状態を作成できません".into());
        }
        Everything3_SetSearchTextW(state, Wide::new("root:").get().0);
        Everything3_AddSearchPropertyRequest(state, FULL_PATH);
        Everything3_SetSearchViewportCount(state, ROOTS);
        let result = Everything3_Search(client, state);
        let roots = if result.is_null() {
            Err(format!("Everything検索に失敗しました (0x{:08X})", Everything3_GetLastError()))
        } else {
            let count = Everything3_GetResultListViewportCount(result);
            let roots = (0..count)
                .map(|index| from_wide(&wide(|buf, size| Everything3_GetResultFullPathNameW(result, index, buf, size))))
                .collect();
            Everything3_DestroyResultList(result);
            Ok(roots)
        };
        Everything3_DestroySearchState(state);
        roots
    }
}

/// What a watch has heard: that the results changed, and the error it stopped on (0 for none).
#[derive(Default)]
struct News {
    changed: AtomicBool,
    failed: AtomicU32,
    /// Set before the watch is woken on purpose, so the shutdown it wakes to is no failure.
    stopping: AtomicBool,
}

/// A client of its own, given the search and waiting for its results to change on a thread, so
/// paging through them on the other client cannot clear the change.
struct Watch {
    client: Handle,
    news: Arc<News>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Gives `client`, which it then owns, the search in `state`, and waits on it; `None` if the
    /// search fails, having let go of `client`.
    unsafe fn start(client: Handle, state: Handle) -> Option<Self> {
        unsafe {
            Everything3_SetSearchViewportOffset(state, 0);
            Everything3_SetSearchViewportCount(state, 1);
            let watched = Everything3_Search(client, state);
            if watched.is_null() {
                Everything3_DestroyClient(client);
                return None;
            }
            Everything3_DestroyResultList(watched);
        }
        let news = Arc::new(News::default());
        let heard = news.clone();
        let address = client as usize;
        let thread = std::thread::spawn(move || {
            let client = address as Handle;
            loop {
                // Holds the client's lock until a change comes, or the client is shut down.
                if unsafe { Everything3_WaitForResultListChange(client) } != 0 {
                    // Nothing clears the change on this client, so waiting again would return at once,
                    // over and over: it stays heard until a fresh search starts another watch.
                    heard.changed.store(true, Ordering::Relaxed);
                    return;
                }
                let error = unsafe { Everything3_GetLastError() };
                if error == 0 {
                    continue;
                }
                if !heard.stopping.load(Ordering::Relaxed) {
                    heard.failed.store(error, Ordering::Relaxed);
                }
                return;
            }
        });
        Some(Self { client, news, thread: Some(thread) })
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.news.stopping.store(true, Ordering::Relaxed);
        unsafe {
            Everything3_ShutdownClient(self.client);
        }
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
        unsafe {
            Everything3_DestroyClient(self.client);
        }
    }
}

/// The last search and its pages, and the watch on its results.
pub struct SearchSession {
    client: Handle,
    watch: Option<Watch>,
    state: Handle,
    key: Option<(String, Order)>,
    /// The connected Everything's, as 1.5.0.1424.
    version: String,
    /// The instance the watch connects to, named to make it fail.
    #[cfg(test)]
    watch_instance: Option<String>,
    watch_retry_at: Option<Instant>,
    /// What it is connected to, as the status bar names it.
    pub connection: String,
}

// Used from one thread at a time, behind the app's lock.
unsafe impl Send for SearchSession {}

impl Default for SearchSession {
    fn default() -> Self {
        Self {
            client: ptr::null_mut(),
            watch: None,
            state: ptr::null_mut(),
            key: None,
            version: String::new(),
            watch_retry_at: None,
            connection: String::new(),
            #[cfg(test)]
            watch_instance: None,
        }
    }
}

impl Drop for SearchSession {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl SearchSession {
    pub fn disconnect(&mut self) {
        self.watch = None;
        unsafe {
            if !self.state.is_null() {
                Everything3_DestroySearchState(self.state);
                self.state = ptr::null_mut();
            }
            if !self.client.is_null() {
                Everything3_DestroyClient(self.client);
                self.client = ptr::null_mut();
            }
        }
        self.key = None;
        self.watch_retry_at = None;
    }

    /// Whether Everything has said the last search's results changed; it stays so until searched afresh.
    pub fn changed(&mut self) -> Result<bool, String> {
        let Some(watch) = self.watch.as_ref().filter(|_| self.key.is_some()) else { return Ok(false) };
        let error = watch.news.failed.load(Ordering::Relaxed);
        if error != 0 {
            // A stopped Everything breaks both pipes. Discard them together so the next search
            // connects afresh; a watch-only failure keeps the usable paging connection.
            if unsafe { Everything3_IsDBLoaded(self.client) == 0 && Everything3_GetLastError() != 0 } {
                self.disconnect();
                return Err(format!("Everythingとの接続が切れました。次の検索で再接続します (0x{error:08X})"));
            }
            self.stop_watching();
            return Err(format!("Everythingの自動更新を停止しました。F5で一覧を更新できます (0x{error:08X})"));
        }
        Ok(watch.news.changed.load(Ordering::Relaxed))
    }

    /// `count` results of `query` from `offset` on. `refresh` searches afresh even when the query and
    /// sort are the ones last run.
    pub fn search(&mut self, query: &str, order: Order, offset: usize, count: usize, refresh: bool) -> Result<Page, String> {
        if query.contains('\0') {
            return Err("入力に使用できない文字が含まれています".into());
        }
        unsafe {
            if self.client.is_null() {
                self.connect()?;
            }
            if Everything3_IsDBLoaded(self.client) == 0 {
                if Everything3_GetLastError() != 0 {
                    self.disconnect();
                    return Err("Everythingとの接続が切れました。再接続してください。".into());
                }
                return Err("Everythingの索引を準備中です。しばらく待って再試行してください。".into());
            }
            let key = (query.to_owned(), order);
            let new_search = self.key.as_ref() != Some(&key) || refresh || self.state.is_null();
            if new_search {
                self.prepare(query, order)?;
                self.start_watching();
            }
            // These setters update the state but return FALSE in the pinned upstream version.
            Everything3_SetSearchViewportOffset(self.state, offset);
            Everything3_SetSearchViewportCount(self.state, count);
            let result =
                if new_search { Everything3_Search(self.client, self.state) } else { Everything3_GetResults(self.client, self.state) };
            if result.is_null() {
                let error = Everything3_GetLastError();
                self.disconnect();
                return Err(format!("Everything検索に失敗しました (0x{error:08X})"));
            }
            self.key = Some(key);
            let page = read(result, offset);
            Everything3_DestroyResultList(result);
            Ok(page)
        }
    }

    /// Connects to the unnamed instance, turning away an Everything older than [`OLDEST`].
    unsafe fn connect(&mut self) -> Result<(), String> {
        let client = unsafe { open(None) };
        if client.is_null() {
            return Err("Everything 1.5に接続できません。本体が起動しているか確認して、再試行してください。".into());
        }
        let version = unsafe {
            [
                Everything3_GetMajorVersion(client),
                Everything3_GetMinorVersion(client),
                Everything3_GetRevision(client),
                Everything3_GetBuildNumber(client),
            ]
        };
        let dotted = |v: [u32; 4]| v.map(|n| n.to_string()).join(".");
        if version < OLDEST {
            unsafe {
                Everything3_DestroyClient(client);
            }
            return Err(format!(
                "Everything {}以降が必要です（接続先: {}）。更新してから再試行してください。",
                dotted(OLDEST),
                dotted(version)
            ));
        }
        self.client = client;
        self.version = dotted(version);
        Ok(())
    }

    /// Watches the new search on a client of its own, as the wait cannot be told of another search
    /// but only shut down, and a shut-down client is spent. One that failed is tried again at most once
    /// per interval, on a new query or full refresh. It starts before the results are read, so none of
    /// their changes is missed.
    unsafe fn start_watching(&mut self) {
        let failed = self.watch.is_none();
        self.watch = None;
        if failed && self.watch_retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        #[cfg(test)]
        let instance = self.watch_instance.as_deref();
        #[cfg(not(test))]
        let instance = None;
        let client = unsafe { open(instance) };
        self.watch = if client.is_null() { None } else { unsafe { Watch::start(client, self.state) } };
        if self.watch.is_some() {
            self.watch_retry_at = None;
            self.name_connection();
        } else {
            self.stop_watching();
        }
    }

    fn stop_watching(&mut self) {
        self.watch = None;
        self.watch_retry_at = Some(Instant::now() + WATCH_RETRY);
        self.name_connection();
    }

    fn name_connection(&mut self) {
        self.connection = format!("Everything {}", self.version);
        if self.watch.is_none() {
            self.connection.push_str(" · 手動更新（F5）");
        }
    }

    unsafe fn prepare(&mut self, query: &str, order: Order) -> Result<(), String> {
        unsafe {
            if !self.state.is_null() {
                Everything3_DestroySearchState(self.state);
            }
            self.state = Everything3_CreateSearchState();
            if self.state.is_null() {
                return Err("検索状態を作成できません".into());
            }
            Everything3_SetSearchTextW(self.state, Wide::new(query).get().0);
            // Not attributes: Everything rarely indexes them, and reads each result's from the disk
            // instead, stalling the search as it is typed.
            for property in [FULL_PATH, SIZE, DATE_MODIFIED] {
                Everything3_AddSearchPropertyRequest(self.state, property);
            }
            // Where the search matched, to mark in the name and in the folder it is in.
            for property in [NAME, PATH] {
                Everything3_AddSearchPropertyRequestHighlighted(self.state, property);
            }
            // Folders first descending too, as the columns order them, unlike Explorer.
            Everything3_SetSearchFoldersFirst(self.state, FOLDERS_FIRST_ALWAYS);
            let property = match order.by {
                Sort::Name => NAME,
                Sort::Size => SIZE,
                Sort::Modified => DATE_MODIFIED,
            };
            Everything3_AddSearchSort(self.state, property, i32::from(!order.descending));
            if order.by != Sort::Name {
                Everything3_AddSearchSort(self.state, NAME, 1);
            }
        }
        Ok(())
    }
}

/// A result's text as UTF-16, which the SDK writes into a buffer it first says the length of, up to its
/// NUL.
unsafe fn wide(write: impl Fn(*mut u16, usize) -> usize) -> Vec<u16> {
    let length = write(ptr::null_mut(), 0);
    let mut buffer = vec![0u16; length + 1];
    write(buffer.as_mut_ptr(), buffer.len());
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    buffer.truncate(end);
    buffer
}

/// A result's text, for what is shown.
unsafe fn text(write: impl Fn(*mut u16, usize) -> usize) -> String {
    String::from_utf16_lossy(&unsafe { wide(write) })
}

unsafe fn read(result: Handle, offset: usize) -> Page {
    unsafe {
        let total = Everything3_GetResultListCount(result);
        let count = Everything3_GetResultListViewportCount(result);
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let path = from_wide(&wide(|buf, size| Everything3_GetResultFullPathNameW(result, index, buf, size)));
            let highlighted =
                |property| text(|buf, size| Everything3_GetResultPropertyTextHighlightedW(result, index, property, buf, size));
            let directory = Everything3_IsFolderResult(result, index) != 0;
            let size = Everything3_GetResultSize(result, index);
            let modified = Everything3_GetResultDateModified(result, index);
            let p = path.as_path();
            let name = p.file_name().map(shown).unwrap_or_else(|| shown(p));
            let parent = p.parent().map(shown).unwrap_or_default();
            let marks = Marks { name: marked(&highlighted(NAME), &name), parent: marked(&highlighted(PATH), &parent) };
            entries.push(Entry {
                name,
                parent,
                directory,
                size: (!directory && size != u64::MAX).then_some(size),
                // FILETIME to Unix milliseconds.
                modified: (modified != u64::MAX).then(|| modified.checked_sub(116_444_736_000_000_000).map(|v| v / 10_000)).flatten(),
                hidden: Hiding::Shown,
                marks,
                path,
            });
        }
        Page { entries, total, offset }
    }
}

#[cfg(test)]
// A mark is a list of ranges, most often holding only one.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// One test at a time talks to Everything: it turns away a connection made as another's pipe is
    /// being replaced, and the app never connects twice at once.
    static EVERYTHING: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn alone() -> std::sync::MutexGuard<'static, ()> {
        EVERYTHING.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The error `changed` reports once the watch thread hears its client fail.
    fn failure(session: &mut SearchSession) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match session.changed() {
                Err(error) => return error,
                Ok(_) => assert!(Instant::now() < deadline, "the watch did not report the failure"),
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "needs a running Everything 1.5"]
    fn names_the_index_roots_with_the_volumes_as_letters() {
        let _alone = alone();
        let roots = roots().unwrap();
        assert!(roots.iter().any(|r| r.as_os_str().len() == 2 && r.as_os_str().as_encoded_bytes().ends_with(b":")), "{roots:?}");
    }

    #[test]
    #[ignore = "needs a running Everything 1.5"]
    fn reconnects_on_the_first_search_after_both_connections_fail() {
        let _alone = alone();
        let query = "parent:\"C:\\colet-sdk-reconnect-probe\"";
        let mut session = SearchSession::default();
        session.search(query, Order::default(), 0, PAGE, true).unwrap();
        // Named by its version, which the session pipes made the oldest asked.
        assert!(session.connection.starts_with("Everything 1.5.0."), "{}", session.connection);
        // Shut down only this session's handles, reproducing failed IPC on both connections without
        // stopping the user's Everything process.
        unsafe {
            assert_ne!(Everything3_ShutdownClient(session.client), 0);
            assert_ne!(Everything3_ShutdownClient(session.watch.as_ref().unwrap().client), 0);
        }
        let error = failure(&mut session);
        assert!(error.contains("接続が切れました"));
        assert!(session.client.is_null() && session.watch.is_none());
        assert!(session.key.is_none() && session.watch_retry_at.is_none());
        session.search(query, Order::default(), 0, PAGE, true).unwrap();
        assert!(!session.client.is_null() && session.watch.is_some());
    }

    #[test]
    #[ignore = "needs a running Everything 1.5"]
    fn a_watch_only_failure_preserves_the_paging_connection() {
        let _alone = alone();
        let query = "parent:\"C:\\colet-sdk-watch-failure-probe\"";
        let mut session = SearchSession::default();
        session.search(query, Order::default(), 0, PAGE, true).unwrap();
        let client = session.client;
        unsafe {
            assert_ne!(Everything3_ShutdownClient(session.watch.as_ref().unwrap().client), 0);
        }
        let error = failure(&mut session);
        assert!(error.contains("自動更新を停止しました"));
        assert_eq!(session.client, client);
        assert!(session.watch.is_none());
        assert!(session.key.is_some() && session.watch_retry_at.is_some());
        session.search(query, Order::default(), 0, PAGE, true).unwrap();
        assert_eq!(session.client, client);
        assert!(session.watch.is_none());
        assert!(!session.changed().unwrap());
    }

    #[test]
    #[ignore = "needs a running Everything 1.5 that indexes the temporary folder"]
    fn a_failed_watch_keeps_search_and_pagination_usable_and_retries_on_refresh() {
        let _alone = alone();
        let fixture = tempfile::Builder::new().prefix("colet-sdk-watch-").tempdir().unwrap();
        for i in 0..511 {
            std::fs::write(fixture.path().join(format!("{i:04}.txt")), b"fixture").unwrap();
        }
        let query = format!("parent:\"{}\"", fixture.path().display());
        let mut session = SearchSession::default();
        unsafe {
            session.connect().unwrap();
        }
        let client = session.client;
        // Keep the real paging connection, but make only the watch's connection attempt fail.
        session.watch_instance = Some("colet-nonexistent-watch-test-instance".into());
        let deadline = Instant::now() + Duration::from_secs(15);
        let first = loop {
            let page = session.search(&query, Order::default(), 0, PAGE, true).unwrap();
            if page.total == 511 {
                break page;
            }
            assert!(Instant::now() < deadline, "Everything did not index the fixture: {}", page.total);
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(first.entries.len(), PAGE);
        assert_eq!(session.client, client);
        assert!(session.watch.is_none());
        assert!(session.connection.contains("手動更新（F5）"));
        assert!(!session.changed().unwrap());
        assert_eq!(session.search(&query, Order::default(), PAGE, PAGE, false).unwrap().entries.len(), 11);
        session.watch_instance = None;
        // A healthy watch is available now, but rapid searches must respect the retry deadline.
        // Control the deadline explicitly rather than rely on wall-clock timing for six SDK calls.
        let retry_at = Instant::now() + Duration::from_secs(60);
        session.watch_retry_at = Some(retry_at);
        for i in 0..6 {
            session.search(&format!("{query} colet-watch-probe-{i}"), Order::default(), 0, PAGE, true).unwrap();
            assert!(session.watch.is_none(), "a query retried the watch before its deadline");
            assert_eq!(session.watch_retry_at, Some(retry_at));
            assert_eq!(session.client, client);
        }
        session.watch_retry_at = Some(Instant::now());
        session.search(&query, Order::default(), 0, PAGE, true).unwrap();
        assert_eq!(session.client, client);
        assert!(session.watch.is_some());
        assert!(!session.connection.contains("手動更新"));
        std::fs::remove_file(fixture.path().join("0000.txt")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !session.changed().unwrap() {
            assert!(Instant::now() < deadline, "no change notification after recovering the watch");
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    #[ignore = "needs a running Everything 1.5 that indexes the temporary folder"]
    fn pagination_keeps_changes_until_the_loaded_range_is_refreshed() {
        let _alone = alone();
        let fixture = tempfile::Builder::new().prefix("colet-sdk-pages-").tempdir().unwrap();
        for i in 0..511 {
            std::fs::write(fixture.path().join(format!("{i:04}.txt")), b"fixture").unwrap();
        }
        let query = format!("parent:\"{}\"", fixture.path().display());
        let mut session = SearchSession::default();
        let deadline = Instant::now() + Duration::from_secs(15);
        let first = loop {
            let page = session.search(&query, Order::default(), 0, PAGE, true).unwrap();
            if page.total == 511 {
                break page;
            }
            assert!(Instant::now() < deadline, "Everything did not index the fixture: {}", page.total);
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(first.entries.len(), PAGE);
        let removed = fixture.path().join("0000.txt");
        std::fs::remove_file(&removed).unwrap();
        let wait = |session: &mut SearchSession| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !session.changed().unwrap() {
                assert!(Instant::now() < deadline, "no change notification");
                std::thread::sleep(Duration::from_millis(25));
            }
        };
        wait(&mut session);
        let second = session.search(&query, Order::default(), PAGE, PAGE, false).unwrap();
        assert_eq!(second.total, 510);
        assert!(session.changed().unwrap(), "pagination cleared the change notification");
        let fresh = session.search(&query, Order::default(), 0, PAGE * 2, true).unwrap();
        assert_eq!(fresh.entries.len(), 510);
        assert!(!fresh.entries.iter().any(|e| e.path == removed.display().to_string()));
        assert!(fresh.entries.iter().any(|e| e.name == "0500.txt"));
        assert!(!session.changed().unwrap());
        // The watch asks for one row, but must see metadata changes elsewhere in the result list too.
        std::fs::write(fixture.path().join("0240.txt"), b"changed size").unwrap();
        wait(&mut session);
        session.disconnect();
        assert!(!session.changed().unwrap());
    }

    #[test]
    #[ignore = "needs a running Everything 1.5 that indexes the temporary folder"]
    fn marks_where_the_search_matched_in_the_name_and_the_folder() {
        let _alone = alone();
        let fixture = tempfile::Builder::new().prefix("colet-sdk-marks-").tempdir().unwrap();
        std::fs::write(fixture.path().join("report 日本語.txt"), b"x").unwrap();
        let folder = fixture.path().display().to_string();
        let mut session = SearchSession::default();
        let deadline = Instant::now() + Duration::from_secs(15);
        let entry = loop {
            let page = session.search(&format!("parent:\"{folder}\" 本"), Order::default(), 0, PAGE, true).unwrap();
            if let Some(entry) = page.entries.into_iter().next() {
                break entry;
            }
            assert!(Instant::now() < deadline, "Everything did not index the fixture");
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(entry.marks.name, [10..13], "{entry:?}");
        assert_eq!(&entry.name[10..13], "本");
        // A folder matched as a whole marks the whole of it.
        assert_eq!(entry.marks.parent, [0..folder.len()]);
        // A path searched for marks it in the folder, and the rest of it in the name.
        let page = session.search(&format!(r"{folder}\rep"), Order::default(), 0, PAGE, true).unwrap();
        let entry = &page.entries[0];
        assert_eq!(entry.marks.parent, [0..folder.len()], "{entry:?}");
        assert_eq!(&entry.name[entry.marks.name[0].clone()], "rep");
        // Outside a search, nothing is marked.
        assert_eq!(Entry::folder(r"C:\", "C:").marks, Marks::default());
    }

    #[test]
    #[ignore = "needs a running Everything 1.5 that indexes the temporary folder"]
    fn puts_folders_first_descending_too() {
        let _alone = alone();
        let fixture = tempfile::Builder::new().prefix("colet-sdk-folders-").tempdir().unwrap();
        std::fs::write(fixture.path().join("a"), b"x").unwrap();
        std::fs::create_dir(fixture.path().join("z")).unwrap();
        let query = format!("parent:\"{}\"", fixture.path().display());
        let mut session = SearchSession::default();
        let deadline = Instant::now() + Duration::from_secs(15);
        for by in Sort::ALL {
            let entries = loop {
                let page = session.search(&query, Order { by, descending: true }, 0, PAGE, true).unwrap();
                if page.entries.len() == 2 {
                    break page.entries;
                }
                assert!(Instant::now() < deadline, "Everything did not index the fixture");
                std::thread::sleep(Duration::from_millis(100));
            };
            assert!(entries[0].directory, "{by:?}: {entries:?}");
        }
    }

    #[test]
    #[ignore = "needs a running Everything 1.5 that indexes the temporary folder"]
    fn pages_unicode_and_changes() {
        let _alone = alone();
        let fixture = tempfile::Builder::new().prefix("colet-sdk-").tempdir().unwrap();
        for i in 0..510 {
            std::fs::write(fixture.path().join(format!("{i:04}.txt")), b"fixture").unwrap();
        }
        let unicode = fixture.path().join("日本語 空白.txt");
        std::fs::write(&unicode, b"unicode").unwrap();
        let query = format!("parent:\"{}\"", fixture.path().display());
        let mut session = SearchSession::default();
        let deadline = Instant::now() + Duration::from_secs(15);
        let first = loop {
            let page = session.search(&query, Order::default(), 0, PAGE, true).unwrap();
            if page.total == 511 {
                break page;
            }
            assert!(Instant::now() < deadline, "Everything did not index the fixture: {}", page.total);
            std::thread::sleep(Duration::from_millis(150));
        };
        assert_eq!(first.entries.len(), 500);
        let second = session.search(&query, Order::default(), 500, PAGE, false).unwrap();
        assert_eq!(second.entries.len(), 11);
        assert!(second.entries.iter().chain(&first.entries).any(|e| e.path == unicode.display().to_string()));
        std::fs::write(fixture.path().join("new.txt"), b"new").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !session.changed().unwrap() {
            assert!(Instant::now() < deadline, "no change notification");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(session.search(&query, Order { by: Sort::Modified, descending: true }, 0, 1000, true).unwrap().total, 512);
    }
}
