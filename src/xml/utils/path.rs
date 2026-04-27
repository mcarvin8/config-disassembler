//! Path normalization for cross-platform consistency.

/// Normalize a path to Unix style (forward slashes).
/// Strips Windows extended path prefix `\\?\` if present so file operations
/// behave consistently across platforms.
pub fn normalize_path_unix(path: &str) -> String {
    let s = path.trim_start_matches(r"\\?\");
    s.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_unix_paths_unchanged() {
        assert_eq!(normalize_path_unix("foo/bar/baz"), "foo/bar/baz");
    }

    #[test]
    fn converts_backslashes_to_forward() {
        assert_eq!(normalize_path_unix(r"foo\bar\baz"), "foo/bar/baz");
    }

    #[test]
    fn strips_windows_extended_prefix() {
        assert_eq!(
            normalize_path_unix(r"\\?\C:\Users\file.xml"),
            "C:/Users/file.xml"
        );
    }
}
