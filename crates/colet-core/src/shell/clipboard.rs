//! Files on the clipboard as Explorer puts them there (`CF_HDROP` with a preferred drop effect), so a copy
//! or cut here pastes in Explorer and the other way round; and paths as text. Shell items with no path, as
//! Explorer puts those from a ZIP folder or a phone there, and what is given only by its contents, as
//! Outlook puts its attachments there, are only noticed: the shell reads them itself.
use std::{
    ffi::{OsStr, OsString, c_void},
    mem,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr, thread,
    time::Duration,
};

use windows::Win32::Foundation::{E_CHANGED_STATE, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_MOVE, OleGetClipboard};
use windows::Win32::UI::Shell::{DROPFILES, DragQueryFileW, FILEDESCRIPTORW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE, MSG, RegisterClassW,
    SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE, WNDCLASSW,
};
use windows::core::{Error, HRESULT, PCWSTR, Result, w};

use super::Owner;
use crate::paths::from_wide;

/// The files the clipboard holds, put there by Colet or any other app, and whether they were cut.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    /// With no `paths`: shell items with none, as from a ZIP folder or a phone, which only the shell reads.
    pub items: bool,
    /// With no `paths` or `items`: the names of what is given only by its contents, as Outlook's attachments,
    /// which the shell writes out.
    pub contents: Vec<OsString>,
    pub cut: bool,
    /// The clipboard's sequence number as read, so a cut is let go of only while it is still there.
    pub sequence: u32,
}

fn preferred_effect() -> u32 {
    unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) }
}

/// A movable global holding `bytes`, as the clipboard takes data.
pub(super) fn global(bytes: &[u8]) -> Result<HGLOBAL> {
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let target = GlobalLock(handle);
        if target.is_null() {
            let _ = GlobalFree(Some(handle));
            return Err(Error::from_thread());
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(handle);
        Ok(handle)
    }
}

/// The clipboard, opened for as long as it lives. Another app may hold it a moment: a few tries.
struct Opened;

impl Opened {
    fn new(owner: Owner) -> Result<Self> {
        let mut last = Error::empty();
        for _ in 0..5 {
            match unsafe { OpenClipboard(Some(HWND(owner as *mut c_void))) } {
                Ok(()) => return Ok(Self),
                Err(error) => last = error,
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err(last)
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// `paths` as `CF_HDROP` holds them.
pub(super) fn drop_files(paths: &[PathBuf]) -> Vec<u8> {
    let mut list: Vec<u16> = Vec::new();
    for path in paths {
        list.extend(path.as_os_str().encode_wide());
        list.push(0);
    }
    list.push(0);
    let header = DROPFILES { pFiles: mem::size_of::<DROPFILES>() as u32, fWide: true.into(), ..Default::default() };
    let mut bytes = Vec::with_capacity(mem::size_of::<DROPFILES>() + list.len() * 2);
    bytes.extend_from_slice(unsafe { std::slice::from_raw_parts((&header as *const DROPFILES).cast::<u8>(), mem::size_of::<DROPFILES>()) });
    bytes.extend(list.iter().flat_map(|c| c.to_le_bytes()));
    bytes
}

/// Puts `paths` on the clipboard, cut or copied.
pub fn write_files(paths: &[PathBuf], cut: bool, owner: Owner) -> Result<()> {
    let bytes = drop_files(paths);
    let effect = if cut { DROPEFFECT_MOVE.0 } else { DROPEFFECT_COPY.0 };
    let _clipboard = Opened::new(owner)?;
    unsafe {
        EmptyClipboard()?;
        let files = global(&bytes)?;
        if let Err(error) = SetClipboardData(CF_HDROP.0.into(), Some(HANDLE(files.0))) {
            let _ = GlobalFree(Some(files));
            return Err(error);
        }
        let preferred = global(&effect.to_le_bytes())?;
        if let Err(error) = SetClipboardData(preferred_effect(), Some(HANDLE(preferred.0))) {
            let _ = GlobalFree(Some(preferred));
            return Err(error);
        }
    }
    Ok(())
}

/// Puts `text` on the clipboard.
pub fn write_text(text: &OsStr, owner: Owner) -> Result<()> {
    let bytes: Vec<u8> = text.encode_wide().chain([0]).flat_map(|c| c.to_le_bytes()).collect();
    let _clipboard = Opened::new(owner)?;
    unsafe {
        EmptyClipboard()?;
        let handle = global(&bytes)?;
        if let Err(error) = SetClipboardData(CF_UNICODETEXT.0.into(), Some(HANDLE(handle.0))) {
            let _ = GlobalFree(Some(handle));
            return Err(error);
        }
    }
    Ok(())
}

/// The files on the clipboard: none when it holds something else.
pub fn read_files() -> Result<ClipboardFiles> {
    let _clipboard = Opened::new(0)?;
    unsafe {
        let mut cut = false;
        if let Ok(effect) = GetClipboardData(preferred_effect()) {
            let value = GlobalLock(HGLOBAL(effect.0)).cast::<u32>();
            if !value.is_null() {
                cut = *value & DROPEFFECT_MOVE.0 != 0;
                let _ = GlobalUnlock(HGLOBAL(effect.0));
            }
        }
        let Ok(drop) = GetClipboardData(CF_HDROP.0.into()) else {
            let items = IsClipboardFormatAvailable(RegisterClipboardFormatW(w!("Shell IDList Array"))).is_ok();
            let contents = if items { Vec::new() } else { described() };
            // Read after the formats: one its owner renders only now, as asked for, counts as a change.
            let sequence = GetClipboardSequenceNumber();
            return Ok(ClipboardFiles { items, contents, cut, sequence, ..ClipboardFiles::default() });
        };
        let drop = HDROP(drop.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        let paths = (0..count)
            .map(|i| {
                let len = DragQueryFileW(drop, i, None) as usize;
                let mut buffer = vec![0u16; len + 1];
                DragQueryFileW(drop, i, Some(&mut buffer));
                from_wide(&buffer[..len])
            })
            .collect();
        let sequence = GetClipboardSequenceNumber();
        Ok(ClipboardFiles { paths, cut, sequence, ..ClipboardFiles::default() })
    }
}

/// The names at the top of the open clipboard's file group descriptor, once each: those of what it gives
/// only by its contents. An item inside a folder it gives is named with the folder's path.
unsafe fn described() -> Vec<OsString> {
    let Ok(handle) = (unsafe { GetClipboardData(RegisterClipboardFormatW(w!("FileGroupDescriptorW"))) }) else {
        return Vec::new();
    };
    let handle = HGLOBAL(handle.0);
    let group = unsafe { GlobalLock(handle) }.cast::<u8>();
    if group.is_null() {
        return Vec::new();
    }
    let size = unsafe { GlobalSize(handle) };
    let mut names: Vec<OsString> = Vec::new();
    if size >= mem::size_of::<u32>() {
        let count = unsafe { group.cast::<u32>().read_unaligned() } as usize;
        let room = (size - mem::size_of::<u32>()) / mem::size_of::<FILEDESCRIPTORW>();
        for i in 0..count.min(room) {
            let at = unsafe { group.add(mem::size_of::<u32>() + i * mem::size_of::<FILEDESCRIPTORW>()) };
            let descriptor = unsafe { at.cast::<FILEDESCRIPTORW>().read_unaligned() };
            let file_name = descriptor.cFileName;
            let len = file_name.iter().position(|&c| c == 0).unwrap_or(file_name.len());
            // The top of what it names, as the OS wrote it, past any leading backslash.
            let backslash = u16::from(b'\\');
            let name = &file_name[..len];
            let name = &name[name.iter().position(|&c| c != backslash).unwrap_or(name.len())..];
            let top = OsString::from_wide(&name[..name.iter().position(|&c| c == backslash).unwrap_or(name.len())]);
            if !top.is_empty() && !names.contains(&top) {
                names.push(top);
            }
        }
    }
    let _ = unsafe { GlobalUnlock(handle) };
    names
}

/// The clipboard's data object, as the shell reads it, so long as it still holds what was read as
/// `sequence`: another copy in the meantime fails with [`CHANGED`] rather than pasting that instead.
pub(super) fn data(sequence: u32) -> Result<IDataObject> {
    let data = unsafe { OleGetClipboard()? };
    if unsafe { GetClipboardSequenceNumber() } != sequence {
        return Err(Error::from_hresult(CHANGED));
    }
    Ok(data)
}

/// The clipboard changed between being read and being pasted.
pub const CHANGED: HRESULT = E_CHANGED_STATE;

/// A cut pasted: empties the clipboard, as Explorer does, if what was read is still what it holds.
pub fn complete(sequence: u32) {
    if let Ok(_clipboard) = Opened::new(0)
        && unsafe { GetClipboardSequenceNumber() } == sequence
    {
        let _ = unsafe { EmptyClipboard() };
    }
}

unsafe extern "system" fn listener(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_CLIPBOARDUPDATE {
        let changed = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const Box<dyn Fn() + Send>;
        if !changed.is_null() {
            unsafe { (*changed)() };
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(window, message, w, l) }
}

/// Calls `changed` whenever any app changes the clipboard, from a thread that lives as long as Colet.
pub fn watch(changed: impl Fn() + Send + 'static) -> Result<()> {
    let (ready, started) = std::sync::mpsc::channel();
    thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || {
            let window = (|| -> Result<HWND> {
                let instance = unsafe { GetModuleHandleW(None)? };
                let class = WNDCLASSW {
                    lpfnWndProc: Some(listener),
                    hInstance: instance.into(),
                    lpszClassName: w!("ColetClipboardWatch"),
                    ..Default::default()
                };
                if unsafe { RegisterClassW(&class) } == 0 {
                    return Err(Error::from_thread());
                }
                let window = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        class.lpszClassName,
                        PCWSTR::null(),
                        WINDOW_STYLE(0),
                        0,
                        0,
                        0,
                        0,
                        Some(HWND_MESSAGE),
                        None,
                        Some(class.hInstance),
                        None,
                    )?
                };
                let changed: Box<Box<dyn Fn() + Send>> = Box::new(Box::new(changed));
                unsafe {
                    SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(changed) as isize);
                    AddClipboardFormatListener(window)?;
                }
                Ok(window)
            })();
            let ok = window.is_ok();
            let _ = ready.send(window.map(|_| ()));
            if !ok {
                return;
            }
            let mut message = MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        })
        .map_err(|_| Error::from_thread())?;
    started.recv().map_err(|_| Error::empty())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };
    use std::time::Instant;

    #[test]
    #[ignore = "replaces the real clipboard"]
    fn sees_cuts_copies_and_their_replacement() {
        let changes = Arc::new(AtomicU32::new(0));
        let seen = changes.clone();
        watch(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        let wait = |before: u32| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while changes.load(Ordering::SeqCst) == before {
                assert!(Instant::now() < deadline, "no clipboard notification");
                thread::sleep(Duration::from_millis(20));
            }
        };
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("切り取り.txt");
        std::fs::write(&file, b"cut").unwrap();
        let path = file.clone();
        let before = changes.load(Ordering::SeqCst);
        write_files(std::slice::from_ref(&path), true, 0).unwrap();
        wait(before);
        let files = read_files().unwrap();
        assert_eq!((files.paths.clone(), files.cut), (vec![path.clone()], true));
        let before = changes.load(Ordering::SeqCst);
        write_files(std::slice::from_ref(&path), false, 0).unwrap();
        wait(before);
        assert!(!read_files().unwrap().cut);
        // A name that is not text comes back as it went, not as its replacement.
        let odd = crate::paths::tests::unpaired(dir.path(), "a");
        let before = changes.load(Ordering::SeqCst);
        write_files(std::slice::from_ref(&odd), false, 0).unwrap();
        wait(before);
        assert_eq!(read_files().unwrap().paths, [odd]);
        let before = changes.load(Ordering::SeqCst);
        write_text("text".as_ref(), 0).unwrap();
        wait(before);
        assert!(read_files().unwrap().paths.is_empty());
        // Emptied only while it still holds what was read.
        write_files(std::slice::from_ref(&path), true, 0).unwrap();
        let read = read_files().unwrap();
        write_text("newer".as_ref(), 0).unwrap();
        complete(read.sequence);
        assert!(Opened::new(0).is_ok());
    }
}
