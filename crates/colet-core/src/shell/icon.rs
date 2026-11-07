use std::{ffi::c_void, mem, path::PathBuf};

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

use super::{Apartment, Wide};

/// What to ask the shell for an icon of.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconSource {
    /// Any file with this extension (`".txt"`, or `""` for none), without touching the disk.
    Extension(String),
    /// The generic folder.
    Folder,
    /// The real item: drives, and files that carry their own icon such as `.exe` and `.lnk`.
    Item(PathBuf),
}

/// A small shell icon as top-down BGRA rows, premultiplied as the shell hands them out.
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// The shell's small icon for `source`, or `None` when the shell has none.
pub fn icon(source: &IconSource) -> Option<Icon> {
    let _apartment = Apartment::enter();
    let (path, synthetic, directory) = match source {
        IconSource::Extension(ext) => (PathBuf::from(format!("file{ext}")), true, false),
        IconSource::Folder => ("folder".into(), true, true),
        IconSource::Item(path) => (path.clone(), false, false),
    };
    let path = Wide::new(&path);
    let mut info = SHFILEINFOW::default();
    let mut flags = SHGFI_ICON | SHGFI_SMALLICON;
    if synthetic {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }
    let attributes = if directory { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
    let found = unsafe { SHGetFileInfoW(path.get(), attributes, Some(&mut info), mem::size_of::<SHFILEINFOW>() as u32, flags) };
    if found == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let pixels = unsafe { pixels(&info) };
    unsafe { DestroyIcon(info.hIcon).ok() };
    pixels
}

unsafe fn pixels(info: &SHFILEINFOW) -> Option<Icon> {
    let mut icon = ICONINFO::default();
    unsafe { GetIconInfo(info.hIcon, &mut icon).ok()? };
    let result = unsafe { read(&icon) };
    unsafe {
        if !icon.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(icon.hbmColor.0));
        }
        if !icon.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(icon.hbmMask.0));
        }
    }
    result
}

unsafe fn read(icon: &ICONINFO) -> Option<Icon> {
    if icon.hbmColor.is_invalid() {
        return None;
    }
    let mut bitmap = BITMAP::default();
    let size = mem::size_of::<BITMAP>() as i32;
    if unsafe { GetObjectW(HGDIOBJ(icon.hbmColor.0), size, Some(&mut bitmap as *mut _ as *mut c_void)) } == 0 {
        return None;
    }
    let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
    let count = (width * height) as usize;
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; count * 4];
    let dc = unsafe { GetDC(None) };
    let read = unsafe { GetDIBits(dc, icon.hbmColor, 0, height as u32, Some(bgra.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS) };
    if read != 0 && bgra.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0) {
        // Legacy icons carry their transparency in the AND mask instead.
        let mut mask = vec![0u8; count * 4];
        let masked =
            unsafe { GetDIBits(dc, icon.hbmMask, 0, height as u32, Some(mask.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS) } != 0;
        for (pixel, mask) in bgra.as_chunks_mut::<4>().0.iter_mut().zip(mask.as_chunks::<4>().0) {
            pixel[3] = if masked && mask[0] != 0 { 0 } else { 255 };
        }
    }
    unsafe { ReleaseDC(None, dc) };
    (read != 0).then_some(Icon { width: width as u32, height: height as u32, bgra })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_shell_icons() {
        for source in [IconSource::Extension(".txt".into()), IconSource::Folder, IconSource::Item("C:\\".into())] {
            let icon = icon(&source).unwrap_or_else(|| panic!("{source:?}"));
            assert!(icon.width > 0 && icon.height > 0, "{source:?}");
            assert_eq!(icon.bgra.len(), (icon.width * icon.height * 4) as usize);
            assert!(icon.bgra.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0), "{source:?} is blank");
        }
    }
}
