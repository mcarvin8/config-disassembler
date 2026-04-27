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
    /// TOML is intentionally isolated from the other formats: TOML's
    /// syntactic constraints (no nulls, no array root, bare keys must
    /// precede tables) mean conversions through TOML can reorder or
    /// fail to represent values produced by JSON/JSON5/YAML. TOML files
    /// can therefore only be split into TOML files and reassembled into
    /// TOML.
    Toml,
}

impl Format {
    /// Canonical file extension (without the leading dot).
    pub fn extension(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Json5 => "json5",
            Format::Yaml => "yaml",
            Format::Toml => "toml",
        }
    }

    /// Whether this format participates in cross-format conversions.
    /// TOML is the only format that does not.
    pub fn is_cross_format_compatible(self) -> bool {
        !matches!(self, Format::Toml)
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
            Some("toml") => Ok(Format::Toml),
            _ => Err(Error::UnknownFormat(path.to_path_buf())),
        }
    }

    /// Parse a string in this format into a generic [`Value`].
    pub fn parse(self, input: &str) -> Result<Value> {
        match self {
            Format::Json => Ok(serde_json::from_str(input)?),
            Format::Json5 => Ok(json5::from_str(input)?),
            Format::Yaml => Ok(serde_yaml::from_str(input)?),
            Format::Toml => Ok(toml::from_str(input)?),
        }
    }

    /// Serialize a [`Value`] in this format. The output is always
    /// pretty-printed with newline-terminated content.
    pub fn serialize(self, value: &Value) -> Result<String> {
        let mut out = match self {
            Format::Json => serde_json::to_string_pretty(value)?,
            Format::Json5 => json5::to_string(value)?,
            Format::Yaml => serde_yaml::to_string(value)?,
            Format::Toml => serialize_toml(value)?,
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
            "toml" => Ok(Format::Toml),
            other => Err(Error::Usage(format!(
                "unknown format `{other}`; expected json, json5, yaml, or toml"
            ))),
        }
    }
}

/// Serialize a `Value` as TOML.
///
/// TOML cannot represent `null` and the document root must be a table,
/// so this function pre-validates and returns a clear error before
/// invoking the underlying TOML serializer.
fn serialize_toml(value: &Value) -> Result<String> {
    if !matches!(value, Value::Object(_)) {
        return Err(Error::Invalid(
            "TOML documents must have a table (object) root; got an array or scalar".into(),
        ));
    }
    if let Some(path) = find_null_path(value, "") {
        return Err(Error::Invalid(format!(
            "TOML cannot represent null values (found at `{}`)",
            if path.is_empty() { "<root>" } else { &path }
        )));
    }
    // Pre-validation above (root must be a table, no null values) covers
    // every case the `toml` crate would reject for a `serde_json::Value`
    // constructed through the normal serde API, so a serialization error
    // here would indicate an unexpected toml-crate behavior; surface it
    // with a clear `Invalid` error rather than a dedicated variant.
    toml::to_string_pretty(value).map_err(|e| Error::Invalid(format!("toml serialize error: {e}")))
}

/// Walks a `Value` and returns the first dotted path to a `Null`, if any.
fn find_null_path(value: &Value, prefix: &str) -> Option<String> {
    match value {
        Value::Null => Some(prefix.to_string()),
        Value::Object(map) => {
            for (k, v) in map {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                if let Some(p) = find_null_path(v, &next) {
                    return Some(p);
                }
            }
            None
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                let next = format!("{prefix}[{i}]");
                if let Some(p) = find_null_path(v, &next) {
                    return Some(p);
                }
            }
            None
        }
        _ => None,
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
        assert_eq!("toml".parse::<Format>().unwrap(), Format::Toml);
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
        assert_eq!(
            Format::from_path(Path::new("a.toml")).unwrap(),
            Format::Toml
        );
    }

    #[test]
    fn from_path_rejects_missing_or_unknown_extension() {
        assert!(Format::from_path(Path::new("a")).is_err());
        assert!(Format::from_path(Path::new("a.ini")).is_err());
    }

    #[test]
    fn display_matches_extension() {
        assert_eq!(Format::Json.to_string(), "json");
        assert_eq!(Format::Json5.to_string(), "json5");
        assert_eq!(Format::Yaml.to_string(), "yaml");
        assert_eq!(Format::Toml.to_string(), "toml");
    }

    #[test]
    fn parse_and_serialize_round_trip_for_all_formats() {
        for (fmt, text) in [
            (Format::Json, r#"{"a":1}"#),
            (Format::Json5, "{ a: 1 }"),
            (Format::Yaml, "a: 1\n"),
            (Format::Toml, "a = 1\n"),
        ] {
            let v = fmt.parse(text).unwrap();
            let out = fmt.serialize(&v).unwrap();
            assert!(out.ends_with('\n'));
            assert_eq!(fmt.parse(&out).unwrap(), v);
        }
    }

    #[test]
    fn toml_rejects_array_root() {
        let v: Value = serde_json::json!([1, 2, 3]);
        let err = Format::Toml.serialize(&v).unwrap_err();
        assert!(err.to_string().contains("table"), "got: {err}");
    }

    #[test]
    fn toml_rejects_null_values() {
        let v: Value = serde_json::json!({ "outer": { "inner": null } });
        let err = Format::Toml.serialize(&v).unwrap_err();
        assert!(err.to_string().contains("null"), "got: {err}");
        assert!(err.to_string().contains("outer.inner"), "got: {err}");
    }

    #[test]
    fn toml_rejects_null_inside_array() {
        let v: Value = serde_json::json!({ "items": [1, null, 3] });
        let err = Format::Toml.serialize(&v).unwrap_err();
        assert!(err.to_string().contains("null"), "got: {err}");
        assert!(err.to_string().contains("items[1]"), "got: {err}");
    }

    #[test]
    fn cross_format_compatibility_excludes_toml() {
        assert!(Format::Json.is_cross_format_compatible());
        assert!(Format::Json5.is_cross_format_compatible());
        assert!(Format::Yaml.is_cross_format_compatible());
        assert!(!Format::Toml.is_cross_format_compatible());
    }
}
