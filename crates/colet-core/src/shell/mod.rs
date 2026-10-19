//! The Windows shell: icons, the Explorer context menu and file operations with the shell's own dialogs.
pub mod clipboard;
mod icon;
mod menu;
mod open;
mod operation;

pub use icon::{Icon, IconSource, icon};
pub use menu::{BUSY_ERROR as MENU_BUSY, MenuThread, ShowMenu};
pub use open::{open, reveal};
pub use operation::{Item, Operation, operate, operate_clipboard};
#[cfg(test)]
pub(crate) use operation::fixtures;

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::core::{HRESULT, PCWSTR};

/// A window that owns the shell's dialogs, as its raw `HWND`.
pub type Owner = isize;

/// COM on this thread, single-threaded as the shell's UI objects need, for as long as it lives.
struct Apartment(HRESULT);

impl Apartment {
    fn enter() -> Self {
        Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) })
    }
    fn result(&self) -> windows::core::Result<()> {
        self.0.ok()
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0.is_ok() {
            unsafe { CoUninitialize() };
        }
    }
}

/// OLE on this thread, as the clipboard's data objects need, for as long as it lives.
struct Ole;

impl Ole {
    fn enter() -> windows::core::Result<Self> {
        unsafe { OleInitialize(None) }.map(|()| Self)
    }
}

impl Drop for Ole {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}

/// A NUL-terminated UTF-16 copy of `text`, kept alive while a `PCWSTR` points into it.
struct Wide(Vec<u16>);

impl Wide {
    fn new(text: &str) -> Self {
        Self(text.encode_utf16().chain([0]).collect())
    }
    fn get(&self) -> PCWSTR {
        PCWSTR(self.0.as_ptr())
    }
}
