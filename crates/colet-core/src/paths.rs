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
    fn compares_without_case_or_trailing_backslash() {
        assert_eq!(path_key(r"D:\Work\"), path_key(r"d:\work"));
        assert_eq!(child(r"D:\", "a"), r"D:\a");
        assert_eq!(child(r"D:\a", "b"), r"D:\a\b");
        assert_eq!(folder_name(r"D:\a\b"), "b");
        assert_eq!(folder_name(r"D:\"), "D:");
    }
}
