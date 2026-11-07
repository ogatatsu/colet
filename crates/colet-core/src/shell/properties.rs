use std::path::PathBuf;

use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, HWND};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    ILClone, ILFindChild, ILIsParent, ILRemoveLastID, SHCreateDataObject, SHMultiFileProperties, SHOP_FILEPATH, SHObjectProperties,
};
use windows::core::{Error, PCWSTR};

use super::{Apartment, Owner, Pidls, Wide};

/// Opens Windows' own properties of `paths`, as Explorer's Properties does: one item's, a drive's among
/// them, or several together in one dialog, from any folders. The dialog runs on a thread of its own; this
/// returns as it opens. Several are read from here as the dialog opens, and it closes with this thread:
/// call it only on [`super::MenuThread`]'s, which lives as long as the app and pumps messages.
pub(super) fn properties(paths: &[PathBuf], owner: Owner) -> windows::core::Result<()> {
    let _apartment = Apartment::enter();
    match paths {
        [] => Ok(()),
        [path] => {
            let name = Wide::new(path);
            let shown = unsafe { SHObjectProperties(Some(HWND(owner as *mut _)), SHOP_FILEPATH, name.get(), PCWSTR::null()) };
            if shown.as_bool() { Ok(()) } else { Err(Error::from_hresult(E_FAIL)) }
        }
        _ => {
            let items = Pidls::parse_all(paths)?;
            // The dialog wants the items under a folder, as Explorer hands them; under none, it shows the
            // desktop's, which has none. The nearest folder they all lie in stands for it, the items named
            // from there, through as many folders as lie between.
            let parent = Pidls(vec![unsafe { ILClone(items.0[0]) }]);
            let folder = parent.0[0];
            if folder.is_null() {
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            loop {
                if !unsafe { ILRemoveLastID(Some(folder)) }.as_bool() {
                    return Err(Error::from_hresult(E_INVALIDARG));
                }
                if items.0.iter().all(|item| unsafe { ILIsParent(folder, *item, false) }.as_bool()) {
                    break;
                }
            }
            let children: Vec<*const ITEMIDLIST> = items.0.iter().map(|item| unsafe { ILFindChild(folder, *item) } as *const _).collect();
            let data: IDataObject = unsafe { SHCreateDataObject(Some(folder), Some(&children), None)? };
            unsafe { SHMultiFileProperties(&data, 0) }
        }
    }
}
