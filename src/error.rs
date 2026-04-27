//! Error type used throughout the crate.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the config-disassembler library.
#[derive(Debug)]
pub enum Error {
    /// I/O error while reading or writing a file.
    Io(io::Error),
    /// Failed to parse JSON.
    Json(serde_json::Error),
    /// Failed to parse JSON5.
    Json5(json5::Error),
    /// Failed to parse YAML.
    Yaml(serde_yaml::Error),
    /// Could not determine the file format from a path.
    UnknownFormat(PathBuf),
    /// CLI usage error.
    Usage(String),
    /// Logical error during disassembly or reassembly.
    Invalid(String),
    /// Error returned by the embedded xml-disassembler.
    Xml(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "i/o error: {e}"),
            Error::Json(e) => write!(f, "json error: {e}"),
            Error::Json5(e) => write!(f, "json5 error: {e}"),
            Error::Yaml(e) => write!(f, "yaml error: {e}"),
            Error::UnknownFormat(p) => {
                write!(
                    f,
                    "could not determine config format from path: {}",
                    p.display()
                )
            }
            Error::Usage(m) => write!(f, "{m}"),
            Error::Invalid(m) => write!(f, "{m}"),
            Error::Xml(m) => write!(f, "xml-disassembler: {m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Json5(e) => Some(e),
            Error::Yaml(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<json5::Error> for Error {
    fn from(e: json5::Error) -> Self {
        Error::Json5(e)
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(e: serde_yaml::Error) -> Self {
        Error::Yaml(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn display_covers_all_variants() {
        let e = Error::UnknownFormat(PathBuf::from("foo.txt"));
        assert!(e.to_string().contains("foo.txt"));
        assert!(Error::Usage("u".into()).to_string().contains("u"));
        assert!(Error::Invalid("i".into()).to_string().contains("i"));
        assert!(Error::Xml("x".into()).to_string().contains("xml"));

        let io_err = Error::from(io::Error::new(io::ErrorKind::NotFound, "missing"));
        assert!(io_err.to_string().contains("i/o error"));
        assert!(io_err.source().is_some());

        let json_err: Error = serde_json::from_str::<serde_json::Value>("{ not json")
            .unwrap_err()
            .into();
        assert!(json_err.to_string().contains("json"));
        assert!(json_err.source().is_some());

        let yaml_err: Error = serde_yaml::from_str::<serde_json::Value>("\t- :: bad")
            .unwrap_err()
            .into();
        assert!(yaml_err.to_string().contains("yaml"));
        assert!(yaml_err.source().is_some());

        let json5_err: Error = json5::from_str::<serde_json::Value>("{ not json5")
            .unwrap_err()
            .into();
        assert!(json5_err.to_string().contains("json5"));
        assert!(json5_err.source().is_some());

        // Variants without a wrapped source.
        assert!(Error::Usage("u".into()).source().is_none());
    }
}
