//! The Windows shell: icons, the Explorer context menu and file operations with the shell's own dialogs.
pub mod clipboard;
mod drop;
mod icon;
mod menu;
mod open;
mod operation;

pub use drop::{Dropped, drop_clipboard};
pub use icon::{Icon, IconSource, icon};
pub use menu::{BUSY_ERROR as MENU_BUSY, MenuThread, ShowMenu};
pub use open::{open, reveal};
pub use operation::{Item, Operation, operate, operate_clipboard};

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{ILFree, SHParseDisplayName};
use windows::core::HRESULT;

use crate::wide::Wide;

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

/// Item ID lists, freed when dropped.
struct Pidls(Vec<*mut ITEMIDLIST>);

impl Drop for Pidls {
    fn drop(&mut self) {
        for pidl in &self.0 {
            unsafe { ILFree(Some(*pidl)) };
        }
    }
}

fn parse(path: &str) -> windows::core::Result<*mut ITEMIDLIST> {
    let path = Wide::new(path);
    let mut pidl = std::ptr::null_mut();
    unsafe { SHParseDisplayName(path.get(), None, &mut pidl, 0, None)? };
    Ok(pidl)
}

#[cfg(test)]
pub(crate) mod fixtures {
    pub use super::drop::fixtures::*;
    pub use super::operation::fixtures::*;
}
