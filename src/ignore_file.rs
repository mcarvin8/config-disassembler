//! Shared ignore-file conventions for every subcommand.
//!
//! Every disassemble action accepts an `--ignore-path` flag pointing at a
//! `.gitignore`-style file. When the flag is omitted we look for
//! [`DEFAULT_IGNORE_FILENAME`] in the directory disassembly is run from,
//! falling back to the legacy `.xmldisassemblerignore` filename for the
//! `xml` subcommand only (with a deprecation warning) so existing setups
//! keep working.

use std::path::Path;

/// Default ignore-file name used by every format. Can be overridden via
/// the `--ignore-path` CLI flag.
pub const DEFAULT_IGNORE_FILENAME: &str = ".cdignore";

/// Legacy `xml-disassembler` ignore-file name. Kept as a fallback for the
/// `xml` subcommand so users migrating from the standalone crate aren't
/// silently broken.
pub const LEGACY_XML_IGNORE_FILENAME: &str = ".xmldisassemblerignore";

/// Resolve the ignore path for the `xml` subcommand.
///
/// * If the caller passed `--ignore-path`, return that unchanged.
/// * Else if [`DEFAULT_IGNORE_FILENAME`] does not exist but
///   [`LEGACY_XML_IGNORE_FILENAME`] does, log a deprecation warning and
///   return the legacy filename so existing XML setups keep working.
/// * Otherwise return [`DEFAULT_IGNORE_FILENAME`] (which the XML handler
///   tolerates as missing).
///
/// `cwd` is the directory the existence checks are resolved against,
/// normally the process working directory.
pub fn resolve_xml_ignore_path<'a>(
    explicit: Option<&'a str>,
    cwd: &Path,
) -> std::borrow::Cow<'a, str> {
    if let Some(p) = explicit {
        return std::borrow::Cow::Borrowed(p);
    }
    let new_default = cwd.join(DEFAULT_IGNORE_FILENAME);
    if !new_default.exists() {
        let legacy = cwd.join(LEGACY_XML_IGNORE_FILENAME);
        if legacy.exists() {
            log::warn!(
                "{LEGACY_XML_IGNORE_FILENAME} is deprecated; rename it to {DEFAULT_IGNORE_FILENAME} or pass --ignore-path explicitly."
            );
            return std::borrow::Cow::Owned(LEGACY_XML_IGNORE_FILENAME.to_string());
        }
    }
    std::borrow::Cow::Owned(DEFAULT_IGNORE_FILENAME.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_value_is_returned_unchanged() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = resolve_xml_ignore_path(Some("custom/.ignore"), &cwd);
        assert_eq!(resolved, "custom/.ignore");
    }

    #[test]
    fn returns_default_when_neither_file_exists() {
        let temp = tempfile::tempdir().unwrap();
        let resolved = resolve_xml_ignore_path(None, temp.path());
        assert_eq!(resolved, DEFAULT_IGNORE_FILENAME);
    }

    #[test]
    fn falls_back_to_legacy_when_only_legacy_exists() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(LEGACY_XML_IGNORE_FILENAME), "").unwrap();
        let resolved = resolve_xml_ignore_path(None, temp.path());
        assert_eq!(resolved, LEGACY_XML_IGNORE_FILENAME);
    }

    #[test]
    fn prefers_new_default_when_both_exist() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(DEFAULT_IGNORE_FILENAME), "").unwrap();
        std::fs::write(temp.path().join(LEGACY_XML_IGNORE_FILENAME), "").unwrap();
        let resolved = resolve_xml_ignore_path(None, temp.path());
        assert_eq!(resolved, DEFAULT_IGNORE_FILENAME);
    }
}
