//! File operations through `IFileOperation`, so name clashes, errors, progress and cancelling are the
//! shell's own dialogs, owned by our window.
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{E_ABORT, E_INVALIDARG};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, IDataObject};
use windows::Win32::UI::Shell::{
    FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_RENAMEONCOLLISION, FOF_WANTNUKEWARNING, FOFX_ADDUNDORECORD, FOFX_RECYCLEONDELETE,
    FileOperation, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl, IShellItem, IShellItemArray,
    SHCreateItemFromParsingName, SHCreateShellItemArrayFromDataObject, SHELLFLAGSTATE, SHGetSettings, SIGDN_DESKTOPABSOLUTEEDITING,
    SIGDN_FILESYSPATH, SSF_NOCONFIRMRECYCLE,
};
use windows::core::{AsImpl, Error, HRESULT, PCWSTR, Ref, Result, implement};

use super::{Apartment, Ole, Owner, Wide};
use crate::paths::from_wide;

#[derive(Clone, Debug)]
pub enum Operation {
    Copy {
        destination: PathBuf,
    },
    Move {
        destination: PathBuf,
    },
    /// To the Recycle Bin, as Explorer's Delete: asked first only as its setting says, and an item the
    /// bin cannot take is deleted outright once the shell has asked.
    Recycle,
    /// Deleted outright, as Explorer's Shift+Delete, once the shell has asked.
    Delete,
    Rename {
        name: String,
    },
    NewFolder {
        parent: PathBuf,
        name: String,
    },
}

/// What happened to one item.
#[derive(Debug, PartialEq)]
pub enum Item {
    /// How the item ended.
    Done { path: PathBuf, result: HRESULT },
    /// Where a copy, move or rename put it, under the name it got, which may not be the one asked.
    Placed { path: PathBuf },
    /// The item just deleted went for good, not to the Recycle Bin.
    Destroyed { path: PathBuf },
}

/// Carries out `operation` on `paths`, telling `report` how each item ends. `rename`: a copy into its own
/// folder, which gets " - コピー" names rather than a clash. `Err(E_ABORT)` when the user cancelled.
/// Blocks until the shell is done, so never call it from the thread that owns `owner`.
pub fn operate(operation: &Operation, paths: &[PathBuf], rename: bool, owner: Owner, report: impl FnMut(Item) + 'static) -> Result<()> {
    perform(operation, rename, owner, report, |op, sink| queue(op, operation, paths, sink))
}

/// Copies or moves the shell items on the clipboard into `operation`'s destination: those with no path,
/// as from a ZIP folder or a phone, which the shell reads from the clipboard's data object itself. Items
/// with no path are told by the name the address bar shows; only while the clipboard still holds what was
/// read as `sequence`. Blocks as [`operate`] does.
pub fn operate_clipboard(operation: &Operation, sequence: u32, owner: Owner, report: impl FnMut(Item) + 'static) -> Result<()> {
    operate_data(operation, owner, report, || super::clipboard::data(sequence))
}

fn operate_data(
    operation: &Operation,
    owner: Owner,
    report: impl FnMut(Item) + 'static,
    data: impl FnOnce() -> Result<IDataObject>,
) -> Result<()> {
    let _ole = Ole::enter()?;
    perform(operation, false, owner, report, |op, _| {
        let items: IShellItemArray = unsafe { SHCreateShellItemArrayFromDataObject(&data()?)? };
        unsafe {
            match operation {
                Operation::Copy { destination } => op.CopyItems(&items, &item(destination)?),
                Operation::Move { destination } => op.MoveItems(&items, &item(destination)?),
                _ => Err(Error::from_hresult(E_INVALIDARG)),
            }
        }
    })
}

/// Sets the shell's file operation up, has `queue` put the items in, and carries them out.
fn perform(
    operation: &Operation,
    rename: bool,
    owner: Owner,
    report: impl FnMut(Item) + 'static,
    queue: impl FnOnce(&IFileOperation, &IFileOperationProgressSink) -> Result<()>,
) -> Result<()> {
    let apartment = Apartment::enter();
    apartment.result()?;
    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL)? };
    unsafe { op.SetOwnerWindow(HWND(owner as *mut _))? };
    // Do not suppress clash dialogs or approve destructive decisions on the user's behalf: deleting
    // outright is always asked, even where sending to the bin is not.
    let mut flags = FOFX_ADDUNDORECORD | FOF_NOCONFIRMMKDIR;
    if !matches!(operation, Operation::Delete) {
        flags |= FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING;
    }
    if matches!(operation, Operation::Recycle) && !confirms_recycle() {
        flags |= FOF_NOCONFIRMATION;
    }
    if rename {
        flags |= FOF_RENAMEONCOLLISION;
    }
    unsafe { op.SetOperationFlags(flags)? };
    let sink: IFileOperationProgressSink = Sink(RefCell::new(Box::new(report))).into();
    let cookie = unsafe { op.Advise(&sink)? };
    let queued = queue(&op, &sink);
    let result = queued.and_then(|()| unsafe { op.PerformOperations() });
    let aborted = unsafe { op.GetAnyOperationsAborted() }.is_ok_and(|aborted| aborted.as_bool());
    unsafe {
        let _ = op.Unadvise(cookie);
    }
    match result {
        Ok(()) if aborted => Err(Error::from_hresult(E_ABORT)),
        result => result,
    }
}

/// Whether the Recycle Bin's "Display delete confirmation dialog" is on, which Explorer asks by.
fn confirms_recycle() -> bool {
    let mut state = SHELLFLAGSTATE::default();
    unsafe { SHGetSettings(&mut state, SSF_NOCONFIRMRECYCLE.0) };
    // fNoConfirmRecycle, the third bit.
    state._bitfield & 0b100 == 0
}

fn item(path: &Path) -> Result<IShellItem> {
    let path = Wide::new(path);
    unsafe { SHCreateItemFromParsingName(path.get(), None) }
}

fn queue(op: &IFileOperation, operation: &Operation, paths: &[PathBuf], sink: &IFileOperationProgressSink) -> Result<()> {
    let target = match operation {
        Operation::Copy { destination } | Operation::Move { destination } => Some(item(destination)?),
        Operation::NewFolder { parent, name } => {
            let name = Wide::new(name);
            unsafe { op.NewItem(&item(parent)?, FILE_ATTRIBUTE_DIRECTORY.0, name.get(), PCWSTR::null(), None)? };
            None
        }
        _ => None,
    };
    for path in paths {
        let source = item(path).inspect_err(|error| {
            let sink = unsafe { sink.as_impl() };
            sink.report(Item::Done { path: path.clone(), result: error.code() });
        })?;
        unsafe {
            match operation {
                Operation::Copy { .. } => op.CopyItem(&source, target.as_ref(), PCWSTR::null(), None)?,
                Operation::Move { .. } => op.MoveItem(&source, target.as_ref(), PCWSTR::null(), None)?,
                Operation::Recycle | Operation::Delete => op.DeleteItem(&source, None)?,
                Operation::Rename { name } => {
                    let name = Wide::new(name);
                    op.RenameItem(&source, name.get(), None)?
                }
                Operation::NewFolder { .. } => {}
            }
        }
    }
    Ok(())
}

#[implement(IFileOperationProgressSink)]
struct Sink(RefCell<Box<dyn FnMut(Item)>>);

impl Sink {
    fn report(&self, item: Item) {
        (self.0.borrow_mut())(item);
    }
    fn done(&self, item: Ref<IShellItem>, result: HRESULT) -> Result<()> {
        self.report(Item::Done { path: path(item.as_ref()), result });
        Ok(())
    }
    fn placed(&self, item: Ref<IShellItem>, result: HRESULT, made: Ref<IShellItem>) -> Result<()> {
        self.done(item, result)?;
        if finished(result)
            && let Some(made) = made.as_ref()
        {
            self.report(Item::Placed { path: path(Some(made)) });
        }
        Ok(())
    }
}

/// The item went as asked: a success, but not COPYENGINE_S_USER_IGNORED (skipped) nor
/// COPYENGINE_S_PENDING (waiting on a question), which leave it where it was.
fn finished(result: HRESULT) -> bool {
    const USER_IGNORED: HRESULT = HRESULT(0x0027_0005);
    const PENDING: HRESULT = HRESULT(0x0027_000B);
    result.is_ok() && result != USER_IGNORED && result != PENDING
}

/// The item's path, or for one with none, as in a ZIP folder or on a phone, its name in the address bar.
fn path(item: Option<&IShellItem>) -> PathBuf {
    let Some(item) = item else { return PathBuf::new() };
    let named = |form| unsafe { item.GetDisplayName(form) };
    let Ok(name) = named(SIGDN_FILESYSPATH).or_else(|_| named(SIGDN_DESKTOPABSOLUTEEDITING)) else {
        return PathBuf::new();
    };
    let path = from_wide(unsafe { name.as_wide() });
    unsafe { CoTaskMemFree(Some(name.0 as _)) };
    path
}

#[allow(non_snake_case)]
impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn StartOperations(&self) -> Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: HRESULT) -> Result<()> {
        Ok(())
    }
    fn PreRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn PostRenameItem(&self, _: u32, item: Ref<IShellItem>, _: &PCWSTR, result: HRESULT, renamed: Ref<IShellItem>) -> Result<()> {
        // Under the name it got, which may not be the one asked.
        self.placed(item, result, renamed)
    }
    fn PreMoveItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        item: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        result: HRESULT,
        made: Ref<IShellItem>,
    ) -> Result<()> {
        self.placed(item, result, made)
    }
    fn PreCopyItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        item: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        result: HRESULT,
        made: Ref<IShellItem>,
    ) -> Result<()> {
        self.placed(item, result, made)
    }
    fn PreDeleteItem(&self, _: u32, _: Ref<IShellItem>) -> Result<()> {
        Ok(())
    }
    fn PostDeleteItem(&self, _: u32, item: Ref<IShellItem>, result: HRESULT, recycled: Ref<IShellItem>) -> Result<()> {
        let gone = path(item.as_ref());
        self.done(item, result)?;
        // Sent to the bin, the shell hands the item there; deleted outright, nothing. One skipped or still
        // waiting on a question is a success code too, yet stays where it was.
        if finished(result) && recycled.is_null() {
            self.report(Item::Destroyed { path: gone });
        }
        Ok(())
    }
    fn PreNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        result: HRESULT,
        made: Ref<IShellItem>,
    ) -> Result<()> {
        self.done(made, result)
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::Path;

    fn crc32(bytes: &[u8]) -> u32 {
        !bytes.iter().fold(!0u32, |crc, &byte| {
            (0..8).fold(crc ^ byte as u32, |crc, _| if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 })
        })
    }

    /// A ZIP file at `path` storing `name` holding `content`, uncompressed: a folder whose items have no path.
    pub fn zip(path: &Path, name: &str, content: &[u8]) {
        let (crc, size, name_len) = (crc32(content), content.len() as u32, name.len() as u16);
        // Version 2.0, no flags, stored, 1980-01-01.
        let common = |out: &mut Vec<u8>| {
            for half in [20u16, 0, 0, 0, 0x21] {
                out.extend(half.to_le_bytes());
            }
            for word in [crc, size, size] {
                out.extend(word.to_le_bytes());
            }
            out.extend(name_len.to_le_bytes());
        };
        let mut out = 0x0403_4b50u32.to_le_bytes().to_vec();
        common(&mut out);
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(content);
        let directory = out.len() as u32;
        out.extend(0x0201_4b50u32.to_le_bytes());
        out.extend(20u16.to_le_bytes());
        common(&mut out);
        // No extra field, comment, disk or attributes; the local header at the start.
        out.extend([0u8; 16]);
        out.extend(name.as_bytes());
        let directory_size = out.len() as u32 - directory;
        out.extend(0x0605_4b50u32.to_le_bytes());
        for half in [0u16, 0, 1, 1] {
            out.extend(half.to_le_bytes());
        }
        out.extend(directory_size.to_le_bytes());
        out.extend(directory.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        std::fs::write(path, out).unwrap();
    }

    /// Puts the shell item at `path` on the clipboard as Explorer would, as a copy, replacing what it held.
    pub fn put_on_clipboard(path: &Path) {
        use windows::Win32::System::Com::IDataObject;
        use windows::Win32::System::Ole::{OleFlushClipboard, OleSetClipboard};
        use windows::Win32::UI::Shell::BHID_DataObject;

        let _ole = super::Ole::enter().unwrap();
        unsafe {
            let data: IDataObject = super::item(path).unwrap().BindToHandler(None, &BHID_DataObject).unwrap();
            OleSetClipboard(&data).unwrap();
            // Left behind on the clipboard once this thread lets OLE go.
            OleFlushClipboard().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use windows::Win32::Foundation::{E_ACCESSDENIED, S_OK};
    use windows::Win32::UI::Shell::BHID_DataObject;

    #[test]
    fn copies_items_with_no_path_and_names_them_as_the_address_bar_does() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("archive.zip");
        fixtures::zip(&archive, "inner.txt", b"zipped");
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let inner = archive.join("inner.txt");
        let reports = Rc::new(RefCell::new(Vec::new()));
        let seen = reports.clone();
        operate_data(
            &Operation::Copy { destination: out.clone() },
            0,
            move |item| seen.borrow_mut().push(item),
            || unsafe { item(&inner)?.BindToHandler(None, &BHID_DataObject) },
        )
        .unwrap();
        assert_eq!(std::fs::read(out.join("inner.txt")).unwrap(), b"zipped");
        let reports = reports.borrow();
        assert!(
            matches!(&reports[0], Item::Done { path, result } if result.is_ok() && path.ends_with(r"archive.zip\inner.txt")),
            "{reports:?}"
        );
        assert_eq!(reports[1], Item::Placed { path: out.join("inner.txt") });
    }

    #[test]
    fn says_which_deleted_items_went_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        let a = item(&file).unwrap();
        let reports = Rc::new(RefCell::new(Vec::new()));
        let seen = reports.clone();
        let sink: IFileOperationProgressSink = Sink(RefCell::new(Box::new(move |item| seen.borrow_mut().push(item)))).into();
        unsafe {
            sink.PostDeleteItem(0, &a, S_OK, &a).unwrap();
            sink.PostDeleteItem(0, &a, S_OK, None).unwrap();
            sink.PostDeleteItem(0, &a, E_ACCESSDENIED, None).unwrap();
        }
        let reports = reports.borrow();
        assert_eq!(reports.iter().filter(|i| matches!(i, Item::Destroyed { .. })).count(), 1, "{reports:?}");
        assert_eq!(reports[2], Item::Destroyed { path: file.clone() });
    }

    #[test]
    fn says_what_a_rename_named_it_whatever_was_asked() {
        let dir = tempfile::tempdir().unwrap();
        let (file, got) = (dir.path().join("a.txt"), dir.path().join("b (2).txt"));
        std::fs::write(&file, "a").unwrap();
        std::fs::write(&got, "b").unwrap();
        let (a, b) = (item(&file).unwrap(), item(&got).unwrap());
        let asked = Wide::new(Path::new("b.txt"));
        let reports = Rc::new(RefCell::new(Vec::new()));
        let seen = reports.clone();
        let sink: IFileOperationProgressSink = Sink(RefCell::new(Box::new(move |item| seen.borrow_mut().push(item)))).into();
        unsafe {
            sink.PostRenameItem(0, &a, asked.get(), S_OK, &b).unwrap();
            sink.PostRenameItem(0, &a, asked.get(), E_ACCESSDENIED, None).unwrap();
        }
        let reports = reports.borrow();
        assert_eq!(reports[1], Item::Placed { path: got });
        assert_eq!(reports.len(), 3, "{reports:?}");
    }

    #[test]
    fn renames_and_says_where_it_went() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        let reports = Rc::new(RefCell::new(Vec::new()));
        let seen = reports.clone();
        operate(&Operation::Rename { name: "b.txt".into() }, std::slice::from_ref(&file), false, 0, move |item| {
            seen.borrow_mut().push(item)
        })
        .unwrap();
        let reports = reports.borrow();
        assert!(matches!(&reports[0], Item::Done { result, .. } if result.is_ok()), "{reports:?}");
        assert_eq!(reports[1], Item::Placed { path: dir.path().join("b.txt") });
    }

    #[test]
    fn copies_and_says_where_each_item_went() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        let reports = Rc::new(RefCell::new(Vec::new()));
        let seen = reports.clone();
        operate(&Operation::Copy { destination: dir.path().to_owned() }, std::slice::from_ref(&file), true, 0, move |item| {
            seen.borrow_mut().push(item)
        })
        .unwrap();
        let reports = reports.borrow();
        assert!(matches!(&reports[0], Item::Done { result, .. } if result.is_ok()), "{reports:?}");
        let Item::Placed { path } = &reports[1] else { panic!("{reports:?}") };
        assert_ne!(path, &file);
        assert!(path.exists());
    }
}
