//! What Windows' Send To offers: the items of the Send To folder, as Explorer's menu lists them. Each is
//! sent to by a drop on it, as [`super::send_to`] does.
use std::os::windows::fs::MetadataExt;
use std::path::PathBuf;

use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_FLAGS_AND_ATTRIBUTES};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_SendTo, KF_FLAG_DEFAULT, SHFILEINFOW, SHGFI_DISPLAYNAME, SHGetFileInfoW, SHGetKnownFolderPath};
use windows::core::Result;

use super::{Apartment, Wide};
use crate::paths::from_wide;

/// An item of the Send To folder, by the name Explorer shows it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendTarget {
    pub name: String,
    pub path: PathBuf,
}

/// The items of the Send To folder, by name, less those hidden as `desktop.ini` is.
pub fn send_targets() -> Result<Vec<SendTarget>> {
    let _apartment = Apartment::enter();
    let folder = unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_SendTo, KF_FLAG_DEFAULT, None)?;
        let folder = from_wide(path.as_wide());
        CoTaskMemFree(Some(path.0 as *const _));
        folder
    };
    let mut targets: Vec<SendTarget> = std::fs::read_dir(&folder)?
        .flatten()
        .filter(|item| item.metadata().is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN.0 == 0))
        .map(|item| {
            let path = item.path();
            let name = display_name(&path).unwrap_or_else(|| item.file_name().to_string_lossy().into_owned());
            SendTarget { name, path }
        })
        .collect();
    targets.sort_by_cached_key(|target| target.name.to_lowercase());
    Ok(targets)
}

/// The name Explorer shows `path` by: a shortcut's without `.lnk`, a handler's as it names itself.
fn display_name(path: &std::path::Path) -> Option<String> {
    let wide = Wide::new(path);
    let mut info = SHFILEINFOW::default();
    let found = unsafe {
        SHGetFileInfoW(wide.get(), FILE_FLAGS_AND_ATTRIBUTES(0), Some(&mut info), size_of::<SHFILEINFOW>() as u32, SHGFI_DISPLAYNAME)
    };
    let name = from_wide(&info.szDisplayName);
    (found != 0 && !name.as_os_str().is_empty()).then(|| name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_send_to_folder_by_name_without_its_hidden_items() {
        let targets = send_targets().unwrap();
        assert!(targets.iter().all(|t| !t.name.eq_ignore_ascii_case("desktop.ini")), "{targets:?}");
        assert!(targets.iter().all(|t| t.path.exists()), "{targets:?}");
        let names: Vec<String> = targets.iter().map(|t| t.name.to_lowercase()).collect();
        assert!(names.is_sorted(), "{names:?}");
    }
}
