//! Searching through Everything 1.5's SDK, which is compiled in from `vendor/everything-sdk3`.
use std::{ffi::c_void, path::Path, ptr};

use crate::entry::{Entry, Sort};

type Handle = *mut c_void;

#[link(name = "everything3", kind = "static")]
unsafe extern "system" {
    fn Everything3_ConnectW(instance: *const u16) -> Handle;
    fn Everything3_DestroyClient(client: Handle) -> i32;
    fn Everything3_IsDBLoaded(client: Handle) -> i32;
    fn Everything3_GetLastError() -> u32;
    fn Everything3_CreateSearchState() -> Handle;
    fn Everything3_DestroySearchState(state: Handle) -> i32;
    fn Everything3_SetSearchTextW(state: Handle, text: *const u16) -> i32;
    fn Everything3_SetSearchViewportOffset(state: Handle, offset: usize) -> i32;
    fn Everything3_SetSearchViewportCount(state: Handle, count: usize) -> i32;
    fn Everything3_AddSearchPropertyRequest(state: Handle, property: u32) -> i32;
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
    fn Everything3_IsResultListChange(client: Handle) -> i32;
}

/// How many results a page holds.
pub const PAGE: usize = 500;

/// A run of results: `entries` from `offset` on, of `total` in all.
#[derive(Debug)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub total: usize,
    pub offset: usize,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// A connection to Everything and the search it last ran, kept so later pages come from the same one.
pub struct SearchSession {
    client: Handle,
    state: Handle,
    key: Option<(String, Sort)>,
    instance: String,
    /// What it is connected to, as the status bar names it.
    pub connection: String,
}

// Used from one thread at a time, behind the app's lock.
unsafe impl Send for SearchSession {}

impl Default for SearchSession {
    fn default() -> Self {
        Self { client: ptr::null_mut(), state: ptr::null_mut(), key: None, instance: String::new(), connection: String::new() }
    }
}

impl Drop for SearchSession {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl SearchSession {
    pub fn disconnect(&mut self) {
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
    }

    /// Whether Everything has said the last search's results changed since it was last asked.
    pub fn changed(&mut self) -> Result<bool, String> {
        if self.client.is_null() || self.key.is_none() {
            return Ok(false);
        }
        unsafe {
            let changed = Everything3_IsResultListChange(self.client) != 0;
            let error = Everything3_GetLastError();
            if !changed && error != 0 {
                self.disconnect();
                return Err(format!("Everythingとの接続が切れました (0x{error:08X})"));
            }
            Ok(changed)
        }
    }

    /// `count` results of `query` from `offset` on. `refresh` searches afresh even when the query and
    /// sort are the ones last run. `instance` empty tries the default instance, then `1.5a`.
    pub fn search(
        &mut self,
        query: &str,
        sort: Sort,
        instance: &str,
        offset: usize,
        count: usize,
        refresh: bool,
    ) -> Result<Page, String> {
        if query.contains('\0') || instance.contains('\0') {
            return Err("入力に使用できない文字が含まれています".into());
        }
        if !self.client.is_null() && self.instance != instance {
            self.disconnect();
        }
        unsafe {
            if self.client.is_null() {
                self.connect(instance)?;
            }
            if Everything3_IsDBLoaded(self.client) == 0 {
                if Everything3_GetLastError() != 0 {
                    self.disconnect();
                    return Err("Everythingとの接続が切れました。再接続してください。".into());
                }
                return Err("Everythingの索引を準備中です。しばらく待って再試行してください。".into());
            }
            let key = (query.to_owned(), sort);
            let new_search = self.key.as_ref() != Some(&key) || refresh;
            if new_search || self.state.is_null() {
                self.prepare(query, sort)?;
            }
            // These setters update the state but return FALSE in the pinned upstream version.
            Everything3_SetSearchViewportOffset(self.state, offset);
            Everything3_SetSearchViewportCount(self.state, count);
            let result = if new_search {
                Everything3_Search(self.client, self.state)
            } else {
                Everything3_GetResults(self.client, self.state)
            };
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

    unsafe fn connect(&mut self, instance: &str) -> Result<(), String> {
        let candidates: &[&str] = if instance.is_empty() { &["", "1.5a"] } else { &[instance] };
        for candidate in candidates {
            let name = wide(candidate);
            let client = unsafe { Everything3_ConnectW(if candidate.is_empty() { ptr::null() } else { name.as_ptr() }) };
            if !client.is_null() {
                self.client = client;
                self.instance = instance.to_owned();
                self.connection =
                    if candidate.is_empty() { "Everything 1.5".into() } else { format!("Everything · {candidate}") };
                return Ok(());
            }
        }
        Err("Everything 1.5に接続できません。本体が起動しているか確認して、再試行してください。".into())
    }

    unsafe fn prepare(&mut self, query: &str, sort: Sort) -> Result<(), String> {
        unsafe {
            if !self.state.is_null() {
                Everything3_DestroySearchState(self.state);
            }
            self.state = Everything3_CreateSearchState();
            if self.state.is_null() {
                return Err("検索状態を作成できません".into());
            }
            Everything3_SetSearchTextW(self.state, wide(query).as_ptr());
            // Not attributes: Everything rarely indexes them, and reads each result's from the disk
            // instead, stalling the search as it is typed.
            for property in [240, 2, 5] {
                Everything3_AddSearchPropertyRequest(self.state, property);
            }
            Everything3_SetSearchFoldersFirst(self.state, 1);
            let property = match sort {
                Sort::Name => 0,
                Sort::Size => 2,
                Sort::Modified => 5,
            };
            Everything3_AddSearchSort(self.state, property, i32::from(sort == Sort::Name));
            if sort != Sort::Name {
                Everything3_AddSearchSort(self.state, 0, 1);
            }
        }
        Ok(())
    }
}

unsafe fn read(result: Handle, offset: usize) -> Page {
    unsafe {
        let total = Everything3_GetResultListCount(result);
        let count = Everything3_GetResultListViewportCount(result);
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let length = Everything3_GetResultFullPathNameW(result, index, ptr::null_mut(), 0);
            let mut buffer = vec![0u16; length + 1];
            Everything3_GetResultFullPathNameW(result, index, buffer.as_mut_ptr(), buffer.len());
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            let path = String::from_utf16_lossy(&buffer[..end]);
            let directory = Everything3_IsFolderResult(result, index) != 0;
            let size = Everything3_GetResultSize(result, index);
            let modified = Everything3_GetResultDateModified(result, index);
            let p = Path::new(&path);
            entries.push(Entry {
                name: p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone()),
                parent: p.parent().map(|s| s.display().to_string()).unwrap_or_default(),
                directory,
                size: (!directory && size != u64::MAX).then_some(size),
                // FILETIME to Unix milliseconds.
                modified: (modified != u64::MAX)
                    .then(|| modified.checked_sub(116_444_736_000_000_000).map(|v| v / 10_000))
                    .flatten(),
                hidden: false,
                path,
            });
        }
        Page { entries, total, offset }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "needs a running Everything 1.5 that indexes the temporary folder"]
    fn pages_unicode_and_changes() {
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
            let page = session.search(&query, Sort::Name, "", 0, PAGE, true).unwrap();
            if page.total == 511 {
                break page;
            }
            assert!(Instant::now() < deadline, "Everything did not index the fixture: {}", page.total);
            std::thread::sleep(Duration::from_millis(150));
        };
        assert_eq!(first.entries.len(), 500);
        let second = session.search(&query, Sort::Name, "", 500, PAGE, false).unwrap();
        assert_eq!(second.entries.len(), 11);
        assert!(second.entries.iter().chain(&first.entries).any(|e| e.path == unicode.display().to_string()));
        std::fs::write(fixture.path().join("new.txt"), b"new").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !session.changed().unwrap() {
            assert!(Instant::now() < deadline, "no change notification");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(session.search(&query, Sort::Modified, "", 0, 1000, true).unwrap().total, 512);
        assert!(session.search(&query, Sort::Name, "colet-nonexistent-test-instance", 0, PAGE, true).is_err());
    }
}
