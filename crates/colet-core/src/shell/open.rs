use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SHOpenFolderAndSelectItems,
    ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{Error, Result, w};

use super::{Apartment, Owner, Wide};

/// Opens `path` with the app Windows associates with it.
pub fn open(path: &str, owner: Owner) -> Result<()> {
    let _apartment = Apartment::enter();
    let file = Wide::new(path);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        hwnd: HWND(owner as *mut _),
        lpVerb: w!("open"),
        lpFile: file.get(),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }
}

/// Shows `path` selected in an Explorer window of its folder.
pub fn reveal(path: &str) -> Result<()> {
    let _apartment = Apartment::enter();
    let file = Wide::new(path);
    let pidl = unsafe { ILCreateFromPathW(file.get()) };
    if pidl.is_null() {
        return Err(Error::from_thread());
    }
    let result = unsafe { SHOpenFolderAndSelectItems(pidl, None, 0) };
    unsafe { ILFree(Some(pidl)) };
    result
}
