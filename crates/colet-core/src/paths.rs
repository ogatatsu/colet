//! Windows paths as Colet compares and walks them. A path is held as the OS gives it, so a name that is
//! not valid Unicode (an unpaired surrogate) still names that item alone; only what is shown of it is
//! made into text.
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// The path's bytes as the OS string holds them: ASCII, such as the backslash, as itself.
fn bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

/// A run of a path's bytes, cut only at its ends or next to an ASCII backslash.
fn path_of(bytes: &[u8]) -> &Path {
    // SAFETY: every caller cuts a path's encoded bytes at its ends or next to an ASCII backslash, which
    // keeps each part a valid OS string.
    Path::new(unsafe { OsStr::from_encoded_bytes_unchecked(bytes) })
}

fn trim_backslashes(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().rposition(|&b| b != b'\\').map_or(0, |i| i + 1);
    &bytes[..end]
}

fn is_unc(path: &Path) -> bool {
    bytes(path).starts_with(br"\\")
}

/// A UNC path's parts before its first folder: the two empty ones its `\\` makes, and the server's name.
const UNC_ROOT: usize = 3;

/// Whether `path` names a server alone (`\\server`), whose column lists its shares.
pub fn is_server(path: &Path) -> bool {
    let trimmed = trim_backslashes(bytes(path));
    is_unc(path) && trimmed.len() > 2 && !trimmed[2..].contains(&b'\\')
}

/// Every folder from the root down to `path`: a drive root keeps its trailing backslash (`D:\`), a UNC
/// path starts at its server (`\\server`), which holds its shares.
pub fn ancestors(path: &Path) -> Vec<PathBuf> {
    if path.as_os_str().is_empty() {
        return Vec::new();
    }
    let parts: Vec<&[u8]> = trim_backslashes(bytes(path)).split(|&b| b == b'\\').collect();
    let unc = is_unc(path);
    let root = if unc {
        // The first parts as they stand in the path, joined by the backslashes between them.
        let length = parts[..parts.len().min(UNC_ROOT)].iter().map(|part| part.len() + 1).sum::<usize>() - 1;
        path_of(&bytes(path)[..length]).to_owned()
    } else {
        child(path_of(parts[0]), "")
    };
    let mut paths = vec![root];
    for part in &parts[if unc { UNC_ROOT.min(parts.len()) } else { 1 }..] {
        let next = child(paths.last().unwrap(), path_of(part));
        paths.push(next);
    }
    paths
}

/// The folder holding `path`, as [`ancestors`] names it but read from the path itself, with nothing made:
/// `None` for a root (`D:\`, `\\server`) or nothing. A drive's own folders name it `D:` here, where
/// `ancestors` gives `D:\`; one or the other alike stands for it.
pub fn parent_folder(path: &Path) -> Option<&Path> {
    let trimmed = trim_backslashes(bytes(path));
    let root = if is_unc(path) { UNC_ROOT } else { 1 };
    if path.as_os_str().is_empty() || trimmed.split(|&b| b == b'\\').nth(root).is_none() {
        return None;
    }
    trimmed.iter().rposition(|&b| b == b'\\').map(|at| path_of(&trimmed[..at]))
}

/// A path compared without case or the drive root's trailing backslash. Letters are lowered as text;
/// what is not text is kept as it is, so two paths that differ there never share a key.
pub fn path_key(path: &Path) -> OsString {
    let trimmed = path_of(trim_backslashes(bytes(path)));
    let mut key = Vec::new();
    let mut text = String::new();
    for unit in char::decode_utf16(trimmed.as_os_str().encode_wide()) {
        match unit {
            Ok(c) => text.push(c),
            Err(unpaired) => {
                key.extend(text.to_lowercase().encode_utf16());
                text.clear();
                key.push(unpaired.unpaired_surrogate());
            }
        }
    }
    key.extend(text.to_lowercase().encode_utf16());
    OsString::from_wide(&key)
}

/// `name` inside `parent`; a drive root already ends in a backslash.
pub fn child(parent: &Path, name: impl AsRef<OsStr>) -> PathBuf {
    let mut joined = parent.as_os_str().to_owned();
    if !bytes(parent).ends_with(b"\\") {
        joined.push("\\");
    }
    joined.push(name);
    joined.into()
}

/// The last part of a path, or the path itself for a root: `D:` for `D:\`.
pub fn folder_name(path: &Path) -> &OsStr {
    let trimmed = trim_backslashes(bytes(path));
    let start = trimmed.iter().rposition(|&b| b == b'\\').map_or(0, |i| i + 1);
    match &trimmed[start..] {
        [] => path.as_os_str(),
        name => path_of(name).as_os_str(),
    }
}

/// `path` as it stands once `from` is moved to `to`: `from` itself, or what it holds, as written (case
/// counts). `None` for any other path.
pub fn moved(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let rest = bytes(path).strip_prefix(bytes(from))?;
    if !rest.is_empty() && !rest.starts_with(b"\\") {
        return None;
    }
    let mut moved = to.as_os_str().to_owned();
    moved.push(path_of(rest));
    Some(moved.into())
}

/// What a path typed or pasted in the address bar names: an absolute path, on a drive (`D:\work`, or
/// `D:` alone for its root) or a server (`\\server\share\dir`, or `\\server` alone for its shares). The quotes Explorer's "Copy as path" puts
/// round it and the spaces about it go, `/` stands for `\`, and each `%NAME%` that `var` knows is put
/// in; `.` and `..` are walked. `None` for anything else, a relative path among them, or a name Windows
/// never allows.
pub fn address(text: &str, var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let mut text = text.trim();
    if let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        text = inner.trim();
    }
    let text = expand(text, var).replace('/', "\\");
    let (root, rest) = if let Some(unc) = text.strip_prefix(r"\\") {
        let (server, rest) = unc.split_once('\\').unwrap_or((unc, ""));
        if server.is_empty() || matches!(server, "?" | ".") || server.contains(not_in_names) {
            return None;
        }
        (format!(r"\\{server}"), rest)
    } else {
        let mut chars = text.chars();
        let (letter, colon) = (chars.next()?, chars.next()?);
        let rest = chars.as_str();
        if !letter.is_ascii_alphabetic() || colon != ':' || !(rest.is_empty() || rest.starts_with('\\')) {
            return None;
        }
        (format!("{}:\\", letter.to_ascii_uppercase()), rest)
    };
    let mut names: Vec<&str> = Vec::new();
    for name in rest.split('\\') {
        match name {
            "" | "." => {}
            ".." => {
                names.pop();
            }
            name if name.contains(not_in_names) => return None,
            name => names.push(name),
        }
    }
    Some(names.into_iter().fold(PathBuf::from(root), |path, name| child(&path, name)))
}

/// What Windows never allows in a name.
fn not_in_names(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || c < ' '
}

/// `text` with each `%NAME%` that `var` knows put in; the rest stays as typed.
fn expand(text: &str, var: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else { break };
        out.push_str(&rest[..start]);
        match var(&after[..end]).filter(|_| end > 0) {
            Some(value) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            // Its closing % may open the next one.
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// What is shown of a path or a name: a part that is not text shows as U+FFFD. Never to be read back as
/// the path: two items can show alike.
pub fn shown(text: impl AsRef<OsStr>) -> String {
    text.as_ref().to_string_lossy().into_owned()
}

/// Text that tells `path` from every other, for what must be one per item, as a drawn row's id: the path
/// itself where it is text, its escaped form, quoted, where it is not (no path holds a quote).
pub fn id_text(path: &Path) -> String {
    match path.to_str() {
        Some(text) => text.to_owned(),
        None => format!("{:?}", path.as_os_str()),
    }
}

/// A path the OS wrote as UTF-16, as it is, up to its first NUL.
pub fn from_wide(wide: &[u16]) -> PathBuf {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    OsString::from_wide(&wide[..end]).into()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn typed(text: &str) -> Option<String> {
        let var = |name: &str| (name == "USERPROFILE").then(|| r"C:\Users\me".to_owned());
        address(text, var).map(|p| p.to_string_lossy().into_owned())
    }

    #[test]
    fn an_address_names_a_drive_or_a_share_however_it_is_written() {
        assert_eq!(typed(r"D:\work\a.txt").as_deref(), Some(r"D:\work\a.txt"));
        assert_eq!(typed("d:").as_deref(), Some(r"D:\"));
        assert_eq!(typed(r"d:\").as_deref(), Some(r"D:\"));
        assert_eq!(typed("  \"D:/work/sub/\"  ").as_deref(), Some(r"D:\work\sub"));
        assert_eq!(typed(r"D:\\work\.\old\..\new\").as_deref(), Some(r"D:\work\new"));
        assert_eq!(typed(r"D:\..\work").as_deref(), Some(r"D:\work"));
        assert_eq!(typed(r"\\server\share").as_deref(), Some(r"\\server\share"));
        assert_eq!(typed("//server/share/dir/").as_deref(), Some(r"\\server\share\dir"));
        assert_eq!(typed(r"\\192.168.1.2").as_deref(), Some(r"\\192.168.1.2"));
        assert_eq!(typed(r"\\192.168.1.2\").as_deref(), Some(r"\\192.168.1.2"));
        assert_eq!(typed(r"\\server\share\..").as_deref(), Some(r"\\server"));
    }

    #[test]
    fn an_address_takes_in_the_variables_it_knows() {
        assert_eq!(typed(r"%USERPROFILE%\Downloads").as_deref(), Some(r"C:\Users\me\Downloads"));
        assert_eq!(typed(r"%NOWHERE%\Downloads"), None);
        assert_eq!(typed(r"D:\a%b\c").as_deref(), Some(r"D:\a%b\c"));
        assert_eq!(typed(r"D:\%%\c").as_deref(), Some(r"D:\%%\c"));
        assert_eq!(typed(r"D:\50%\%USERPROFILE%x").as_deref(), None);
    }

    #[test]
    fn an_address_never_names_a_relative_or_impossible_path() {
        for text in ["", "work", r"..\work", r"\work", "D:work", r"\\", r"\\\share", r"\\a:b", r"\\?\D:\work", r"\\.\pipe\x", r"D:\a*b", r"D:\a?", r"1:\x"] {
            assert_eq!(typed(text), None, "{text}");
        }
    }

    fn p(text: &str) -> &Path {
        Path::new(text)
    }

    /// `folder\` then `name` with an unpaired surrogate and an `X` after it: a name Windows allows, which
    /// shows as `name\u{FFFD}X`.
    pub(crate) fn unpaired(folder: &Path, name: &str) -> PathBuf {
        let mut wide: Vec<u16> = child(folder, name).as_os_str().encode_wide().collect();
        wide.push(0xD800);
        wide.extend("X".encode_utf16());
        OsString::from_wide(&wide).into()
    }

    #[test]
    fn lists_every_folder_from_the_root() {
        assert_eq!(ancestors(p(r"D:\a\b")), [p(r"D:\"), p(r"D:\a"), p(r"D:\a\b")]);
        assert_eq!(ancestors(p(r"D:\")), [p(r"D:\")]);
        assert_eq!(ancestors(p(r"\\server\share\a")), [p(r"\\server"), p(r"\\server\share"), p(r"\\server\share\a")]);
        assert_eq!(ancestors(p(r"\\server\")), [p(r"\\server")]);
        assert!(ancestors(p("")).is_empty());
    }

    #[test]
    fn finds_the_parent_where_ancestors_would_without_making_them() {
        let paths = [
            r"D:\", "D:", r"D:\a", r"D:\a\", r"D:\a\b", r"\\server\share", r"\\server\share\", r"\\server",
            r"\\server\share\a", r"\\server\share\a\b", "", "search:ext:pdf", r"search:a\b",
        ];
        for path in paths {
            let made = ancestors(p(path));
            // A root, or nothing, has no folder above it.
            assert_eq!(parent_folder(p(path)).is_none(), made.len() < 2, "{path}");
            if let Some(parent) = parent_folder(p(path)) {
                assert_eq!(path_key(parent), path_key(&made[made.len() - 2]), "{path}");
            }
        }
        assert_eq!(parent_folder(p(r"D:\a\b")), Some(p(r"D:\a")));
        assert_eq!(parent_folder(p(r"D:\a")), Some(p("D:")));
        assert_eq!(parent_folder(p(r"\\server\share\a")), Some(p(r"\\server\share")));
        assert_eq!(parent_folder(p(r"\\server\share")), Some(p(r"\\server")));
        assert_eq!(parent_folder(p(r"\\server")), None);
    }

    #[test]
    fn compares_without_case_or_trailing_backslash() {
        assert_eq!(path_key(p(r"D:\Work\")), path_key(p(r"d:\work")));
        assert_eq!(child(p(r"D:\"), "a"), p(r"D:\a"));
        assert_eq!(child(p(r"D:\a"), "b"), p(r"D:\a\b"));
        assert_eq!(folder_name(p(r"D:\a\b")), "b");
        assert_eq!(folder_name(p(r"D:\")), "D:");
        assert_eq!(folder_name(p(r"\\192.168.1.2")), "192.168.1.2");
        assert_eq!(folder_name(p(r"\\server\share")), "share");
    }

    #[test]
    fn tells_a_server_from_its_shares_and_drives() {
        for path in [r"\\server", r"\\192.168.1.2\"] {
            assert!(is_server(p(path)), "{path}");
        }
        for path in [r"\\server\share", r"\\server\share\a", r"D:\", r"\\", ""] {
            assert!(!is_server(p(path)), "{path}");
        }
    }

    #[test]
    fn keeps_a_name_that_is_not_text_apart_from_its_replacement() {
        let odd = unpaired(p(r"D:\"), "Ab");
        // Shown with U+FFFD, as a name that really holds one is.
        let real = PathBuf::from("D:\\Ab\u{FFFD}X");
        assert_eq!(shown(&odd), shown(&real));
        assert_ne!(odd, real);
        assert_ne!(path_key(&odd), path_key(&real));
        // Still lowered around it.
        assert_eq!(path_key(&odd), path_key(&unpaired(p(r"d:\"), "aB")));
        let inner = child(&odd, "c");
        assert_eq!(parent_folder(&inner), Some(odd.as_path()));
        assert_eq!(ancestors(&inner).last(), Some(&inner));
        assert_eq!(folder_name(&inner), "c");
        assert_eq!(folder_name(&odd), odd.file_name().unwrap());
        let wide: Vec<u16> = odd.as_os_str().encode_wide().chain([0, 7]).collect();
        assert_eq!(from_wide(&wide), odd);
    }
}
