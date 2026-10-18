//! Telling when what the open folders hold changes.
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
};

use notify::{RecursiveMode, Watcher as _};

use crate::paths::path_key;

fn inside(path: &str, folder: &str) -> bool {
    path == folder || path.strip_prefix(folder).is_some_and(|rest| rest.starts_with('\\'))
}

fn relevant(path: &str, folder: &str) -> bool {
    inside(folder, path) || path.rsplit_once('\\').is_some_and(|(parent, _)| parent == folder)
}

/// Both arguments already hold path keys. Children change a listing, but only a watched folder or
/// one of its ancestors can invalidate its directory handle.
fn repair_paths(paths: &[String], folders: &[String]) -> Vec<String> {
    paths.iter().filter(|path| folders.iter().any(|folder| inside(folder, path))).cloned().collect()
}

/// What changed since the batch was last taken.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Changes {
    #[default]
    Nothing,
    Paths(HashSet<String>),
    /// Anything may have: an unnamed change, which outweighs the named ones around it.
    All,
}

impl Changes {
    /// An empty `paths` stands for any change.
    pub fn add(&mut self, paths: Vec<String>) {
        if paths.is_empty() {
            *self = Changes::All;
            return;
        }
        match self {
            Changes::Nothing => *self = Changes::Paths(paths.into_iter().collect()),
            Changes::Paths(batch) => batch.extend(paths),
            Changes::All => {}
        }
    }

    /// The batch, leaving nothing behind.
    pub fn take(&mut self) -> Changes {
        std::mem::take(self)
    }
}

/// Watches folders, one level deep each, adding what changes in them to a batch the caller takes when it
/// likes. Dropped, it stops.
///
/// Starting to watch a folder opens it, which a share that answers slowly can keep waiting for seconds:
/// that happens on a thread of its own, never the caller's.
pub struct FolderWatcher {
    wanted: mpsc::Sender<WatchCommand>,
}

enum WatchCommand {
    Set(Vec<String>),
    Repair(Vec<String>),
    Stop,
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        let _ = self.wanted.send(WatchCommand::Stop);
    }
}

impl FolderWatcher {
    /// Watches nothing until [`set`](Self::set), adding what changes to `changes`. `unwatched` hears of
    /// the folders it could list but not watch, as for a share that sends no change notifications; one it
    /// cannot read is left out, as its column says so. It is called on the watcher's thread.
    pub fn new(
        changes: Arc<Mutex<Changes>>,
        unwatched: impl Fn(Vec<String>) + Send + 'static,
    ) -> Result<Self, String> {
        let desired = Arc::new(Mutex::new(Vec::<String>::new()));
        let requested = desired.clone();
        let (wanted, sets) = mpsc::channel::<WatchCommand>();
        let repair = wanted.clone();
        let repaired = changes.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let (paths, reset, error) = match event {
                Ok(event) => {
                    if matches!(event.kind, notify::EventKind::Access(_)) {
                        return;
                    }
                    let reset = matches!(event.kind,
                        notify::EventKind::Any | notify::EventKind::Create(_) | notify::EventKind::Remove(_)
                        | notify::EventKind::Modify(notify::event::ModifyKind::Name(_)));
                    (event.paths, reset, false)
                }
                Err(error) => (error.paths, true, true),
            };
            let paths: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
            let keys: Vec<String> = paths.iter().map(|p| path_key(p)).collect();
            let requested = requested.lock().unwrap();
            if !keys.is_empty() && !keys.iter().any(|p| requested.iter().any(|f| relevant(p, f))) {
                return;
            }
            let repairs = if reset { repair_paths(&keys, &requested) } else { Vec::new() };
            drop(requested);
            if reset && (paths.is_empty() || !repairs.is_empty()) {
                let _ = repair.send(WatchCommand::Repair(repairs));
            }
            changes.lock().unwrap().add(if error { Vec::new() } else { paths });
        })
        .map_err(|e| e.to_string())?;
        // Dropping the handle sends Stop: the callback also has a sender, so disconnection alone
        // cannot stop this thread. Waiting on commands keeps it asleep while no folders change.
        thread::Builder::new()
            .name("folder-watch".into())
            .spawn(move || {
                let mut wanted = HashMap::<String, String>::new();
                // Keep each folder's parent too: a directory handle follows a renamed directory, so
                // only its parent reports that a new directory has taken the same path.
                let mut asked = HashMap::<String, (String, bool)>::new();
                while let Ok(mut command) = sets.recv() {
                    let mut next = None;
                    let mut all = false;
                    let mut reset = HashSet::new();
                    loop {
                        match command {
                            WatchCommand::Set(paths) => next = Some(paths),
                            WatchCommand::Repair(paths) => {
                                all |= paths.is_empty();
                                reset.extend(paths);
                            }
                            WatchCommand::Stop => return,
                        }
                        match sets.try_recv() {
                            Ok(newer) => command = newer,
                            Err(_) => break,
                        }
                    }
                    let mut checked = HashSet::new();
                    if let Some(paths) = next {
                        wanted = paths.into_iter().map(|p| (path_key(&p), p)).collect();
                        *desired.lock().unwrap() = wanted.keys().cloned().collect();
                        let mut needed = wanted.clone();
                        for path in wanted.values() {
                            if let Some(parent) = Path::new(path).parent() {
                                let parent = parent.display().to_string();
                                needed.entry(path_key(&parent)).or_insert(parent);
                            }
                        }
                        asked.retain(|key, (path, watched)| {
                            let keep = needed.contains_key(key);
                            if !keep && *watched {
                                let _ = watcher.unwatch(Path::new(path));
                            }
                            keep
                        });
                        for (key, path) in needed {
                            if asked.contains_key(&key) {
                                continue;
                            }
                            let folder = Path::new(&path);
                            let watched = watcher.watch(folder, RecursiveMode::NonRecursive).is_ok();
                            checked.insert(key.clone());
                            asked.insert(key, (path, watched));
                        }
                    }
                    let mut refreshed = Vec::new();
                    for (key, (path, watched)) in &mut asked {
                        if !all && !reset.iter().any(|p| inside(key, p)) {
                            continue;
                        }
                        if *watched {
                            let _ = watcher.unwatch(Path::new(path));
                        }
                        let folder = Path::new(path);
                        *watched = watcher.watch(folder, RecursiveMode::NonRecursive).is_ok();
                        checked.insert(key.clone());
                        if wanted.contains_key(key) && *watched {
                            refreshed.push(path.clone());
                        }
                    }
                    // A Set and Repair can try the same folder twice. Report its final state once,
                    // and do not report an initial failure that the repair recovered from.
                    let failed: Vec<String> = checked.into_iter().filter_map(|key| {
                        let (path, watched) = asked.get(&key)?;
                        (!watched && wanted.contains_key(&key) && std::fs::read_dir(path).is_ok()).then(|| path.clone())
                    }).collect();
                    if !refreshed.is_empty() {
                        // List again after the new handle is installed, covering the gap before it.
                        repaired.lock().unwrap().add(refreshed);
                    }
                    if !failed.is_empty() {
                        unwatched(failed);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { wanted })
    }

    /// Watches exactly `paths` from now on: the new ones are watched, the ones gone let go. Returns at
    /// once.
    pub fn set(&self, paths: Vec<String>) {
        let _ = self.wanted.send(WatchCommand::Set(paths));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn file_changes_reach_the_listing_without_repairing_directory_handles() {
        let folders = vec![path_key(r"D:\Work\"), path_key(r"D:\Work\Nested")];
        let files = vec![path_key(r"d:\work\file.txt"), path_key(r"D:\WORK\Nested\another.txt")];
        assert!(files.iter().all(|p| folders.iter().any(|f| relevant(p, f))));
        assert!(repair_paths(&files, &folders).is_empty());
        assert!(!relevant(&path_key(r"D:\Work-other\file.txt"), &folders[0]));
    }

    #[test]
    fn only_changes_to_watched_folders_and_their_ancestors_repair_handles() {
        let folders = vec![path_key(r"D:\Work\Nested"), path_key(r"\\server\share\folder")];
        let paths: Vec<String> = [r"D:\", r"d:\WORK", r"D:\Work\Nested", r"D:\Work\other", r"\\SERVER\share"]
            .into_iter().map(path_key).collect();
        assert_eq!(repair_paths(&paths, &folders), [paths[0].clone(), paths[1].clone(), paths[2].clone(), paths[4].clone()]);
        assert!(paths.iter().take(3).all(|p| relevant(p, &folders[0])));
    }

    #[test]
    fn a_burst_keeps_both_rename_paths_in_one_batch() {
        let mut changes = Changes::default();
        changes.add(vec!["old".into(), "new".into()]);
        for _ in 0..1000 {
            changes.add(vec!["new".into()]);
        }
        assert_eq!(changes.take(), Changes::Paths(HashSet::from(["old".into(), "new".into()])));
        assert_eq!(changes.take(), Changes::Nothing);
        changes.add(vec!["later".into()]);
        assert_eq!(changes.take(), Changes::Paths(HashSet::from(["later".into()])));
    }

    #[test]
    fn an_unnamed_change_dominates_the_batch_but_not_the_next() {
        let mut changes = Changes::default();
        changes.add(vec!["before".into()]);
        changes.add(vec![]);
        changes.add(vec!["after".into()]);
        assert_eq!(changes.take(), Changes::All);
        changes.add(vec!["next".into()]);
        assert_eq!(changes.take(), Changes::Paths(HashSet::from(["next".into()])));
    }

    /// The paths of the next batch to arrive within `wait`, taken as the window takes them; none named
    /// for any change.
    fn next(changes: &Mutex<Changes>, wait: Duration) -> Option<Vec<String>> {
        let deadline = std::time::Instant::now() + wait;
        while std::time::Instant::now() < deadline {
            match changes.lock().unwrap().take() {
                Changes::Nothing => {}
                Changes::Paths(paths) => return Some(paths.into_iter().collect()),
                Changes::All => return Some(Vec::new()),
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    /// Waits out the batches still arriving.
    fn drain(changes: &Mutex<Changes>, quiet: Duration) {
        while next(changes, quiet).is_some() {}
    }

    /// Writes a file into `dir` until the watcher reports one there: it starts watching a moment after it
    /// is asked to.
    fn reported(dir: &Path, changes: &Mutex<Changes>) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let prefix = dir.display().to_string();
        let mut i = 0;
        while std::time::Instant::now() < deadline {
            i += 1;
            std::fs::write(dir.join(format!("new{i}.txt")), "x").unwrap();
            if next(changes, Duration::from_millis(300)).is_some_and(|paths| paths.iter().any(|p| p.starts_with(&prefix))) {
                return true;
            }
        }
        false
    }

    #[test]
    fn watches_the_folders_set_and_lets_go_of_those_dropped() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let changes = Arc::new(Mutex::new(Changes::default()));
        let (missed, unwatched) = mpsc::channel();
        let watcher = FolderWatcher::new(changes.clone(), move |paths| missed.send(paths).unwrap()).unwrap();
        let path = |dir: &tempfile::TempDir| dir.path().display().to_string();
        watcher.set(vec![path(&a)]);
        assert!(reported(a.path(), &changes));
        watcher.set(vec![path(&b)]);
        assert!(reported(b.path(), &changes));
        // Let go of, `a` reports nothing more.
        drain(&changes, Duration::from_millis(300));
        std::fs::write(a.path().join("later.txt"), "x").unwrap();
        assert!(next(&changes, Duration::from_millis(500)).is_none());
        assert!(unwatched.try_recv().is_err());
    }

    #[test]
    fn watches_a_replacement_at_the_same_path_without_forwarding_its_parents_other_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watched");
        std::fs::create_dir(&path).unwrap();
        let changes = Arc::new(Mutex::new(Changes::default()));
        let watcher = FolderWatcher::new(changes.clone(), |_| {}).unwrap();
        watcher.set(vec![path.display().to_string()]);
        assert!(reported(&path, &changes));
        std::fs::rename(&path, dir.path().join("old")).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(reported(&path, &changes));
        // A replacement's own notification must not stand in for notifications from its new handle.
        drain(&changes, Duration::from_millis(200));
        assert!(reported(&path, &changes));
        drain(&changes, Duration::from_millis(200));
        std::fs::write(dir.path().join("unrelated.txt"), b"outside").unwrap();
        std::fs::write(dir.path().join("old").join("after-rename.txt"), b"old handle").unwrap();
        assert!(next(&changes, Duration::from_millis(300)).is_none());
    }
}
