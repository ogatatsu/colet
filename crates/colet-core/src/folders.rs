//! What a folder holds, and the drives home lists.
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};

use crate::{
    entry::{Entry, Marks, Order, is_hidden, sort_entries},
    wide::Wide,
};

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
pub fn list(path: &str, order: Order, cancelled: &AtomicBool) -> Result<Vec<Entry>, String> {
    check(cancelled)?;
    if path.contains('\0') || !Path::new(path).is_absolute() {
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
            // opens it, one by one. A link or junction is still asked, for what it points to.
            let listed = entry.metadata();
            let item = match &listed {
                Ok(m) if !m.file_type().is_symlink() => Ok(Entry::from_metadata(&p, m)),
                _ => Entry::from_path(&p),
            };
            // Keep an item it cannot read in sight: a failure must not silently hide it.
            Ok(item.unwrap_or_else(|_| Entry {
                path: p.display().to_string(),
                name: entry.file_name().to_string_lossy().into_owned(),
                parent: path.to_owned(),
                directory: entry.file_type().is_ok_and(|t| t.is_dir()),
                size: None,
                modified: None,
                // The listing's own record, which needs no access to the item.
                hidden: listed.as_ref().is_ok_and(|m| {
                    is_hidden(&p, std::os::windows::fs::MetadataExt::file_attributes(m))
                }),
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
            Entry::folder(&path, &name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_listing_never_opens_the_folder() {
        assert_eq!(list("invalid path", Order::default(), &AtomicBool::new(true)).unwrap_err(), STALE);
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
        let entries = list(&dir.path().display().to_string(), Order::default(), &AtomicBool::new(false)).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "z");
        assert!(entries[0].directory);
        assert_eq!(entries[1].size, Some(4));
    }

    #[test]
    fn lists_the_system_drive() {
        let drives = roots();
        let system = drives.iter().find(|d| d.path == "C:\\").expect("C:");
        assert!(system.name.ends_with("C:)") || system.name == "C:", "{}", system.name);
    }
}
