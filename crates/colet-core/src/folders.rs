//! What a folder holds, and the drives home lists.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FIND_FIRST_EX_FLAGS, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, GetDriveTypeW,
    GetLogicalDrives, GetVolumeInformationW, WIN32_FIND_DATAW,
};

use crate::{
    entry::{Entry, Hiding, Marks, Order, hiding, sort_entries},
    paths::{ancestors, child, from_wide, shown},
    wide::Wide,
};

/// `path` as it is named on disk, each name in its own case, and whether it is a folder: a junction or
/// link to one is, as listed, and is not followed. `None` when any part of the way is not there or cannot
/// be reached. It touches the disk, once for each folder on the way.
pub fn locate(path: &Path) -> Option<(PathBuf, bool)> {
    let chain = ancestors(path);
    let mut found = chain.first()?.clone();
    if !std::fs::metadata(&found).ok()?.is_dir() {
        return None;
    }
    let mut directory = true;
    for part in &chain[1..] {
        let mut data = WIN32_FIND_DATAW::default();
        let name = Wide::new(part);
        let handle = unsafe {
            FindFirstFileExW(name.get(), FindExInfoBasic, &mut data as *mut _ as *mut _, FindExSearchNameMatch, None, FIND_FIRST_EX_FLAGS(0))
        }
        .ok()?;
        unsafe { FindClose(handle) }.ok()?;
        found = child(&found, from_wide(&data.cFileName));
        directory = data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    }
    Some((found, directory))
}

/// What a listing that a newer one, or its column's removal, made unnecessary fails with.
pub const STALE: &str = "古いフォルダー要求です";

fn check(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) { Err(STALE.into()) } else { Ok(()) }
}

/// Checks before asking the iterator for another item, including its metadata.
fn collect<T>(mut items: impl Iterator<Item = T>, cancelled: &AtomicBool) -> Result<Vec<T>, String> {
    let mut all = Vec::new();
    loop {
        check(cancelled)?;
        match items.next() {
            Some(item) => all.push(item),
            None => return Ok(all),
        }
    }
}

/// The items directly in `path`, sorted; stops early, failing with [`STALE`], once `cancelled` is set.
pub fn list(path: &Path, order: Order, cancelled: &AtomicBool) -> Result<Vec<Entry>, String> {
    check(cancelled)?;
    if path.as_os_str().as_encoded_bytes().contains(&0) || !path.is_absolute() {
        return Err("絶対パスを指定してください".into());
    }
    let items = std::fs::read_dir(path).map_err(|e| format!("フォルダーを開けません: {e}"))?;
    let entries = collect(
        items.map(|entry| {
            // Not one item unreadable, whose name it would still give, but the listing broken off: a part
            // of the folder shown as all of it would hide the rest.
            let entry = entry.map_err(|e| format!("フォルダーを最後まで読めません: {e}"))?;
            let p = entry.path();
            // The listing already holds each item's size, date and attributes; asking the item again
            // opens it, one by one. A link or junction is still asked, for what it points to, but hides as
            // its own attributes say, as in Explorer: "Documents and Settings" is hidden, "Users" not.
            let listed = entry.metadata();
            let item = match &listed {
                Ok(m) if !m.file_type().is_symlink() => Ok(Entry::from_metadata(&p, m)),
                Ok(m) => {
                    let hidden = hiding(&p, std::os::windows::fs::MetadataExt::file_attributes(m));
                    Entry::from_path(&p).map(|e| Entry { hidden, ..e })
                }
                Err(_) => Entry::from_path(&p),
            };
            // Keep an item it cannot read in sight: a failure must not silently hide it.
            Ok(item.unwrap_or_else(|_| Entry {
                name: shown(entry.file_name()),
                parent: shown(path),
                path: p.clone(),
                directory: entry.file_type().is_ok_and(|t| t.is_dir()),
                size: None,
                modified: None,
                // The listing's own record, which needs no access to the item.
                hidden: listed
                    .as_ref()
                    .map_or(Hiding::Shown, |m| hiding(&p, std::os::windows::fs::MetadataExt::file_attributes(m))),
                marks: Marks::default(),
            }))
        }),
        cancelled,
    )?;
    let mut entries = entries.into_iter().collect::<Result<Vec<_>, String>>()?;
    check(cancelled)?;
    sort_entries(&mut entries, order);
    check(cancelled)?;
    Ok(entries)
}

/// The volume label, as Explorer shows it beside the letter. Asked only of local disks: a disconnected
/// network drive or an empty optical drive can keep the call waiting for seconds.
fn volume_label(root: &str) -> Option<String> {
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    let root = Wide::new(root);
    if !matches!(unsafe { GetDriveTypeW(root.get()) }, DRIVE_REMOVABLE | DRIVE_FIXED) {
        return None;
    }
    let mut label = [0u16; 261];
    unsafe { GetVolumeInformationW(root.get(), Some(&mut label), None, None, None, None) }.ok()?;
    let len = label.iter().position(|&c| c == 0).unwrap_or(label.len());
    let label = String::from_utf16_lossy(&label[..len]);
    (!label.trim().is_empty()).then_some(label)
}

/// The drives, named as Explorer names them (`データ (D:)`); their columns keep the letter alone.
pub fn roots() -> Vec<Entry> {
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| {
            let letter = (b'A' + i as u8) as char;
            let path = format!("{letter}:\\");
            let name = match volume_label(&path) {
                Some(label) => format!("{label} ({letter}:)"),
                None => format!("{letter}:"),
            };
            Entry::folder(path, &name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_listing_never_opens_the_folder() {
        assert_eq!(list(Path::new("invalid path"), Order::default(), &AtomicBool::new(true)).unwrap_err(), STALE);
    }

    #[test]
    fn cancelling_while_listing_stops_before_the_next_item() {
        let cancelled = AtomicBool::new(false);
        let mut visited = 0;
        let items = (0..100).inspect(|&i| {
            visited += 1;
            if i == 2 {
                cancelled.store(true, Ordering::Relaxed);
            }
        });
        assert!(collect(items, &cancelled).is_err());
        assert_eq!(visited, 3);
    }

    #[test]
    fn lists_with_metadata_folders_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "data").unwrap();
        std::fs::create_dir(dir.path().join("z")).unwrap();
        let entries = list(dir.path(), Order::default(), &AtomicBool::new(false)).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "z");
        assert!(entries[0].directory);
        assert_eq!(entries[1].size, Some(4));
    }

    #[test]
    fn a_junction_hides_as_its_own_attributes_say_not_its_targets() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let run = |program: &str, args: &[&std::ffi::OsStr]| {
            assert!(std::process::Command::new(program).args(args).output().unwrap().status.success());
        };
        let (hidden, shown) = (dir.path().join("hidden link"), dir.path().join("shown link"));
        for link in [&hidden, &shown] {
            run("cmd", &["/C".as_ref(), "mklink".as_ref(), "/J".as_ref(), link.as_os_str(), target.as_os_str()]);
        }
        // On the link itself, not what it points to.
        run("attrib", &["+h".as_ref(), "+s".as_ref(), "/L".as_ref(), hidden.as_os_str()]);
        let entries = list(dir.path(), Order::default(), &AtomicBool::new(false)).unwrap();
        let hides = |name: &str| entries.iter().find(|e| e.name == name).unwrap().hidden.is_hidden();
        assert!(hides("hidden link"));
        assert!(!hides("shown link"));
        assert!(!hides("target"));
        assert!(entries.iter().all(|e| e.directory));
    }

    #[test]
    fn lists_a_name_that_is_not_text_as_the_item_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let odd = crate::paths::tests::unpaired(dir.path(), "a");
        let real = dir.path().join("a\u{FFFD}X");
        std::fs::write(&odd, "odd").unwrap();
        std::fs::write(&real, "real").unwrap();
        let entries = list(dir.path(), Order::default(), &AtomicBool::new(false)).unwrap();
        // Both show alike, yet each path still opens its own item.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, entries[1].name);
        let mut read: Vec<String> = entries.iter().map(|e| std::fs::read_to_string(&e.path).unwrap()).collect();
        read.sort();
        assert_eq!(read, ["odd", "real"]);
    }

    #[test]
    fn locates_a_path_in_the_case_it_is_named_on_disk_without_following_a_link() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Mixed")).unwrap();
        std::fs::write(dir.path().join(r"Mixed\Inner.TXT"), "").unwrap();
        let (found, directory) = locate(&dir.path().join(r"mixed\inner.txt")).unwrap();
        assert!(found.to_string_lossy().ends_with(r"\Mixed\Inner.TXT"), "{found:?}");
        assert!(!directory);
        assert_eq!(crate::paths::path_key(&found), crate::paths::path_key(&dir.path().join(r"Mixed\Inner.TXT")));
        let (found, directory) = locate(&dir.path().join("MIXED")).unwrap();
        assert!(found.ends_with("Mixed") && directory);
        assert_eq!(locate(&dir.path().join(r"Mixed\gone")), None);
        assert_eq!(locate(&dir.path().join(r"gone\Inner.TXT")), None);
        assert_eq!(locate(Path::new(r"C:\")), Some((PathBuf::from(r"C:\"), true)));
        let link = dir.path().join("link");
        let made = std::process::Command::new("cmd").args(["/C".as_ref(), "mklink".as_ref(), "/J".as_ref(), link.as_os_str(), dir.path().join("Mixed").as_os_str()]).output().unwrap();
        assert!(made.status.success());
        let (found, directory) = locate(&dir.path().join(r"LINK\inner.txt")).unwrap();
        assert!(found.to_string_lossy().ends_with(r"\link\Inner.TXT"), "{found:?}");
        assert!(!directory);
    }

    #[test]
    fn lists_the_system_drive() {
        let drives = roots();
        let system = drives.iter().find(|d| d.path == Path::new("C:\\")).expect("C:");
        assert!(system.name.ends_with("C:)") || system.name == "C:", "{}", system.name);
    }
}
