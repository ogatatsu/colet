//! What the clipboard gives only by its contents, as Outlook's attachments, pasted as Explorer pastes it:
//! dropped on the folder, whose drop target writes the files out with the shell's own dialogs; and items
//! sent to one of Windows' Send To, dropped on it as Explorer's Send To does. The shell may go on reading
//! the data after the drop returns, so drops happen on a thread kept for the life of the process, with OLE
//! and a message loop.
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};
use std::thread;

use std::mem::ManuallyDrop;

use windows::Win32::Foundation::{E_INVALIDARG, E_OUTOFMEMORY, GlobalFree, HWND, LPARAM, POINTL, WPARAM};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, IDropTarget, ReleaseStgMedium};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_SHIFT, MODIFIERKEYS_FLAGS};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    ILClone, ILFindChild, ILFindLastID, ILIsParent, ILRemoveLastID, IShellFolder, SHBindToParent, SHCreateDataObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage, WM_APP,
};
use windows::core::{Error, Result, w};

use super::{Ole, Owner, Pidls, parse};

/// How a drop went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dropped {
    /// The folder would not take it.
    Refused,
    Copied,
    /// Moved: its source lets it go.
    Moved,
}

/// What is dropped: what the clipboard holds, so long as it still holds what was read as `sequence`; or
/// the items at `paths`, with Shift held or not.
enum Source {
    Clipboard { sequence: u32, cut: bool },
    Items { paths: Vec<PathBuf>, shift: bool },
}

struct Request {
    source: Source,
    destination: PathBuf,
    owner: Owner,
    done: mpsc::Sender<Result<Dropped>>,
}

/// The thread drops happen on, and how requests reach it.
#[derive(Clone)]
struct Dropper {
    thread: u32,
    requests: mpsc::Sender<Request>,
}

/// Wakes the dropping thread to take its requests.
const WAKE: u32 = WM_APP;

/// Drops what the clipboard holds on the folder `destination`, moved if `cut` and its source allows it, so
/// long as it still holds what was read as `sequence`.
/// Blocks until the drop returns, which may be before the shell has written everything out.
pub fn drop_clipboard(destination: &Path, cut: bool, sequence: u32, owner: Owner) -> Result<Dropped> {
    request(Source::Clipboard { sequence, cut }, destination, owner)
}

/// Drops the items at `paths`, from any folders, on `target`, an item of the Send To folder, as Explorer's
/// Send To does: copied, or with Shift held moved where the target allows it.
/// Blocks until the drop returns, which may be before the shell has written everything out.
pub fn send_to(target: &Path, paths: &[PathBuf], shift: bool, owner: Owner) -> Result<Dropped> {
    request(Source::Items { paths: paths.to_vec(), shift }, target, owner)
}

fn request(source: Source, destination: &Path, owner: Owner) -> Result<Dropped> {
    let dropper = dropper()?;
    let (done, result) = mpsc::channel();
    let request = Request { source, destination: destination.to_owned(), owner, done };
    dropper.requests.send(request).map_err(|_| Error::empty())?;
    unsafe { PostThreadMessageW(dropper.thread, WAKE, WPARAM(0), LPARAM(0))? };
    result.recv().map_err(|_| Error::empty())?
}

fn dropper() -> Result<Dropper> {
    static DROPPER: Mutex<Option<Dropper>> = Mutex::new(None);
    let mut dropper = DROPPER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match &*dropper {
        Some(dropper) => Ok(dropper.clone()),
        None => {
            let started = spawn()?;
            *dropper = Some(started.clone());
            Ok(started)
        }
    }
}

fn spawn() -> Result<Dropper> {
    let (requests, incoming) = mpsc::channel::<Request>();
    let (ready, started) = mpsc::channel();
    thread::Builder::new()
        .name("colet-drop".into())
        .spawn(move || {
            let ole = Ole::enter();
            let mut message = MSG::default();
            // A message queue, so the wake-ups posted from now on are kept.
            unsafe {
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
            }
            let _ = ready.send(ole.as_ref().map(|_| unsafe { GetCurrentThreadId() }).map_err(Clone::clone));
            if ole.is_err() {
                return;
            }
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                if message.hwnd.is_invalid() && message.message == WAKE {
                    for Request { source, destination, owner, done } in incoming.try_iter() {
                        let dropped = match source {
                            // As Explorer's paste: a cut moves where its source allows it, and copies where not.
                            Source::Clipboard { sequence, cut } => super::clipboard::data(sequence).and_then(|data| {
                                let allowed = if cut { DROPEFFECT_COPY | DROPEFFECT_MOVE } else { DROPEFFECT_COPY };
                                drop_on(&destination, allowed, MODIFIERKEYS_FLAGS(0), owner, data)
                            }),
                            Source::Items { paths, shift } => items(&paths).and_then(|data| {
                                let (allowed, keys) = sent(shift);
                                drop_on(&destination, allowed, keys, owner, data)
                            }),
                        };
                        let _ = done.send(dropped);
                    }
                    continue;
                }
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        })
        .map_err(|_| Error::empty())?;
    let thread = started.recv().map_err(|_| Error::empty())??;
    Ok(Dropper { thread, requests })
}

/// What a drop sent with Shift held or not may do, and the keys it tells the target of: a copy, or with
/// Shift a move, as a drag with it held.
fn sent(shift: bool) -> (DROPEFFECT, MODIFIERKEYS_FLAGS) {
    if shift { (DROPEFFECT_MOVE, MODIFIERKEYS_FLAGS(MK_SHIFT.0)) } else { (DROPEFFECT_COPY, MODIFIERKEYS_FLAGS(0)) }
}

/// The items at `paths` as one data object, by the shell's own IDs as Explorer's selection gives them, and
/// by their paths as any app's drag gives them. Their IDs are taken from the deepest folder they all lie
/// in, so they need not share one; not from the desktop where it can be helped, which the desktop's
/// shortcut maker fails on.
pub(super) fn items(paths: &[PathBuf]) -> Result<IDataObject> {
    if paths.is_empty() {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let pidls = Pidls::parse_all(paths)?;
    let clone = unsafe { ILClone(pidls.0[0]) };
    if clone.is_null() {
        return Err(Error::from_hresult(E_OUTOFMEMORY));
    }
    let folder = Pidls(vec![clone]);
    unsafe {
        let _ = ILRemoveLastID(Some(folder.0[0]));
        while !pidls.0.iter().all(|pidl| ILIsParent(folder.0[0], *pidl, false).as_bool()) && ILRemoveLastID(Some(folder.0[0])).as_bool() {}
    }
    let children: Vec<*const ITEMIDLIST> = pidls.0.iter().map(|pidl| unsafe { ILFindChild(folder.0[0], *pidl) } as *const _).collect();
    let data: IDataObject = unsafe { SHCreateDataObject(Some(folder.0[0]), Some(&children), None)? };
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let files = super::clipboard::global(&super::clipboard::drop_files(paths))?;
    let medium = STGMEDIUM { tymed: TYMED_HGLOBAL.0 as u32, u: STGMEDIUM_0 { hGlobal: files }, pUnkForRelease: ManuallyDrop::new(None) };
    // Taken by the data object once it is set.
    if let Err(error) = unsafe { data.SetData(&format, &medium, true) } {
        let _ = unsafe { GlobalFree(Some(files)) };
        return Err(error);
    }
    Ok(data)
}

/// What the target set on `data` as done, if it did.
fn performed(data: &IDataObject) -> Option<DROPEFFECT> {
    let format = FORMATETC {
        cfFormat: unsafe { RegisterClipboardFormatW(w!("Performed DropEffect")) } as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let mut medium = unsafe { data.GetData(&format) }.ok()?;
    let effect = unsafe {
        let held = GlobalLock(medium.u.hGlobal).cast::<u32>();
        let effect = (!held.is_null() && GlobalSize(medium.u.hGlobal) >= size_of::<u32>()).then(|| held.read_unaligned());
        let _ = GlobalUnlock(medium.u.hGlobal);
        ReleaseStgMedium(&mut medium);
        effect
    };
    effect.map(DROPEFFECT)
}

/// Drops `data` on `destination`, a folder or anything else that takes a drop, doing what `allowed` lets
/// it with `keys` held.
fn drop_on(destination: &Path, allowed: DROPEFFECT, keys: MODIFIERKEYS_FLAGS, owner: Owner, data: IDataObject) -> Result<Dropped> {
    let folder = Pidls(vec![parse(destination)?]);
    let parent: IShellFolder = unsafe { SHBindToParent(folder.0[0], None)? };
    let child = unsafe { ILFindLastID(folder.0[0]) } as *const _;
    let target: IDropTarget = unsafe { parent.GetUIObjectOf(HWND(owner as *mut _), &[child], None)? };
    let at = POINTL::default();
    let mut effect = allowed;
    unsafe { target.DragEnter(&data, MODIFIERKEYS_FLAGS(keys.0 | MK_LBUTTON.0), at, &mut effect)? };
    if effect.0 & allowed.0 == 0 {
        unsafe { target.DragLeave()? };
        return Ok(Dropped::Refused);
    }
    // Let go of the button, as a drag ends.
    let mut effect = allowed;
    unsafe { target.Drop(&data, keys, at, &mut effect)? };
    // A move the target made itself it says nothing of here, so the source deletes nothing, but in what
    // it sets on the data.
    if effect.0 == 0 {
        effect = performed(&data).unwrap_or_default();
    }
    Ok(if effect.0 & DROPEFFECT_MOVE.0 != 0 {
        Dropped::Moved
    } else if effect.0 & DROPEFFECT_COPY.0 != 0 {
        Dropped::Copied
    } else {
        Dropped::Refused
    })
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::mem::{self, ManuallyDrop};
    use std::ptr;

    use windows::Win32::Foundation::{DV_E_FORMATETC, DV_E_TYMED, E_NOTIMPL, OLE_E_ADVISENOTSUPPORTED, S_OK};
    use windows::Win32::System::Com::{
        DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, STGMEDIUM,
        STGMEDIUM_0, TYMED_HGLOBAL,
    };
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::ReleaseStgMedium;
    use windows::Win32::UI::Shell::{FD_FILESIZE, FILEDESCRIPTORW, SHCreateStdEnumFmtEtc};
    use windows::core::{BOOL, Error, HRESULT, Ref, Result, implement, w};

    /// One file given only by its contents, as Outlook gives an attachment.
    #[implement(IDataObject)]
    struct Contents {
        name: String,
        bytes: Vec<u8>,
    }

    /// A data object holding a file named `name` with `bytes`, by its descriptor and contents only.
    pub fn contents(name: &str, bytes: &[u8]) -> IDataObject {
        Contents { name: name.into(), bytes: bytes.to_vec() }.into()
    }

    /// Does `work` on another thread while [`contents`] is on the clipboard, served from this one as an
    /// app serves what it put there; replaces what the clipboard held.
    pub fn while_on_clipboard<T: Send + 'static>(name: &str, bytes: &[u8], work: impl FnOnce() -> T + Send + 'static) -> T {
        use windows::Win32::System::Ole::{OleFlushClipboard, OleSetClipboard};
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};

        let _ole = super::Ole::enter().unwrap();
        unsafe { OleSetClipboard(&contents(name, bytes)).unwrap() };
        let worker = std::thread::spawn(work);
        let mut message = MSG::default();
        while !worker.is_finished() {
            unsafe {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Lets the data object go; only the descriptor stays behind.
        let _ = unsafe { OleFlushClipboard() };
        worker.join().unwrap()
    }

    fn descriptor() -> u16 {
        unsafe { RegisterClipboardFormatW(w!("FileGroupDescriptorW")) as u16 }
    }

    fn file_contents() -> u16 {
        unsafe { RegisterClipboardFormatW(w!("FileContents")) as u16 }
    }

    fn format(format: u16) -> FORMATETC {
        FORMATETC { cfFormat: format, ptd: ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 }
    }

    fn held(format: &FORMATETC) -> Result<()> {
        if format.cfFormat != descriptor() && format.cfFormat != file_contents() {
            return Err(Error::from_hresult(DV_E_FORMATETC));
        }
        if format.tymed & TYMED_HGLOBAL.0 as u32 == 0 {
            return Err(Error::from_hresult(DV_E_TYMED));
        }
        Ok(())
    }

    fn global(bytes: &[u8]) -> Result<STGMEDIUM> {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
            ptr::copy_nonoverlapping(bytes.as_ptr(), GlobalLock(handle).cast::<u8>(), bytes.len());
            let _ = GlobalUnlock(handle);
            Ok(STGMEDIUM { tymed: TYMED_HGLOBAL.0 as u32, u: STGMEDIUM_0 { hGlobal: handle }, pUnkForRelease: ManuallyDrop::new(None) })
        }
    }

    #[allow(non_snake_case)]
    impl IDataObject_Impl for Contents_Impl {
        fn GetData(&self, format: *const FORMATETC) -> Result<STGMEDIUM> {
            let format = unsafe { &*format };
            held(format)?;
            if format.cfFormat == file_contents() {
                return global(&self.bytes);
            }
            let mut name = [0u16; 260];
            for (to, from) in name.iter_mut().zip(self.name.encode_utf16()) {
                *to = from;
            }
            let file = FILEDESCRIPTORW {
                dwFlags: FD_FILESIZE.0 as u32,
                nFileSizeLow: self.bytes.len() as u32,
                cFileName: name,
                ..Default::default()
            };
            let mut group = 1u32.to_le_bytes().to_vec();
            group.extend(unsafe {
                std::slice::from_raw_parts((&file as *const FILEDESCRIPTORW).cast::<u8>(), mem::size_of::<FILEDESCRIPTORW>())
            });
            global(&group)
        }
        fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> Result<()> {
            Err(Error::from_hresult(E_NOTIMPL))
        }
        fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
            held(unsafe { &*format }).map_or_else(|error| error.code(), |()| S_OK)
        }
        fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
            E_NOTIMPL
        }
        fn SetData(&self, _: *const FORMATETC, medium: *const STGMEDIUM, release: BOOL) -> Result<()> {
            // What the shell says back, as the effect it performed, is taken and let go of.
            if release.as_bool() {
                unsafe { ReleaseStgMedium(medium as *mut _) };
            }
            Ok(())
        }
        fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
            if direction != DATADIR_GET.0 as u32 {
                return Err(Error::from_hresult(E_NOTIMPL));
            }
            unsafe { SHCreateStdEnumFmtEtc(&[format(descriptor()), format(file_contents())]) }
        }
        fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<IAdviseSink>) -> Result<u32> {
            Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
        }
        fn DUnadvise(&self, _: u32) -> Result<()> {
            Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
        }
        fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
            Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_out_what_is_given_only_by_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        let _ole = Ole::enter().unwrap();
        let data = fixtures::contents("添付.txt", b"attached");
        let dropped = drop_on(dir.path(), DROPEFFECT_COPY, MODIFIERKEYS_FLAGS(0), 0, data).unwrap();
        assert_eq!(dropped, Dropped::Copied);
        assert_eq!(std::fs::read(dir.path().join("添付.txt")).unwrap(), b"attached");
    }

    #[test]
    fn sends_items_from_different_folders_copied_or_with_shift_moved() {
        let (a, b, to) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (x, y) = (a.path().join("x.txt"), b.path().join("y.txt"));
        std::fs::write(&x, "x").unwrap();
        std::fs::write(&y, "y").unwrap();
        let dropped = send_to(to.path(), &[x.clone(), y.clone()], false, 0).unwrap();
        assert_eq!(dropped, Dropped::Copied);
        assert_eq!(std::fs::read(to.path().join("x.txt")).unwrap(), b"x");
        assert_eq!(std::fs::read(to.path().join("y.txt")).unwrap(), b"y");
        assert!(x.exists() && y.exists());
        let moved = tempfile::tempdir().unwrap();
        assert_eq!(send_to(moved.path(), std::slice::from_ref(&x), true, 0).unwrap(), Dropped::Moved);
        assert!(moved.path().join("x.txt").exists() && !x.exists());
    }
}
