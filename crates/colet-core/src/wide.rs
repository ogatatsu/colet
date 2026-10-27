//! Text as the Windows APIs take it.
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::core::PCWSTR;

/// A NUL-terminated UTF-16 copy of `text`, kept alive while a `PCWSTR` points into it. A path goes as
/// the OS gave it, even where it is not valid Unicode.
pub(crate) struct Wide(Vec<u16>);

impl Wide {
    pub(crate) fn new(text: impl AsRef<OsStr>) -> Self {
        Self(text.as_ref().encode_wide().chain([0]).collect())
    }
    pub(crate) fn get(&self) -> PCWSTR {
        PCWSTR(self.0.as_ptr())
    }
}
