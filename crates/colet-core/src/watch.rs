//! Telling when what the open folders hold changes.
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
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

fn record(changes: &Mutex<Changes>, wake: &mpsc::Sender<()>, paths: Vec<String>) {
    if changes.lock().unwrap().add(paths) {
        let _ = wake.send(());
    }
}

/// The paths changed since the last batch was taken; none named means anything may have.
#[derive(Default)]
pub struct Changes {
    all: bool,
    paths: HashSet<String>,
}

impl Changes {
    /// Only the first change in a batch schedules it to be sent: true then. An empty `paths` stands for
    /// any change, even after named ones.
    pub fn add(&mut self, paths: Vec<String>) -> bool {
        let schedule = !self.all && self.paths.is_empty();
        if paths.is_empty() {
            self.all = true;
            self.paths.clear();
        } else if !self.all {
            self.paths.extend(paths);
        }
        schedule
    }

    /// The batch, empty when any change may have happened.
    pub fn take(&mut self) -> Vec<String> {
        self.all = false;
        self.paths.drain().collect()
    }
}

/// Watches folders, one level deep each, and hands `changed` each batch of changed paths a little after
/// the first in it. Dropped, it stops.
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
    /// Watches nothing until [`set`](Self::set). `unwatched` hears of the folders it could list but not
    /// watch, as for a share that sends no change notifications; one it cannot read is left out, as its
    /// column says so. Both are called on the watcher's threads.
    pub fn new(
        changed: impl Fn(Vec<String>) + Send + 'static,
        unwatched: impl Fn(Vec<String>) + Send + 'static,
    ) -> Result<Self, String> {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let (wake, batches) = mpsc::channel::<()>();
        let desired = Arc::new(Mutex::new(Vec::<String>::new()));
        let requested = desired.clone();
        let (wanted, sets) = mpsc::channel::<WatchCommand>();
        let repair = wanted.clone();
        let (repaired, repaired_wake) = (changes.clone(), wake.clone());
        let queued = changes.clone();
        // Asleep when idle; dropping the watcher closes the channel once any batch is delivered.
        thread::spawn(move || {
            for () in batches {
                thread::sleep(Duration::from_millis(100));
                let paths = queued.lock().unwrap().take();
                changed(paths);
            }
        });
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
            record(&changes, &wake, if error { Vec::new() } else { paths });
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
                        record(&repaired, &repaired_wake, refreshed);
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
    fn a_burst_keeps_both_rename_paths_and_schedules_one_batch() {
        let mut changes = Changes::default();
        assert!(changes.add(vec!["old".into(), "new".into()]));
        for _ in 0..1000 {
            assert!(!changes.add(vec!["new".into()]));
        }
        let paths: HashSet<_> = changes.take().into_iter().collect();
        assert_eq!(paths, HashSet::from(["old".into(), "new".into()]));
        assert!(changes.add(vec!["later".into()]));
        assert_eq!(changes.take(), ["later"]);
    }

    #[test]
    fn an_unnamed_change_dominates_the_batch_but_not_the_next() {
        let mut changes = Changes::default();
        changes.add(vec!["before".into()]);
        changes.add(vec![]);
        changes.add(vec!["after".into()]);
        assert!(changes.take().is_empty());
        assert!(changes.add(vec!["next".into()]));
        assert_eq!(changes.take(), ["next"]);
    }

    /// Writes a file into `dir` until the watcher reports one there: it starts watching a moment after it
    /// is asked to.
    fn reported(dir: &Path, received: &mpsc::Receiver<Vec<String>>) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let prefix = dir.display().to_string();
        let mut i = 0;
        while std::time::Instant::now() < deadline {
            i += 1;
            std::fs::write(dir.join(format!("new{i}.txt")), "x").unwrap();
            if received.recv_timeout(Duration::from_millis(300)).is_ok_and(|paths| paths.iter().any(|p| p.starts_with(&prefix))) {
                return true;
            }
        }
        false
    }

    #[test]
    fn watches_the_folders_set_and_lets_go_of_those_dropped() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (sent, received) = mpsc::channel();
        let (missed, unwatched) = mpsc::channel();
        let watcher = FolderWatcher::new(
            move |paths| sent.send(paths).unwrap(),
            move |paths| missed.send(paths).unwrap(),
        )
        .unwrap();
        let path = |dir: &tempfile::TempDir| dir.path().display().to_string();
        watcher.set(vec![path(&a)]);
        assert!(reported(a.path(), &received));
        watcher.set(vec![path(&b)]);
        assert!(reported(b.path(), &received));
        // Let go of, `a` reports nothing more.
        while received.recv_timeout(Duration::from_millis(300)).is_ok() {}
        std::fs::write(a.path().join("later.txt"), "x").unwrap();
        assert!(received.recv_timeout(Duration::from_millis(500)).is_err());
        assert!(unwatched.try_recv().is_err());
    }

    #[test]
    fn watches_a_replacement_at_the_same_path_without_forwarding_its_parents_other_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watched");
        std::fs::create_dir(&path).unwrap();
        let (sent, received) = mpsc::channel();
        let watcher = FolderWatcher::new(move |paths| { let _ = sent.send(paths); }, |_| {}).unwrap();
        watcher.set(vec![path.display().to_string()]);
        assert!(reported(&path, &received));
        std::fs::rename(&path, dir.path().join("old")).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(reported(&path, &received));
        // A replacement's own notification must not stand in for notifications from its new handle.
        while received.recv_timeout(Duration::from_millis(200)).is_ok() {}
        assert!(reported(&path, &received));
        while received.recv_timeout(Duration::from_millis(200)).is_ok() {}
        std::fs::write(dir.path().join("unrelated.txt"), b"outside").unwrap();
        std::fs::write(dir.path().join("old").join("after-rename.txt"), b"old handle").unwrap();
        assert!(received.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
