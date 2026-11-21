//! File operations as Colet asks for them: a paste from the clipboard, sending to the Recycle Bin or
//! deleting outright, a rename, a new folder. Checked here, carried out by the shell with its own dialogs.
use std::collections::HashSet;
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use windows::Win32::Foundation::E_ABORT;
use windows::core::HRESULT;

use crate::paths::{child, path_key};
use crate::shell::{self, Dropped, Item, Operation, Owner, clipboard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// What the clipboard holds, copied or moved into `destination`.
    Paste,
    /// To the Recycle Bin; what the bin cannot take is deleted outright, once the shell has asked.
    Recycle,
    /// Deleted outright, once the shell has asked.
    Delete,
    Rename,
    NewFolder,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub kind: Kind,
    /// What it acts on; a paste takes them from the clipboard.
    pub paths: Vec<PathBuf>,
    /// Where a paste goes, or where a new folder is made.
    pub destination: PathBuf,
    /// A rename's new name, or the new folder's.
    pub name: String,
}

/// How one item ended, or where a copy or move put it.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Done {
        path: PathBuf,
    },
    Failed {
        path: PathBuf,
        error: HRESULT,
    },
    /// Passed over at the user's word, as from a name clash.
    Skipped {
        path: PathBuf,
    },
    /// Stopped at the user's word.
    Cancelled {
        path: PathBuf,
    },
    /// Where the last item went: copied, moved or renamed, under the name it got, which may not be the
    /// one asked.
    Placed {
        path: PathBuf,
    },
    /// The last item done was deleted outright, not sent to the Recycle Bin.
    Destroyed {
        path: PathBuf,
    },
    /// The last item done is no longer where it was: moved, renamed or deleted. A copy's
    /// source stays, and a new folder was nowhere before.
    Left {
        path: PathBuf,
    },
    /// Handed to the shell, which writes it out on its own and tells nothing back: where it goes by the
    /// name it was given, which a name clash may change.
    Handed {
        path: PathBuf,
    },
}

/// Why an operation was not carried out, or not all of it. The window says each in words of its own;
/// these are for a developer.
#[derive(Debug, thiserror::Error)]
pub enum OperationError {
    /// The user stopped it.
    #[error("cancelled")]
    Cancelled,
    #[error("the path is not absolute")]
    NotAbsolute,
    #[error("the destination is not a folder")]
    NotAFolder,
    #[error("a folder cannot go into itself or a folder inside it")]
    IntoItself,
    #[error("no files on the clipboard")]
    NoFilesOnClipboard,
    #[error("nothing selected")]
    NothingSelected,
    #[error("the name cannot be used")]
    InvalidName,
    #[error("one item is renamed at a time")]
    RenameOneAtATime,
    #[error("a drive's root cannot be acted on")]
    DriveRoot,
    #[error("the destination is not found")]
    NoDestination,
    #[error("the destination refused what the clipboard holds")]
    Refused,
    #[error("the clipboard changed before the paste")]
    ClipboardChanged,
    /// Some items failed; how each went is in the history.
    #[error("some items failed")]
    PartlyFailed,
    /// A path could not be looked up on the disk.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The shell refused.
    #[error("the shell refused (0x{:08X})", .0.0 as u32)]
    Windows(HRESULT),
}

/// What Windows refuses in a file or folder name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameProblem {
    /// A control character, or one of `\ / : * ? " < > |`.
    ForbiddenCharacter,
    /// A period or a space at the end.
    TrailingDotOrSpace,
    /// A device's name, as `CON` or `COM1`.
    Reserved,
}

/// COPYENGINE_S_USER_IGNORED: skipped at the user's word.
const USER_IGNORED: HRESULT = HRESULT(0x0027_0005);
/// COPYENGINE_S_PENDING: waiting on a question; not its end yet.
const PENDING: HRESULT = HRESULT(0x0027_000B);
/// COPYENGINE_E_USER_CANCELLED.
const USER_CANCELLED: HRESULT = HRESULT(0x8027_0000_u32 as i32);

/// Windows's own words for `code`, in the language it is set to; none when it has none.
pub fn system_message(code: HRESULT) -> Option<String> {
    let text = windows::core::Error::from_hresult(code).message();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// What Windows refuses in a file or folder name, to say before it is sent. A forbidden character is
/// wrong whatever follows; the rest only once `tried`, as typing "report.pdf" passes through "report.".
pub fn name_problem(name: &str, tried: bool) -> Option<NameProblem> {
    if name.chars().any(|c| c < ' ' || "\\/:*?\"<>|".contains(c)) {
        return Some(NameProblem::ForbiddenCharacter);
    }
    if !tried {
        return None;
    }
    if name.ends_with(['.', ' ']) {
        return Some(NameProblem::TrailingDotOrSpace);
    }
    if is_device_name(name) {
        return Some(NameProblem::Reserved);
    }
    None
}

/// A device's name, which the shell refuses as an item's whole name. As Explorer on Windows 11 does, one
/// with an extension passes (`con.txt`, `COM1.log`), though Windows 10 still took it for the device.
fn is_device_name(name: &str) -> bool {
    let name = name.to_uppercase();
    let numbered = |prefix: &str| {
        name.strip_prefix(prefix).is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"))
    };
    ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&name.as_str()) || numbered("COM") || numbered("LPT")
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name_problem(name, true).is_none() && name != "." && name != ".."
}

fn absolute(path: &Path) -> Result<&Path, OperationError> {
    if path.as_os_str().as_encoded_bytes().contains(&0) || !path.is_absolute() {
        return Err(OperationError::NotAbsolute);
    }
    Ok(path)
}

/// The real path, for telling whether a folder lies inside another.
fn normalized(path: &Path) -> Result<OsString, OperationError> {
    Ok(path_key(&std::fs::canonicalize(path)?))
}

/// Drops paths given twice and those inside another given, so nothing is acted on twice. Compares the
/// paths as written: their targets may not exist, as for a broken junction. Each path's folders are looked
/// up among the others, so a selection of thousands is not compared pair by pair.
fn collapse(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, OperationError> {
    let keys = paths
        .iter()
        .map(|p| {
            let path = std::path::absolute(absolute(p)?)?;
            let separated: Vec<u16> =
                path.as_os_str().encode_wide().map(|c| if c == u16::from(b'/') { u16::from(b'\\') } else { c }).collect();
            Ok(path_key(Path::new(&OsString::from_wide(&separated))))
        })
        .collect::<Result<Vec<OsString>, OperationError>>()?;
    let all: HashSet<&[u8]> = keys.iter().map(|key| key.as_encoded_bytes()).collect();
    let mut seen = HashSet::new();
    Ok(paths
        .into_iter()
        .zip(&keys)
        .filter(|(_, key)| {
            let key = key.as_encoded_bytes();
            let inside = key.iter().enumerate().any(|(at, &b)| b == b'\\' && at > 0 && all.contains(&key[..at]));
            !inside && seen.insert(key)
        })
        .map(|(p, _)| p)
        .collect())
}

/// A folder cannot go into itself or what it holds.
fn check_destination(paths: &[PathBuf], destination: &Path) -> Result<(), OperationError> {
    let target = absolute(destination)?;
    if !target.is_dir() {
        return Err(OperationError::NotAFolder);
    }
    let destination = normalized(target)?;
    for source in paths {
        let path = absolute(source)?;
        let source = normalized(path)?;
        let below = destination.as_encoded_bytes().strip_prefix(source.as_encoded_bytes()).is_some_and(|rest| rest.starts_with(b"\\"));
        if path.is_dir() && (destination == source || below) {
            return Err(OperationError::IntoItself);
        }
    }
    Ok(())
}

fn in_folder(path: &Path, folder: &Path) -> Result<bool, OperationError> {
    let Some(parent) = absolute(path)?.parent().map(Path::to_path_buf) else { return Ok(false) };
    Ok(normalized(&parent)? == normalized(absolute(folder)?)?)
}

/// The request as the shell takes it.
#[derive(Debug)]
struct Prepared {
    operation: Operation,
    paths: Vec<PathBuf>,
    /// A copy of items into their own folder, which the shell names " - コピー" as Explorer does.
    rename: bool,
    /// A paste of cut files, with the clipboard's sequence number as read.
    cut: Option<u32>,
}

/// A paste read from the clipboard, the request checked, and overlapping items collapsed.
fn prepare(
    request: Request,
    clipboard: impl FnOnce() -> Result<clipboard::ClipboardFiles, OperationError>,
) -> Result<Prepared, OperationError> {
    let Request { kind, mut paths, destination, name } = request;
    let mut cut = None;
    let mut copy = false;
    if kind == Kind::Paste {
        let files = clipboard()?;
        if files.paths.is_empty() {
            return Err(OperationError::NoFilesOnClipboard);
        }
        paths = files.paths;
        copy = !files.cut;
        cut = files.cut.then_some(files.sequence);
    }
    if kind != Kind::NewFolder && paths.is_empty() {
        return Err(OperationError::NothingSelected);
    }
    if matches!(kind, Kind::Rename | Kind::NewFolder) && !valid_name(&name) {
        return Err(OperationError::InvalidName);
    }
    if kind == Kind::Rename && paths.len() != 1 {
        return Err(OperationError::RenameOneAtATime);
    }
    for path in &paths {
        if absolute(path)?.parent().is_none() {
            return Err(OperationError::DriveRoot);
        }
    }
    let mut rename = false;
    if kind == Kind::Paste {
        check_destination(&paths, &destination)?;
        let inside = paths.iter().map(|p| in_folder(p, &destination)).collect::<Result<Vec<_>, _>>()?;
        if copy {
            rename = inside.iter().all(|&i| i);
        } else {
            // Already where it is going: nothing to move, as in Explorer.
            let mut inside = inside.into_iter();
            paths.retain(|_| !inside.next().unwrap_or(false));
        }
    }
    if kind == Kind::NewFolder && !absolute(&destination)?.is_dir() {
        return Err(OperationError::NoDestination);
    }
    let paths = collapse(paths)?;
    let operation = match kind {
        Kind::Paste if copy => Operation::Copy { destination },
        Kind::Paste => Operation::Move { destination },
        Kind::Recycle => Operation::Recycle,
        Kind::Delete => Operation::Delete,
        Kind::Rename => Operation::Rename { name },
        Kind::NewFolder => Operation::NewFolder { parent: destination, name },
    };
    Ok(Prepared { operation, paths, rename, cut })
}

/// Carries out `request`, telling `report` how each item ends; a paste pastes `read`, what was read from
/// the clipboard, or when `None` reads it now. Fails with [`OperationError::Cancelled`] when the user stopped it, and
/// says so when some items failed. Blocks until the shell is done.
pub fn run(
    request: Request,
    read: Option<clipboard::ClipboardFiles>,
    owner: Owner,
    report: impl FnMut(Event) + Send + 'static,
) -> Result<(), OperationError> {
    let files = match (request.kind, read) {
        (Kind::Paste, Some(files)) => files,
        (Kind::Paste, None) => clipboard::read_files().map_err(|e| OperationError::Windows(e.code()))?,
        _ => clipboard::ClipboardFiles::default(),
    };
    if request.kind == Kind::Paste && files.paths.is_empty() {
        if files.items {
            return paste_items(request.destination, files, owner, report);
        }
        if !files.contents.is_empty() {
            return paste_contents(request.destination, files, owner, report);
        }
    }
    let prepared = prepare(request, || Ok(files))?;
    // A cut pasted where it already is: nothing moves, and it stays cut.
    if matches!(prepared.operation, Operation::Move { .. }) && prepared.paths.is_empty() {
        return Ok(());
    }
    let tally = Arc::new(Tally::default());
    let leaves = matches!(prepared.operation, Operation::Move { .. } | Operation::Recycle | Operation::Delete | Operation::Rename { .. });
    let result = shell::operate(&prepared.operation, &prepared.paths, prepared.rename, owner, reporter(leaves, tally.clone(), report));
    let failed = tally.failed.load(Ordering::Relaxed);
    // Never empty a replaced clipboard, or one whose cut did not all go.
    if let Some(sequence) = prepared.cut
        && result.is_ok()
        && !failed
        && prepared.paths.iter().all(|p| !p.exists())
    {
        clipboard::complete(sequence);
    }
    outcome(result, failed)
}

/// A paste of shell items with no path, as from a ZIP folder or a phone, which the shell reads from the
/// clipboard itself. A cut is let go of once every item went, as there is no path to look for them by.
fn paste_items(
    destination: PathBuf,
    files: clipboard::ClipboardFiles,
    owner: Owner,
    report: impl FnMut(Event) + Send + 'static,
) -> Result<(), OperationError> {
    check_destination(&[], &destination)?;
    let operation = if files.cut { Operation::Move { destination } } else { Operation::Copy { destination } };
    let tally = Arc::new(Tally::default());
    let result = shell::operate_clipboard(&operation, files.sequence, owner, reporter(files.cut, tally.clone(), report));
    if files.cut && result.is_ok() && !tally.kept.load(Ordering::Relaxed) {
        clipboard::complete(files.sequence);
    }
    outcome(result, tally.failed.load(Ordering::Relaxed))
}

/// A paste of what is given only by its contents, as Outlook's attachments: dropped on the folder for the
/// shell to write out, as Explorer pastes it. Its items are told only by the names they were given.
fn paste_contents(
    destination: PathBuf,
    files: clipboard::ClipboardFiles,
    owner: Owner,
    mut report: impl FnMut(Event) + Send + 'static,
) -> Result<(), OperationError> {
    check_destination(&[], &destination)?;
    match shell::drop_clipboard(&destination, files.cut, files.sequence, owner) {
        Ok(Dropped::Refused) => Err(OperationError::Refused),
        Ok(dropped) => {
            for name in &files.contents {
                report(Event::Handed { path: child(&destination, name) });
            }
            if dropped == Dropped::Moved {
                clipboard::complete(files.sequence);
            }
            Ok(())
        }
        Err(error) => outcome(Err(error), false),
    }
}

/// How the items of an operation went.
#[derive(Default)]
struct Tally {
    failed: AtomicBool,
    /// Some item stayed where it was: failed, skipped or cancelled.
    kept: AtomicBool,
}

/// Tells `report` how each item the shell reports ends; with `leaves`, that those done are gone from where
/// they were.
fn reporter(leaves: bool, tally: Arc<Tally>, mut report: impl FnMut(Event) + Send + 'static) -> impl FnMut(Item) + 'static {
    move |item| {
        let left = match &item {
            Item::Done { path, result } if leaves && result.is_ok() && *result != PENDING && *result != USER_IGNORED => Some(path.clone()),
            _ => None,
        };
        let event = match item {
            Item::Done { result, .. } if result == PENDING => return,
            Item::Done { path, result } if result == USER_CANCELLED => Event::Cancelled { path },
            Item::Done { path, result } if result == USER_IGNORED => Event::Skipped { path },
            Item::Done { path, result } if result.is_err() => {
                tally.failed.store(true, Ordering::Relaxed);
                Event::Failed { path, error: result }
            }
            Item::Done { path, .. } => Event::Done { path },
            Item::Placed { path } => Event::Placed { path },
            Item::Destroyed { path } => Event::Destroyed { path },
        };
        if matches!(event, Event::Cancelled { .. } | Event::Skipped { .. } | Event::Failed { .. }) {
            tally.kept.store(true, Ordering::Relaxed);
        }
        report(event);
        if let Some(path) = left {
            report(Event::Left { path });
        }
    }
}

/// What the operation comes to, from the shell's result and whether any item failed.
fn outcome(result: windows::core::Result<()>, failed: bool) -> Result<(), OperationError> {
    match result {
        Err(error) if error.code() == E_ABORT || error.code() == USER_CANCELLED => Err(OperationError::Cancelled),
        Err(error) if error.code() == clipboard::CHANGED => Err(OperationError::ClipboardChanged),
        Err(error) => Err(OperationError::Windows(error.code())),
        Ok(()) if failed => Err(OperationError::PartlyFailed),
        Ok(()) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: Kind, paths: Vec<PathBuf>, destination: &Path, name: &str) -> Request {
        Request { kind, paths, destination: destination.to_owned(), name: name.into() }
    }

    fn files(paths: Vec<PathBuf>, cut: bool) -> impl FnOnce() -> Result<clipboard::ClipboardFiles, OperationError> {
        move || Ok(clipboard::ClipboardFiles { paths, cut, sequence: 1, ..Default::default() })
    }

    fn all(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn collapses_duplicates_and_what_lies_inside_another() {
        let paths = [r"D:\parent\child", r"D:\parent", r"d:/PARENT/", r"D:\parent-other", r"D:\sibling\..\parent"];
        assert_eq!(collapse(all(&paths)).unwrap(), all(&[r"D:\parent", r"D:\parent-other"]));
        let shares = [r"\\server\share\a\b", r"\\SERVER\share\a", r"\\server\share-other\a", r"D:\", r"D:\anything"];
        assert_eq!(collapse(all(&shares)).unwrap(), all(&[r"\\SERVER\share\a", r"\\server\share-other\a", r"D:\"]));
    }

    #[test]
    fn keeps_apart_a_name_that_is_not_text_and_its_replacement() {
        let folder = Path::new(r"D:\folder");
        let odd = crate::paths::tests::unpaired(folder, "a");
        let real = folder.join("a\u{FFFD}X");
        assert_eq!(collapse(vec![odd.clone(), real.clone()]).unwrap(), [odd.clone(), real]);
        assert_eq!(collapse(vec![odd.clone(), crate::paths::child(&odd, "inner")]).unwrap(), [odd]);
    }

    #[test]
    fn collapses_a_selection_of_thousands_at_once() {
        // Pair by pair, these would be 400 million comparisons before the shell is asked anything.
        let siblings: Vec<PathBuf> = (0..20_000).map(|i| format!(r"D:\folder\{i:05}.txt").into()).collect();
        assert_eq!(collapse(siblings.clone()).unwrap().len(), 20_000);
        let mut inside = siblings;
        inside.push(r"D:\folder".into());
        assert_eq!(collapse(inside).unwrap(), all(&[r"D:\folder"]));
    }

    #[test]
    fn copies_into_its_own_folder_under_a_new_name_and_moves_nothing_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        let (x, y) = (a.join("x.txt"), b.join("y.txt"));
        std::fs::write(&x, "").unwrap();
        std::fs::write(&y, "").unwrap();
        let p = |p: &Path| p.to_owned();
        let paste =
            |paths: Vec<PathBuf>, cut: bool, folder: &Path| prepare(request(Kind::Paste, vec![], folder, ""), files(paths, cut)).unwrap();
        assert!(paste(vec![p(&x)], false, &a).rename);
        // A copy from elsewhere among them still asks at a clash.
        assert!(!paste(vec![p(&x), p(&y)], false, &a).rename);
        assert!(!paste(vec![p(&x)], false, &b).rename);
        let moved = paste(vec![p(&x), p(&y)], true, &a);
        assert_eq!(moved.paths, [p(&y)]);
        assert_eq!(moved.cut, Some(1));
        assert!(paste(vec![p(&x)], true, &a).paths.is_empty());
    }

    #[test]
    fn refuses_a_folder_into_itself_drives_and_bad_names() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let p = dir.path().to_owned();
        assert!(prepare(request(Kind::Paste, vec![], &child, ""), files(vec![p.clone()], false)).is_err());
        assert!(prepare(request(Kind::Recycle, vec![r"C:\".into()], dir.path(), ""), files(vec![], false)).is_err());
        for name in ["..", "CON", "a/b", "a ", "a.", "LPT1", "a:b", ""] {
            assert!(prepare(request(Kind::NewFolder, vec![], dir.path(), name), files(vec![], false)).is_err(), "{name}");
        }
        assert!(prepare(request(Kind::NewFolder, vec![], dir.path(), "日本語 folder"), files(vec![], false)).is_ok());
        assert!(prepare(request(Kind::Paste, vec![], dir.path(), ""), files(vec![], false)).is_err());
    }

    #[test]
    fn says_what_is_wrong_with_a_name_as_it_is_typed_and_once_tried() {
        assert!(name_problem("a:b", false).is_some());
        assert_eq!(name_problem("report.", false), None);
        assert!(name_problem("report.", true).is_some());
        assert_eq!(name_problem("config", true), None);
    }

    /// As Explorer's rename took them on Windows 11 (26300): a device's name refused as the whole name,
    /// but not with an extension, nor COM0, LPT0 or CLOCK$.
    #[test]
    fn refuses_a_device_name_as_the_whole_name_as_the_shell_does() {
        for name in ["CON", "con", "PRN", "AUX", "NUL", "COM1", "COM9", "lpt9", "COM¹", "LPT²", "LPT³", "CONIN$", "conout$"] {
            assert_eq!(name_problem(name, true), Some(NameProblem::Reserved), "{name}");
            // Not yet a problem while it is typed: "CON" passes through on the way to "CONTRACT".
            assert_eq!(name_problem(name, false), None, "{name}");
        }
        for name in [
            "con.txt",
            "nul.txt",
            "NUL.tar.gz",
            "COM1.txt",
            "COM¹.txt",
            "LPT³.txt",
            "CONIN$.txt",
            "CON .txt",
            "COM0",
            "LPT0",
            "COM10",
            "CLOCK$",
        ] {
            assert_eq!(name_problem(name, true), None, "{name}");
        }
        // Ending in a dot or a space, the shell would trim it to the device's name: refused as such.
        assert!(name_problem("CON.", true).is_some());
        assert!(name_problem("CON ", true).is_some());
    }

    #[test]
    #[ignore = "replaces the real clipboard"]
    fn pastes_items_with_no_path_from_the_clipboard() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("archive.zip");
        shell::fixtures::zip(&archive, "inner.txt", b"zipped");
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        shell::fixtures::put_on_clipboard(&archive.join("inner.txt"));
        // Clipboard history and the like open it as it changes: let them be done with it.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let files = clipboard::read_files().unwrap();
        assert_eq!((files.paths.len(), files.items, files.cut), (0, true, false));
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = events.clone();
        run(request(Kind::Paste, vec![], &out, ""), None, 0, move |event| seen.lock().unwrap().push(event)).unwrap();
        assert_eq!(std::fs::read(out.join("inner.txt")).unwrap(), b"zipped");
        let events = events.lock().unwrap();
        assert!(matches!(&events[0], Event::Done { path } if path.ends_with(r"archive.zip\inner.txt")), "{events:?}");
        assert_eq!(events[1], Event::Placed { path: out.join("inner.txt") });
        assert_eq!(events.len(), 2, "{events:?}");
    }

    #[test]
    #[ignore = "replaces the real clipboard"]
    fn pastes_what_is_given_only_by_its_contents_from_the_clipboard() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().to_path_buf();
        let (events, files) = shell::fixtures::while_on_clipboard("添付.txt", b"attached", move || {
            let files = clipboard::read_files().unwrap();
            let events = Arc::new(std::sync::Mutex::new(Vec::new()));
            let seen = events.clone();
            run(request(Kind::Paste, vec![], &destination, ""), None, 0, move |event| seen.lock().unwrap().push(event)).unwrap();
            let events = events.lock().unwrap().clone();
            (events, files)
        });
        assert_eq!((files.paths.len(), files.items, files.contents), (0, false, vec![OsString::from("添付.txt")]));
        assert_eq!(std::fs::read(dir.path().join("添付.txt")).unwrap(), b"attached");
        assert_eq!(events, [Event::Handed { path: dir.path().join("添付.txt") }]);
    }

    #[test]
    #[ignore = "sends a temporary folder to the real Recycle Bin"]
    fn makes_renames_and_sends_to_the_bin_through_the_shell() {
        let dir = tempfile::Builder::new().prefix("colet-shell-").tempdir().unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = events.clone();
        let record = move |event| seen.lock().unwrap().push(event);
        run(request(Kind::NewFolder, vec![], dir.path(), "child"), None, 0, record.clone()).unwrap();
        let child = dir.path().join("child");
        assert!(child.is_dir());
        run(request(Kind::Rename, vec![child.clone()], dir.path(), "renamed"), None, 0, record.clone()).unwrap();
        let renamed = dir.path().join("renamed");
        assert!(renamed.is_dir());
        run(request(Kind::Recycle, vec![renamed.clone()], dir.path(), ""), None, 0, record).unwrap();
        assert!(!renamed.exists());
        let events = events.lock().unwrap();
        let done = events.iter().filter(|e| matches!(e, Event::Done { .. })).count();
        let left: Vec<_> = events.iter().filter_map(|e| if let Event::Left { path } = e { Some(path) } else { None }).collect();
        assert_eq!((done, events.len()), (3, 6), "{events:?}");
        assert!(events.contains(&Event::Placed { path: renamed.clone() }), "{events:?}");
        // The new folder was nowhere before; the renamed one and the one sent to the bin left.
        assert_eq!(left, [&child, &renamed]);
    }
}
