//! Text as the Windows APIs take it.
use windows::core::PCWSTR;

/// A NUL-terminated UTF-16 copy of `text`, kept alive while a `PCWSTR` points into it.
pub(crate) struct Wide(Vec<u16>);

impl Wide {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.encode_utf16().chain([0]).collect())
    }
    pub(crate) fn get(&self) -> PCWSTR {
        PCWSTR(self.0.as_ptr())
    }
}
