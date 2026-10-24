//! Windows paths as Colet compares and walks them.

/// Every folder from the root down to `path`: a drive root keeps its trailing backslash (`D:\`), a UNC
/// path starts at its share (`\\server\share`).
pub fn ancestors(path: &str) -> Vec<String> {
    if path.is_empty() {
        return Vec::new();
    }
    let parts: Vec<&str> = path.trim_end_matches('\\').split('\\').collect();
    let unc = path.starts_with("\\\\");
    let root = if unc { parts[..parts.len().min(4)].join("\\") } else { format!("{}\\", parts[0]) };
    let mut paths = vec![root];
    for part in &parts[if unc { 4.min(parts.len()) } else { 1 }..] {
        let next = child(paths.last().unwrap(), part);
        paths.push(next);
    }
    paths
}

/// The folder holding `path`, as [`ancestors`] names it but read from the path itself, with nothing made:
/// `None` for a root (`D:\`, `\\server\share`) or nothing. A drive's own folders name it `D:` here, where
/// `ancestors` gives `D:\`; one or the other alike stands for it.
pub fn parent_folder(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('\\');
    let root = if path.starts_with("\\\\") { 4 } else { 1 };
    if path.is_empty() || trimmed.split('\\').nth(root).is_none() {
        return None;
    }
    trimmed.rsplit_once('\\').map(|(parent, _)| parent)
}

/// A path compared without case or the drive root's trailing backslash.
pub fn path_key(path: &str) -> String {
    path.trim_end_matches('\\').to_lowercase()
}

/// `name` inside `parent`; a drive root already ends in a backslash.
pub fn child(parent: &str, name: &str) -> String {
    if parent.ends_with('\\') { format!("{parent}{name}") } else { format!("{parent}\\{name}") }
}

/// The last part of a path, or the path itself for a root: `D:` for `D:\`.
pub fn folder_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('\\');
    match trimmed.rsplit('\\').next() {
        Some(name) if !name.is_empty() => name,
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_folder_from_the_root() {
        assert_eq!(ancestors(r"D:\a\b"), [r"D:\", r"D:\a", r"D:\a\b"]);
        assert_eq!(ancestors(r"D:\"), [r"D:\"]);
        assert_eq!(ancestors(r"\\server\share\a"), [r"\\server\share", r"\\server\share\a"]);
        assert!(ancestors("").is_empty());
    }

    #[test]
    fn finds_the_parent_where_ancestors_would_without_making_them() {
        let paths = [
            r"D:\", "D:", r"D:\a", r"D:\a\", r"D:\a\b", r"\\server\share", r"\\server\share\", r"\\server",
            r"\\server\share\a", r"\\server\share\a\b", "", "search:ext:pdf", r"search:a\b",
        ];
        for path in paths {
            let made = ancestors(path);
            // A root, or nothing, has no folder above it.
            assert_eq!(parent_folder(path).is_none(), made.len() < 2, "{path}");
            if let Some(parent) = parent_folder(path) {
                assert_eq!(parent.trim_end_matches('\\'), made[made.len() - 2].trim_end_matches('\\'), "{path}");
            }
        }
        assert_eq!(parent_folder(r"D:\a\b"), Some(r"D:\a"));
        assert_eq!(parent_folder(r"D:\a"), Some("D:"));
        assert_eq!(parent_folder(r"\\server\share\a"), Some(r"\\server\share"));
    }

    #[test]
    fn compares_without_case_or_trailing_backslash() {
        assert_eq!(path_key(r"D:\Work\"), path_key(r"d:\work"));
        assert_eq!(child(r"D:\", "a"), r"D:\a");
        assert_eq!(child(r"D:\a", "b"), r"D:\a\b");
        assert_eq!(folder_name(r"D:\a\b"), "b");
        assert_eq!(folder_name(r"D:\"), "D:");
    }
}
