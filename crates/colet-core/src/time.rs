//! Times as the user's clock reads them.
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

/// Milliseconds since the Unix epoch, in local time; `None` for a time Windows cannot place.
pub fn local(unix_ms: u64) -> Option<SYSTEMTIME> {
    let ticks = unix_ms.checked_mul(10_000)?.checked_add(116_444_736_000_000_000)?;
    let file = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        FileTimeToSystemTime(&file, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
    }
    Some(local)
}

/// `2026/10/16`, as `toLocaleDateString("ja-JP")` writes it.
pub fn date(unix_ms: u64) -> Option<String> {
    let t = local(unix_ms)?;
    Some(format!("{}/{}/{}", t.wYear, t.wMonth, t.wDay))
}

/// `2026/10/16 9:05:03`, as `toLocaleString("ja-JP")` writes it.
pub fn date_time(unix_ms: u64) -> Option<String> {
    let t = local(unix_ms)?;
    Some(format!("{}/{}/{} {}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond))
}

/// `1.5 MB`: bytes below a kilobyte, kilobytes whole, the rest to one place.
pub fn size(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else { return "—".into() };
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let shown = |value: f64, i: usize| if i == 1 { value.round() } else { (value * 10.).round() / 10. };
    let (mut value, mut i) = (bytes as f64, 0);
    // Rounded, 1023.7 KB would read 1024 KB: such a value goes on to the next unit.
    while i < 4 && shown(value, i) >= 1024. {
        value /= 1024.;
        i += 1;
    }
    let unit = ["B", "KB", "MB", "GB", "TB"][i];
    if i == 1 { format!("{value:.0} {unit}") } else { format!("{value:.1} {unit}") }
}

/// `19,770`, as `toLocaleString("ja-JP")` writes a count.
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_counts_in_threes() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1000), "1,000");
        assert_eq!(count(19_770), "19,770");
        assert_eq!(count(1_234_567), "1,234,567");
    }

    #[test]
    fn writes_sizes_as_the_old_columns_did() {
        assert_eq!(size(None), "—");
        assert_eq!(size(Some(512)), "512 B");
        assert_eq!(size(Some(1536)), "2 KB");
        assert_eq!(size(Some(1024 * 1024 * 3 / 2)), "1.5 MB");
        assert_eq!(size(Some(1024u64.pow(4) * 2)), "2.0 TB");
        // Never 1024 of a unit: it rounds up to the next.
        assert_eq!(size(Some(1024 * 1024 - 1)), "1.0 MB");
        assert_eq!(size(Some(1024u64.pow(3) - 1)), "1.0 GB");
        assert_eq!(size(Some(1024 * 1023 + 400)), "1023 KB");
        assert_eq!(size(Some(1024u64.pow(5) * 3)), "3072.0 TB");
    }

    #[test]
    fn writes_dates_in_local_time() {
        let text = date_time(1_700_000_000_000).unwrap();
        assert!(text.starts_with("2023/11/1"), "{text}");
        assert!(date(1_700_000_000_000).unwrap().starts_with("2023/11/1"));
    }
}
