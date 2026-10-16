//! File operations through `IFileOperation`, so name clashes, errors, progress and cancelling are the
//! shell's own dialogs, owned by our window.
use std::cell::RefCell;

use windows::Win32::Foundation::{E_ABORT, ERROR_NOT_SUPPORTED};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    FileOperation, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_RENAMEONCOLLISION, FOFX_ADDUNDORECORD,
    FOFX_RECYCLEONDELETE, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl,
    IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH, TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows::Win32::Foundation::HWND;
use windows::core::{AsImpl, Error, HRESULT, PCWSTR, Ref, Result, implement};

use super::{Apartment, Owner, Wide};

#[derive(Clone, Debug)]
pub enum Operation {
    Copy { destination: String },
    Move { destination: String },
    /// To the Recycle Bin only: an item the bin cannot take is refused, never deleted outright.
    Recycle,
    Rename { name: String },
    NewFolder { parent: String, name: String },
}

/// What happened to one item.
#[derive(Debug, PartialEq)]
pub enum Item {
    /// How the item ended.
    Done { path: String, result: HRESULT },
    /// Where a copy or move put it, which a name clash may have renamed.
    Placed { path: String },
}

/// Carries out `operation` on `paths`, telling `report` how each item ends. `rename`: a copy into its own
/// folder, which gets " - コピー" names rather than a clash. `Err(E_ABORT)` when the user cancelled.
/// Blocks until the shell is done, so never call it from the thread that owns `owner`.
pub fn operate(
    operation: &Operation,
    paths: &[String],
    rename: bool,
    owner: Owner,
    report: impl FnMut(Item) + 'static,
) -> Result<()> {
    let apartment = Apartment::enter();
    apartment.result()?;
    let op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL)? };
    unsafe { op.SetOwnerWindow(HWND(owner as *mut _))? };
    // Do not suppress clash dialogs or approve destructive decisions on the user's behalf.
    let mut flags = FOFX_RECYCLEONDELETE | FOFX_ADDUNDORECORD | FOF_NOCONFIRMMKDIR;
    if matches!(operation, Operation::Recycle) {
        flags |= FOF_NOCONFIRMATION;
    }
    if rename {
        flags |= FOF_RENAMEONCOLLISION;
    }
    unsafe { op.SetOperationFlags(flags)? };
    let sink: IFileOperationProgressSink = Sink(RefCell::new(Box::new(report))).into();
    let cookie = unsafe { op.Advise(&sink)? };
    let queued = queue(&op, operation, paths, &sink);
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

fn item(path: &str) -> Result<IShellItem> {
    let path = Wide::new(path);
    unsafe { SHCreateItemFromParsingName(path.get(), None) }
}

fn queue(op: &IFileOperation, operation: &Operation, paths: &[String], sink: &IFileOperationProgressSink) -> Result<()> {
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
                Operation::Recycle => op.DeleteItem(&source, None)?,
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
        if result.is_ok()
            && let Some(made) = made.as_ref()
        {
            self.report(Item::Placed { path: path(Some(made)) });
        }
        Ok(())
    }
}

fn path(item: Option<&IShellItem>) -> String {
    let Some(item) = item else { return String::new() };
    let Ok(name) = (unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }) else { return String::new() };
    let path = unsafe { name.to_string() }.unwrap_or_default();
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
    fn PostRenameItem(&self, _: u32, item: Ref<IShellItem>, _: &PCWSTR, result: HRESULT, _: Ref<IShellItem>) -> Result<()> {
        self.done(item, result)
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
    fn PreDeleteItem(&self, flags: u32, item: Ref<IShellItem>) -> Result<()> {
        // The shell says here whether the item really goes to the bin. Refuse deleting it outright.
        if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            self.report(Item::Done { path: path(item.as_ref()), result: ERROR_NOT_SUPPORTED.to_hresult() });
            return Err(Error::from_hresult(E_ABORT));
        }
        Ok(())
    }
    fn PostDeleteItem(&self, _: u32, item: Ref<IShellItem>, result: HRESULT, _: Ref<IShellItem>) -> Result<()> {
        self.done(item, result)
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
mod tests {
    use std::{rc::Rc, cell::RefCell};

    use super::*;
    use windows::Win32::UI::Shell::TSF_NORMAL;

    #[test]
    fn refuses_to_delete_what_the_bin_cannot_take() {
        let reports = Rc::new(RefCell::new(Vec::new()));
        let seen = reports.clone();
        let sink: IFileOperationProgressSink =
            Sink(RefCell::new(Box::new(move |item| seen.borrow_mut().push(item)))).into();
        unsafe {
            assert_eq!(sink.PreDeleteItem(TSF_NORMAL.0 as u32, None).unwrap_err().code(), E_ABORT);
            assert!(sink.PreDeleteItem(TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32, None).is_ok());
        }
        assert_eq!(reports.borrow().len(), 1);
    }

    #[test]
    fn copies_and_says_where_each_item_went() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        let reports = Rc::new(RefCell::new(Vec::new()));
        let seen = reports.clone();
        operate(
            &Operation::Copy { destination: dir.path().to_string_lossy().into_owned() },
            &[file.to_string_lossy().into_owned()],
            true,
            0,
            move |item| seen.borrow_mut().push(item),
        )
        .unwrap();
        let reports = reports.borrow();
        assert!(matches!(&reports[0], Item::Done { result, .. } if result.is_ok()), "{reports:?}");
        let Item::Placed { path } = &reports[1] else { panic!("{reports:?}") };
        assert_ne!(path, &file.to_string_lossy());
        assert!(std::path::Path::new(path).exists());
    }
}
