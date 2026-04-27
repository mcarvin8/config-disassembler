//! Format detection and serialization for JSON, JSON5, and YAML.
//!
//! All three formats are loaded into a common [`serde_json::Value`] so that
//! a file in one format can be re-emitted in any of the others.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde_json::Value;

use crate::error::{Error, Result};

/// Supported textual formats for the JSON-family disassembler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Json5,
    Yaml,
}

impl Format {
    /// Canonical file extension (without the leading dot).
    pub fn extension(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Json5 => "json5",
            Format::Yaml => "yaml",
        }
    }

    /// Best-effort detection of a format from a file path's extension.
    pub fn from_path(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("json") => Ok(Format::Json),
            Some("json5") => Ok(Format::Json5),
            Some("yaml" | "yml") => Ok(Format::Yaml),
            _ => Err(Error::UnknownFormat(path.to_path_buf())),
        }
    }

    /// Parse a string in this format into a generic [`Value`].
    pub fn parse(self, input: &str) -> Result<Value> {
        match self {
            Format::Json => Ok(serde_json::from_str(input)?),
            Format::Json5 => Ok(json5::from_str(input)?),
            Format::Yaml => Ok(serde_yaml::from_str(input)?),
        }
    }

    /// Serialize a [`Value`] in this format. The output is always
    /// pretty-printed with newline-terminated content.
    pub fn serialize(self, value: &Value) -> Result<String> {
        let mut out = match self {
            Format::Json => serde_json::to_string_pretty(value)?,
            Format::Json5 => json5::to_string(value)?,
            Format::Yaml => serde_yaml::to_string(value)?,
        };
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out)
    }

    /// Read and parse a file in this format.
    pub fn load(self, path: &Path) -> Result<Value> {
        let text = fs::read_to_string(path)?;
        self.parse(&text)
    }
}

impl FromStr for Format {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(Format::Json),
            "json5" => Ok(Format::Json5),
            "yaml" | "yml" => Ok(Format::Yaml),
            other => Err(Error::Usage(format!(
                "unknown format `{other}`; expected json, json5, or yaml"
            ))),
        }
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.extension())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_accepts_canonical_and_aliases() {
        assert_eq!("json".parse::<Format>().unwrap(), Format::Json);
        assert_eq!("JSON5".parse::<Format>().unwrap(), Format::Json5);
        assert_eq!("yaml".parse::<Format>().unwrap(), Format::Yaml);
        assert_eq!("yml".parse::<Format>().unwrap(), Format::Yaml);
    }

    #[test]
    fn from_str_rejects_unknown() {
        let err = "xml".parse::<Format>().unwrap_err();
        assert!(err.to_string().contains("unknown format"));
    }

    #[test]
    fn from_path_detects_supported_extensions() {
        assert_eq!(
            Format::from_path(Path::new("a.json")).unwrap(),
            Format::Json
        );
        assert_eq!(
            Format::from_path(Path::new("a.JSON5")).unwrap(),
            Format::Json5
        );
        assert_eq!(Format::from_path(Path::new("a.yml")).unwrap(), Format::Yaml);
    }

    #[test]
    fn from_path_rejects_missing_or_unknown_extension() {
        assert!(Format::from_path(Path::new("a")).is_err());
        assert!(Format::from_path(Path::new("a.toml")).is_err());
    }

    #[test]
    fn display_matches_extension() {
        assert_eq!(Format::Json.to_string(), "json");
        assert_eq!(Format::Json5.to_string(), "json5");
        assert_eq!(Format::Yaml.to_string(), "yaml");
    }

    #[test]
    fn parse_and_serialize_round_trip_for_all_formats() {
        for (fmt, text) in [
            (Format::Json, r#"{"a":1}"#),
            (Format::Json5, "{ a: 1 }"),
            (Format::Yaml, "a: 1\n"),
        ] {
            let v = fmt.parse(text).unwrap();
            let out = fmt.serialize(&v).unwrap();
            assert!(out.ends_with('\n'));
            assert_eq!(fmt.parse(&out).unwrap(), v);
        }
    }
}
