//! Telling when what the open folders hold changes.
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use notify::{RecursiveMode, Watcher as _};

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
    wanted: mpsc::Sender<Vec<String>>,
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
            let Ok(event) = event else { return };
            if matches!(event.kind, notify::EventKind::Access(_)) {
                return;
            }
            let paths = event.paths.iter().map(|p| p.display().to_string()).collect();
            if changes.lock().unwrap().add(paths) {
                let _ = wake.send(());
            }
        })
        .map_err(|e| e.to_string())?;
        let (wanted, sets) = mpsc::channel::<Vec<String>>();
        // Ends once `wanted` is dropped, letting go of the watcher and with it the batches above.
        thread::Builder::new()
            .name("folder-watch".into())
            .spawn(move || {
                // Each folder asked for, and whether it is watched; one that failed is tried again only
                // once asked for afresh.
                let mut asked: HashMap<String, bool> = HashMap::new();
                while let Ok(mut paths) = sets.recv() {
                    // Only the latest set matters once the folders before it have kept this waiting.
                    while let Ok(newer) = sets.try_recv() {
                        paths = newer;
                    }
                    let wanted: HashSet<String> = paths.into_iter().collect();
                    asked.retain(|path, watched| {
                        let keep = wanted.contains(path);
                        if !keep && *watched {
                            let _ = watcher.unwatch(Path::new(path));
                        }
                        keep
                    });
                    let mut failed = Vec::new();
                    for path in wanted {
                        if asked.contains_key(&path) {
                            continue;
                        }
                        let folder = Path::new(&path);
                        let watched = watcher.watch(folder, RecursiveMode::NonRecursive).is_ok();
                        if !watched && std::fs::read_dir(folder).is_ok() {
                            failed.push(path.clone());
                        }
                        asked.insert(path, watched);
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
        let _ = self.wanted.send(paths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
