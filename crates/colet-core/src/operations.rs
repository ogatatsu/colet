//! File operations as Colet asks for them: a paste from the clipboard, sending to the Recycle Bin, a
//! rename, a new folder. Checked here, carried out by the shell with its own dialogs.
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use windows::Win32::Foundation::E_ABORT;
use windows::core::HRESULT;

use crate::shell::{self, Item, Operation, Owner, clipboard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// What the clipboard holds, copied or moved into `destination`.
    Paste,
    /// To the Recycle Bin, never deleted outright.
    Recycle,
    Rename,
    NewFolder,
}

impl Kind {
    /// What the history and the notices call it.
    pub fn title(self) -> &'static str {
        match self {
            Kind::Paste => "貼り付け",
            Kind::Recycle => "ごみ箱へ移動",
            Kind::Rename => "名前を変更",
            Kind::NewFolder => "フォルダーを作成",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub kind: Kind,
    /// What it acts on; a paste takes them from the clipboard.
    pub paths: Vec<String>,
    /// Where a paste goes, or where a new folder is made.
    pub destination: String,
    /// A rename's new name, or the new folder's.
    pub name: String,
}

/// How one item ended, or where a copy or move put it.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Done { path: String },
    Failed { path: String, error: String },
    /// Passed over at the user's word, as from a name clash.
    Skipped { path: String },
    /// Stopped at the user's word.
    Cancelled { path: String },
    /// Where the last item went, which a name clash may have renamed.
    Placed { path: String },
    /// The last item done is no longer where it was: moved, renamed or sent to the Recycle Bin. A copy's
    /// source stays, and a new folder was nowhere before.
    Left { path: String },
}

/// What an operation the user stopped fails with.
pub const CANCELLED: &str = "操作をキャンセルしました";

/// COPYENGINE_S_USER_IGNORED: skipped at the user's word.
const USER_IGNORED: HRESULT = HRESULT(0x0027_0005);
/// COPYENGINE_S_PENDING: waiting on a question; not its end yet.
const PENDING: HRESULT = HRESULT(0x0027_000B);
/// COPYENGINE_E_USER_CANCELLED.
const USER_CANCELLED: HRESULT = HRESULT(0x8027_0000_u32 as i32);

/// Windows's own words for `code`, or its number.
pub fn message(code: HRESULT) -> String {
    let text = windows::core::Error::from_hresult(code).message();
    let text = text.trim();
    if text.is_empty() { format!("Windowsの操作に失敗しました (0x{:08X})", code.0 as u32) } else { text.to_owned() }
}

/// What Windows refuses in a file or folder name, to say before it is sent. A forbidden character is
/// wrong whatever follows; the rest only once `tried`, as typing "report.pdf" passes through "report.".
pub fn name_problem(name: &str, tried: bool) -> Option<&'static str> {
    if name.chars().any(|c| c < ' ' || "\\/:*?\"<>|".contains(c)) {
        return Some("次の文字は使えません：\\ / : * ? \" < > |");
    }
    if !tried {
        return None;
    }
    if name.ends_with(['.', ' ']) {
        return Some("末尾にピリオドや空白は使えません");
    }
    let stem = name.trim().split('.').next().unwrap_or("").to_uppercase();
    let numbered = stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(stem.as_bytes()[3], b'1'..=b'9');
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str()) || numbered {
        return Some("Windowsで予約された名前は使えません");
    }
    None
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name_problem(name, true).is_none() && name != "." && name != ".."
}

fn absolute(path: &str) -> Result<PathBuf, String> {
    if path.contains('\0') || !Path::new(path).is_absolute() {
        return Err("絶対パスを指定してください".into());
    }
    Ok(PathBuf::from(path))
}

/// The real path, for telling whether a folder lies inside another.
fn normalized(path: &Path) -> Result<String, String> {
    Ok(std::fs::canonicalize(path).map_err(|e| e.to_string())?.to_string_lossy().trim_end_matches('\\').to_lowercase())
}

/// Drops paths given twice and those inside another given, so nothing is acted on twice. Compares the
/// paths as written: their targets may not exist, as for a broken junction.
fn collapse(paths: Vec<String>) -> Result<Vec<String>, String> {
    let keys = paths
        .iter()
        .map(|p| {
            let path = std::path::absolute(absolute(p)?).map_err(|e| e.to_string())?;
            Ok(path.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase())
        })
        .collect::<Result<Vec<String>, String>>()?;
    Ok(paths
        .into_iter()
        .enumerate()
        .filter(|(i, _)| {
            !keys.iter().enumerate().any(|(j, other)| {
                j != *i && (keys[*i].starts_with(&format!("{other}\\")) || (j < *i && keys[*i] == *other))
            })
        })
        .map(|(_, p)| p)
        .collect())
}

/// A folder cannot go into itself or what it holds.
fn check_destination(paths: &[String], destination: &str) -> Result<(), String> {
    let target = absolute(destination)?;
    if !target.is_dir() {
        return Err("貼り付け先がフォルダーではありません".into());
    }
    let destination = normalized(&target)?;
    for source in paths {
        let path = absolute(source)?;
        let source = normalized(&path)?;
        if path.is_dir() && (destination == source || destination.starts_with(&format!("{source}\\"))) {
            return Err("フォルダー自身やその子孫にはコピー・移動できません".into());
        }
    }
    Ok(())
}

fn in_folder(path: &str, folder: &str) -> Result<bool, String> {
    let Some(parent) = absolute(path)?.parent().map(Path::to_path_buf) else { return Ok(false) };
    Ok(normalized(&parent)? == normalized(&absolute(folder)?)?)
}

/// The request as the shell takes it.
#[derive(Debug)]
struct Prepared {
    operation: Operation,
    paths: Vec<String>,
    /// A copy of items into their own folder, which the shell names " - コピー" as Explorer does.
    rename: bool,
    /// A paste of cut files, with the clipboard's sequence number as read.
    cut: Option<u32>,
}

/// A paste read from the clipboard, the request checked, and overlapping items collapsed.
fn prepare(request: Request, clipboard: impl FnOnce() -> Result<clipboard::ClipboardFiles, String>) -> Result<Prepared, String> {
    let Request { kind, mut paths, destination, name } = request;
    let mut cut = None;
    let mut copy = false;
    if kind == Kind::Paste {
        let files = clipboard()?;
        if files.paths.is_empty() {
            return Err("クリップボードにファイルがありません".into());
        }
        paths = files.paths;
        copy = !files.cut;
        cut = files.cut.then_some(files.sequence);
    }
    if kind != Kind::NewFolder && paths.is_empty() {
        return Err("項目が選択されていません".into());
    }
    if matches!(kind, Kind::Rename | Kind::NewFolder) && !valid_name(&name) {
        return Err("使用できない名前です".into());
    }
    if kind == Kind::Rename && paths.len() != 1 {
        return Err("名前の変更は1項目ずつ行ってください".into());
    }
    for path in &paths {
        if absolute(path)?.parent().is_none() {
            return Err("ドライブのルートは操作できません".into());
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
        return Err("作成先が見つかりません".into());
    }
    let paths = collapse(paths)?;
    let operation = match kind {
        Kind::Paste if copy => Operation::Copy { destination },
        Kind::Paste => Operation::Move { destination },
        Kind::Recycle => Operation::Recycle,
        Kind::Rename => Operation::Rename { name },
        Kind::NewFolder => Operation::NewFolder { parent: destination, name },
    };
    Ok(Prepared { operation, paths, rename, cut })
}

/// Carries out `request`, telling `report` how each item ends. Fails with [`CANCELLED`] when the user
/// stopped it, and says so when some items failed. Blocks until the shell is done.
pub fn run(request: Request, owner: Owner, report: impl FnMut(Event) + Send + 'static) -> Result<(), String> {
    let files = match request.kind {
        Kind::Paste => clipboard::read_files().map_err(|e| message(e.code()))?,
        _ => clipboard::ClipboardFiles::default(),
    };
    if request.kind == Kind::Paste && files.paths.is_empty() && files.items {
        return paste_items(request.destination, files, owner, report);
    }
    let prepared = prepare(request, || Ok(files))?;
    // A cut pasted where it already is: nothing moves, and it stays cut.
    if matches!(prepared.operation, Operation::Move { .. }) && prepared.paths.is_empty() {
        return Ok(());
    }
    let tally = Arc::new(Tally::default());
    let leaves = matches!(prepared.operation, Operation::Move { .. } | Operation::Recycle | Operation::Rename { .. });
    let result =
        shell::operate(&prepared.operation, &prepared.paths, prepared.rename, owner, reporter(leaves, tally.clone(), report));
    let failed = tally.failed.load(Ordering::Relaxed);
    // Never empty a replaced clipboard, or one whose cut did not all go.
    if let Some(sequence) = prepared.cut
        && result.is_ok()
        && !failed
        && prepared.paths.iter().all(|p| !Path::new(p).exists())
    {
        clipboard::complete(sequence);
    }
    outcome(result, failed)
}

/// A paste of shell items with no path, as from a ZIP folder or a phone, which the shell reads from the
/// clipboard itself. A cut is let go of once every item went, as there is no path to look for them by.
fn paste_items(
    destination: String,
    files: clipboard::ClipboardFiles,
    owner: Owner,
    report: impl FnMut(Event) + Send + 'static,
) -> Result<(), String> {
    check_destination(&[], &destination)?;
    let operation = if files.cut { Operation::Move { destination } } else { Operation::Copy { destination } };
    let tally = Arc::new(Tally::default());
    let result = shell::operate_clipboard(&operation, owner, reporter(files.cut, tally.clone(), report));
    if files.cut && result.is_ok() && !tally.kept.load(Ordering::Relaxed) {
        clipboard::complete(files.sequence);
    }
    outcome(result, tally.failed.load(Ordering::Relaxed))
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
            Item::Done { path, result } if leaves && result.is_ok() && *result != PENDING && *result != USER_IGNORED => {
                Some(path.clone())
            }
            _ => None,
        };
        let event = match item {
            Item::Done { result, .. } if result == PENDING => return,
            Item::Done { path, result } if result == USER_CANCELLED => Event::Cancelled { path },
            Item::Done { path, result } if result == USER_IGNORED => Event::Skipped { path },
            Item::Done { path, result } if result.is_err() => {
                tally.failed.store(true, Ordering::Relaxed);
                Event::Failed { path, error: message(result) }
            }
            Item::Done { path, .. } => Event::Done { path },
            Item::Placed { path } => Event::Placed { path },
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
fn outcome(result: windows::core::Result<()>, failed: bool) -> Result<(), String> {
    match result {
        Err(error) if error.code() == E_ABORT || error.code() == USER_CANCELLED => Err(CANCELLED.into()),
        Err(error) => Err(message(error.code())),
        Ok(()) if failed => Err("一部の項目を操作できませんでした。結果を確認してください。".into()),
        Ok(()) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: Kind, paths: Vec<String>, destination: &Path, name: &str) -> Request {
        Request { kind, paths, destination: destination.display().to_string(), name: name.into() }
    }

    fn files(paths: Vec<String>, cut: bool) -> impl FnOnce() -> Result<clipboard::ClipboardFiles, String> {
        move || Ok(clipboard::ClipboardFiles { paths, cut, sequence: 1, ..Default::default() })
    }

    #[test]
    fn collapses_duplicates_and_what_lies_inside_another() {
        let paths = [r"D:\parent\child", r"D:\parent", r"d:/PARENT/", r"D:\parent-other", r"D:\sibling\..\parent"];
        assert_eq!(collapse(paths.iter().map(|p| p.to_string()).collect()).unwrap(), [r"D:\parent", r"D:\parent-other"]);
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
        let p = |p: &Path| p.display().to_string();
        let paste = |paths: Vec<String>, cut: bool, folder: &Path| {
            prepare(request(Kind::Paste, vec![], folder, ""), files(paths, cut)).unwrap()
        };
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
        let p = dir.path().display().to_string();
        assert!(prepare(request(Kind::Paste, vec![], &child, ""), files(vec![p.clone()], false)).is_err());
        assert!(prepare(request(Kind::Recycle, vec![r"C:\".into()], dir.path(), ""), files(vec![], false)).is_err());
        for name in ["..", "CON.txt", "a/b", "a ", "a.", "LPT1", "a:b", ""] {
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
        assert!(name_problem("con", true).is_some());
        assert!(name_problem("COM1.txt", true).is_some());
        assert_eq!(name_problem("COM0", true), None);
        assert_eq!(name_problem("config", true), None);
    }

    #[test]
    #[ignore = "replaces the real clipboard"]
    fn pastes_items_with_no_path_from_the_clipboard() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("archive.zip");
        shell::fixtures::zip(&archive, "inner.txt", b"zipped");
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        shell::fixtures::put_on_clipboard(&format!(r"{}\inner.txt", archive.display()));
        // Clipboard history and the like open it as it changes: let them be done with it.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let files = clipboard::read_files().unwrap();
        assert_eq!((files.paths.len(), files.items, files.cut), (0, true, false));
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = events.clone();
        run(request(Kind::Paste, vec![], &out, ""), 0, move |event| seen.lock().unwrap().push(event)).unwrap();
        assert_eq!(std::fs::read(out.join("inner.txt")).unwrap(), b"zipped");
        let events = events.lock().unwrap();
        assert!(matches!(&events[0], Event::Done { path } if path.ends_with(r"archive.zip\inner.txt")), "{events:?}");
        assert_eq!(events[1], Event::Placed { path: out.join("inner.txt").display().to_string() });
        assert_eq!(events.len(), 2, "{events:?}");
    }

    #[test]
    #[ignore = "sends a temporary folder to the real Recycle Bin"]
    fn makes_renames_and_sends_to_the_bin_through_the_shell() {
        let dir = tempfile::Builder::new().prefix("colet-shell-").tempdir().unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = events.clone();
        let record = move |event| seen.lock().unwrap().push(event);
        run(request(Kind::NewFolder, vec![], dir.path(), "child"), 0, record.clone()).unwrap();
        let child = dir.path().join("child");
        assert!(child.is_dir());
        run(request(Kind::Rename, vec![child.display().to_string()], dir.path(), "renamed"), 0, record.clone()).unwrap();
        let renamed = dir.path().join("renamed");
        assert!(renamed.is_dir());
        run(request(Kind::Recycle, vec![renamed.display().to_string()], dir.path(), ""), 0, record).unwrap();
        assert!(!renamed.exists());
        let events = events.lock().unwrap();
        let done = events.iter().filter(|e| matches!(e, Event::Done { .. })).count();
        let left: Vec<_> = events.iter().filter_map(|e| if let Event::Left { path } = e { Some(path) } else { None }).collect();
        assert_eq!((done, events.len()), (3, 5), "{events:?}");
        // The new folder was nowhere before; the renamed one and the one sent to the bin left.
        assert_eq!(left, [&child.display().to_string(), &renamed.display().to_string()]);
    }
}
