//! Error type used throughout the crate.

use std::fmt;

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the config-disassembler library.
#[derive(Debug)]
pub enum Error {
    /// CLI usage error.
    Usage(String),
    /// Error returned by the in-tree XML disassembler.
    Xml(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(m) => write!(f, "{m}"),
            Error::Xml(m) => write!(f, "xml-disassembler: {m}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn display_covers_all_variants() {
        assert_eq!(Error::Usage("u".into()).to_string(), "u");
        assert_eq!(Error::Xml("x".into()).to_string(), "xml-disassembler: x");
        assert!(Error::Usage("u".into()).source().is_none());
    }
}
