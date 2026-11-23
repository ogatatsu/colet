//! The language Windows shows itself in.
use windows::Win32::Globalization::GetUserDefaultUILanguage;

/// Japanese's primary language, the low ten bits of a language id (`LANG_JAPANESE`).
const JAPANESE: u16 = 0x11;

/// Whether Windows shows the user its menus and dialogs in Japanese, whatever its region.
pub fn windows_is_japanese() -> bool {
    is_japanese(unsafe { GetUserDefaultUILanguage() })
}

fn is_japanese(language: u16) -> bool {
    language & 0x3ff == JAPANESE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_primary_language_whatever_the_region() {
        assert!(is_japanese(0x0411), "ja-JP");
        assert!(!is_japanese(0x0409), "en-US");
        assert!(!is_japanese(0x0809), "en-GB");
        assert!(!is_japanese(0x0804), "zh-CN");
    }
}
