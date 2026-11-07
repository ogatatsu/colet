use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{E_INVALIDARG, HWND};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    ILClone, ILCreateFromPathW, ILFindLastID, ILFree, ILRemoveLastID, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
    SHOpenFolderAndSelectItems, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{Error, Result, w};

use super::{Apartment, Owner, Pidls, Wide};

/// Opens `path` with the app Windows associates with it.
pub fn open(path: &Path, owner: Owner) -> Result<()> {
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

/// Shows `paths`, all in one folder, selected together in an Explorer window of it.
pub fn reveal(paths: &[PathBuf]) -> Result<()> {
    let _apartment = Apartment::enter();
    if let [path] = paths {
        let file = Wide::new(path);
        let pidl = unsafe { ILCreateFromPathW(file.get()) };
        if pidl.is_null() {
            return Err(Error::from_thread());
        }
        let result = unsafe { SHOpenFolderAndSelectItems(pidl, None, 0) };
        unsafe { ILFree(Some(pidl)) };
        return result;
    }
    let items = Pidls::parse_all(paths)?;
    let Some(&first) = items.0.first() else { return Ok(()) };
    let folder = Pidls(vec![unsafe { ILClone(first) }]);
    if folder.0[0].is_null() || !unsafe { ILRemoveLastID(Some(folder.0[0])) }.as_bool() {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    // Each named by its last ID, under the folder it shares.
    let children: Vec<*const ITEMIDLIST> = items.0.iter().map(|item| unsafe { ILFindLastID(*item) } as *const _).collect();
    unsafe { SHOpenFolderAndSelectItems(folder.0[0], Some(&children), 0) }
}
