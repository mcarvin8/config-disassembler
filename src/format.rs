//! Format detection, capabilities, and serialization for value-model formats.
//!
//! Each format in this module is loaded into a common [`serde_json::Value`].
//! Conversion rules are expressed as format capabilities so adding another
//! value-model format only requires registering its aliases, extensions,
//! conversion family, and serializer/parser here.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use configparser::ini::Ini;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Value;

use crate::error::{Error, Result};

/// Supported textual formats for the value-model disassembler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Json,
    Json5,
    Jsonc,
    Yaml,
    Toon,
    /// TOML is intentionally isolated from the JSON-value formats: TOML's
    /// syntactic constraints (no nulls, no array root, bare keys must
    /// precede tables) mean conversions through TOML can reorder or
    /// fail to represent values produced by JSON/JSON5/JSONC/YAML/TOON.
    /// TOML files can therefore only be split into TOML files and
    /// reassembled into TOML.
    Toml,
    /// INI uses the same table-document split layout as TOML, but is even
    /// narrower: section values are strings (or valueless keys) and deeper
    /// nesting/arrays cannot be represented without inventing an encoding.
    Ini,
}

/// A family of formats that can safely convert among themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatFamily {
    JsonValue,
    Toml,
    Ini,
}

/// Which operation is checking a conversion edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionOperation {
    Convert,
    Reassemble,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitPayloadLayout {
    Direct,
    WrappedByParentKey,
}

struct FormatSpec {
    canonical_name: &'static str,
    display_name: &'static str,
    aliases: &'static [&'static str],
    extensions: &'static [&'static str],
    family: FormatFamily,
    split_payload_layout: SplitPayloadLayout,
}

impl Format {
    /// All formats handled by the value-model disassembler.
    pub const ALL: &'static [Format] = &[
        Format::Json,
        Format::Json5,
        Format::Jsonc,
        Format::Yaml,
        Format::Toon,
        Format::Toml,
        Format::Ini,
    ];

    const JSON_VALUE_FAMILY: &'static [Format] = &[
        Format::Json,
        Format::Json5,
        Format::Jsonc,
        Format::Yaml,
        Format::Toon,
    ];
    const TOML_FAMILY: &'static [Format] = &[Format::Toml];
    const INI_FAMILY: &'static [Format] = &[Format::Ini];

    fn spec(self) -> &'static FormatSpec {
        match self {
            Format::Json => &FormatSpec {
                canonical_name: "json",
                display_name: "JSON",
                aliases: &["json"],
                extensions: &["json"],
                family: FormatFamily::JsonValue,
                split_payload_layout: SplitPayloadLayout::Direct,
            },
            Format::Json5 => &FormatSpec {
                canonical_name: "json5",
                display_name: "JSON5",
                aliases: &["json5"],
                extensions: &["json5"],
                family: FormatFamily::JsonValue,
                split_payload_layout: SplitPayloadLayout::Direct,
            },
            Format::Jsonc => &FormatSpec {
                canonical_name: "jsonc",
                display_name: "JSONC",
                aliases: &["jsonc"],
                extensions: &["jsonc"],
                family: FormatFamily::JsonValue,
                split_payload_layout: SplitPayloadLayout::Direct,
            },
            Format::Yaml => &FormatSpec {
                canonical_name: "yaml",
                display_name: "YAML",
                aliases: &["yaml", "yml"],
                extensions: &["yaml", "yml"],
                family: FormatFamily::JsonValue,
                split_payload_layout: SplitPayloadLayout::Direct,
            },
            Format::Toon => &FormatSpec {
                canonical_name: "toon",
                display_name: "TOON",
                aliases: &["toon"],
                extensions: &["toon"],
                family: FormatFamily::JsonValue,
                split_payload_layout: SplitPayloadLayout::Direct,
            },
            Format::Toml => &FormatSpec {
                canonical_name: "toml",
                display_name: "TOML",
                aliases: &["toml"],
                extensions: &["toml"],
                family: FormatFamily::Toml,
                split_payload_layout: SplitPayloadLayout::WrappedByParentKey,
            },
            Format::Ini => &FormatSpec {
                canonical_name: "ini",
                display_name: "INI",
                aliases: &["ini"],
                extensions: &["ini"],
                family: FormatFamily::Ini,
                split_payload_layout: SplitPayloadLayout::WrappedByParentKey,
            },
        }
    }

    /// Canonical file extension (without the leading dot).
    pub fn extension(self) -> &'static str {
        self.spec().canonical_name
    }

    /// Canonical lower-case name used in CLI and metadata.
    pub fn canonical_name(self) -> &'static str {
        self.spec().canonical_name
    }

    /// Human-facing display name.
    pub fn display_name(self) -> &'static str {
        self.spec().display_name
    }

    /// Accepted names for CLI parsing.
    pub fn aliases(self) -> &'static [&'static str] {
        self.spec().aliases
    }

    /// File extensions that identify this format.
    pub fn extensions(self) -> &'static [&'static str] {
        self.spec().extensions
    }

    /// The conversion family this format belongs to.
    pub fn family(self) -> FormatFamily {
        self.spec().family
    }

    /// Formats that can safely convert to/from this format.
    pub fn compatible_formats(self) -> &'static [Format] {
        match self.family() {
            FormatFamily::JsonValue => Self::JSON_VALUE_FAMILY,
            FormatFamily::Toml => Self::TOML_FAMILY,
            FormatFamily::Ini => Self::INI_FAMILY,
        }
    }

    /// Whether CLI `--input-format` / `--output-format` flags are useful
    /// for this subcommand.
    pub fn allows_format_overrides(self) -> bool {
        self.compatible_formats().len() > 1
    }

    /// Whether this format participates in cross-format conversions.
    pub fn is_cross_format_compatible(self) -> bool {
        self.allows_format_overrides()
    }

    /// Whether this format can be converted into `output`.
    pub fn can_convert_to(self, output: Format) -> bool {
        self.family() == output.family()
    }

    /// Return a clear error if a conversion edge is not allowed.
    pub fn ensure_can_convert_to(
        self,
        output: Format,
        operation: ConversionOperation,
    ) -> Result<()> {
        if self.can_convert_to(output) {
            return Ok(());
        }

        if let Some(name) = self
            .family()
            .isolated_format_name()
            .or_else(|| output.family().isolated_format_name())
        {
            return match operation {
                ConversionOperation::Convert => Err(Error::Invalid(format!(
                    "{name} can only be converted to and from {name}; got input={self}, output={output}"
                ))),
                ConversionOperation::Reassemble => Err(Error::Invalid(format!(
                    "{name} can only be reassembled to and from {name}; the disassembled \
                     directory was written in {self} but reassembly target is {output}"
                ))),
            };
        }

        Err(Error::Invalid(format!(
            "conversion from {self} to {output} is not supported"
        )))
    }

    /// Best-effort detection of a format from a file path's extension.
    pub fn from_path(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        if let Some(ext) = ext.as_deref() {
            for format in Self::ALL {
                if format.extensions().contains(&ext) {
                    return Ok(*format);
                }
            }
        }
        Err(Error::UnknownFormat(path.to_path_buf()))
    }

    /// Parse a string in this format into a generic [`Value`].
    pub fn parse(self, input: &str) -> Result<Value> {
        match self {
            Format::Json => Ok(serde_json::from_str(input)?),
            Format::Json5 => Ok(json5::from_str(input)?),
            Format::Jsonc => parse_jsonc(input),
            Format::Yaml => Ok(serde_yaml::from_str(input)?),
            Format::Toon => toon_format::decode_default(input)
                .map_err(|e| Error::Invalid(format!("toon parse error: {e}"))),
            Format::Toml => Ok(toml::from_str(input)?),
            Format::Ini => parse_ini(input),
        }
    }

    /// Serialize a [`Value`] in this format. The output is always
    /// pretty-printed with newline-terminated content.
    pub fn serialize(self, value: &Value) -> Result<String> {
        let mut out = match self {
            Format::Json => serde_json::to_string_pretty(value)?,
            Format::Json5 => json5::to_string(value)?,
            // JSON is a valid JSONC document. Comments from input files are
            // treated as syntax and are not preserved in the value model.
            Format::Jsonc => serde_json::to_string_pretty(value)?,
            Format::Yaml => serde_yaml::to_string(value)?,
            Format::Toon => toon_format::encode_default(value)
                .map_err(|e| Error::Invalid(format!("toon serialize error: {e}")))?,
            Format::Toml => serialize_toml(value)?,
            Format::Ini => serialize_ini(value)?,
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

    /// Prepare a per-key split payload for this format.
    ///
    /// Most formats can write the payload value directly. TOML wraps the
    /// payload under its parent key so every split file remains a valid TOML
    /// table document.
    pub fn wrap_split_payload(self, key: &str, value: &Value) -> Value {
        match self.spec().split_payload_layout {
            SplitPayloadLayout::Direct => value.clone(),
            SplitPayloadLayout::WrappedByParentKey => {
                let mut wrapper = Map::new();
                wrapper.insert(key.to_string(), value.clone());
                Value::Object(wrapper)
            }
        }
    }

    /// Reverse [`Format::wrap_split_payload`] while reassembling.
    pub fn unwrap_split_payload(self, key: &str, filename: &str, loaded: Value) -> Result<Value> {
        match self.spec().split_payload_layout {
            SplitPayloadLayout::Direct => Ok(loaded),
            SplitPayloadLayout::WrappedByParentKey => {
                let Value::Object(mut map) = loaded else {
                    return Err(Error::Invalid(format!(
                        "{} file `{filename}` did not deserialize to a table",
                        self.display_name()
                    )));
                };
                map.remove(key).ok_or_else(|| {
                    Error::Invalid(format!(
                        "{} file `{filename}` does not contain expected wrapper key `{key}`",
                        self.display_name()
                    ))
                })
            }
        }
    }

    /// Canonical CLI names for all registered formats.
    pub fn supported_format_list() -> String {
        Self::ALL
            .iter()
            .map(|f| f.canonical_name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl FromStr for Format {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.to_ascii_lowercase();
        for format in Format::ALL {
            if format.aliases().contains(&s.as_str()) {
                return Ok(*format);
            }
        }
        Err(Error::Usage(format!(
            "unknown format `{s}`; expected {}",
            Format::supported_format_list()
        )))
    }
}

impl FormatFamily {
    fn isolated_format_name(self) -> Option<&'static str> {
        match self {
            FormatFamily::JsonValue => None,
            FormatFamily::Toml => Some("TOML"),
            FormatFamily::Ini => Some("INI"),
        }
    }
}

const INI_DEFAULT_SECTION: &str = "__config_disassembler_root__";

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

/// Parse an INI document into the common value model.
///
/// Keys outside any section become top-level scalar keys. Section entries
/// become one-level nested objects. INI values are strings; valueless keys are
/// represented as `null` so they can round-trip through the same format.
fn parse_ini(input: &str) -> Result<Value> {
    let mut ini = new_ini();
    let parsed = ini
        .read(input.to_string())
        .map_err(|e| Error::Invalid(format!("ini parse error: {e}")))?;
    let mut root = Map::new();

    for (section, values) in parsed {
        if section == INI_DEFAULT_SECTION {
            for (key, value) in values {
                root.insert(key, ini_value_to_json(value));
            }
            continue;
        }

        let mut section_object = Map::new();
        for (key, value) in values {
            section_object.insert(key, ini_value_to_json(value));
        }
        root.insert(section, Value::Object(section_object));
    }

    Ok(Value::Object(root))
}

fn serialize_ini(value: &Value) -> Result<String> {
    let Value::Object(map) = value else {
        return Err(Error::Invalid(
            "INI documents must have an object root; got an array or scalar".into(),
        ));
    };

    let mut ini = new_ini();
    for (key, value) in map {
        match value {
            Value::Object(section) => {
                // Preserve empty sections. Without this, a `[section]` with
                // no keys would serialize as an empty file and fail unwrap.
                ini.get_mut_map().entry(key.clone()).or_default();
                for (section_key, section_value) in section {
                    ini.set(
                        key,
                        section_key,
                        ini_scalar_value(section_value, &format!("{key}.{section_key}"))?,
                    );
                }
            }
            _ => {
                ini.set(
                    INI_DEFAULT_SECTION,
                    key,
                    ini_scalar_value(value, key.as_str())?,
                );
            }
        }
    }

    Ok(ini.writes())
}

fn new_ini() -> Ini {
    let mut ini = Ini::new_cs();
    ini.set_default_section(INI_DEFAULT_SECTION);
    ini.set_multiline(true);
    ini
}

fn ini_value_to_json(value: Option<String>) -> Value {
    match value {
        Some(value) => Value::String(value),
        None => Value::Null,
    }
}

fn ini_scalar_value(value: &Value, path: &str) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::Bool(value) => Ok(Some(value.to_string())),
        Value::Number(value) => Ok(Some(value.to_string())),
        Value::String(value) => Ok(Some(value.clone())),
        Value::Array(_) | Value::Object(_) => Err(Error::Invalid(format!(
            "INI can only represent scalar values at the document root or one level of sections \
             (found unsupported value at `{path}`)"
        ))),
    }
}

/// Parse JSONC as JSON plus comments and trailing commas.
///
/// The upstream parser defaults are intentionally loose, so keep the accepted
/// syntax close to JSONC rather than expanding this into JSON5.
fn parse_jsonc(input: &str) -> Result<Value> {
    jsonc_parser::parse_to_serde_value(input, &jsonc_parse_options())
        .map_err(|e| Error::Invalid(format!("jsonc parse error: {e}")))
}

pub(crate) fn jsonc_parse_options() -> jsonc_parser::ParseOptions {
    jsonc_parser::ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
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
        f.write_str(self.canonical_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_accepts_canonical_and_aliases() {
        assert_eq!("json".parse::<Format>().unwrap(), Format::Json);
        assert_eq!("JSON5".parse::<Format>().unwrap(), Format::Json5);
        assert_eq!("jsonc".parse::<Format>().unwrap(), Format::Jsonc);
        assert_eq!("yaml".parse::<Format>().unwrap(), Format::Yaml);
        assert_eq!("yml".parse::<Format>().unwrap(), Format::Yaml);
        assert_eq!("toon".parse::<Format>().unwrap(), Format::Toon);
        assert_eq!("toml".parse::<Format>().unwrap(), Format::Toml);
        assert_eq!("ini".parse::<Format>().unwrap(), Format::Ini);
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
        assert_eq!(
            Format::from_path(Path::new("a.JSONC")).unwrap(),
            Format::Jsonc
        );
        assert_eq!(Format::from_path(Path::new("a.yml")).unwrap(), Format::Yaml);
        assert_eq!(
            Format::from_path(Path::new("a.toon")).unwrap(),
            Format::Toon
        );
        assert_eq!(
            Format::from_path(Path::new("a.toml")).unwrap(),
            Format::Toml
        );
        assert_eq!(Format::from_path(Path::new("a.ini")).unwrap(), Format::Ini);
    }

    #[test]
    fn from_path_rejects_missing_or_unknown_extension() {
        assert!(Format::from_path(Path::new("a")).is_err());
        assert!(Format::from_path(Path::new("a.txt")).is_err());
    }

    #[test]
    fn display_matches_extension() {
        assert_eq!(Format::Json.to_string(), "json");
        assert_eq!(Format::Json5.to_string(), "json5");
        assert_eq!(Format::Jsonc.to_string(), "jsonc");
        assert_eq!(Format::Yaml.to_string(), "yaml");
        assert_eq!(Format::Toon.to_string(), "toon");
        assert_eq!(Format::Toml.to_string(), "toml");
        assert_eq!(Format::Ini.to_string(), "ini");
    }

    #[test]
    fn parse_and_serialize_round_trip_for_all_formats() {
        for (fmt, text) in [
            (Format::Json, r#"{"a":1}"#),
            (Format::Json5, "{ a: 1 }"),
            (Format::Jsonc, "{ \"a\": 1, } // kept as syntax only"),
            (Format::Yaml, "a: 1\n"),
            (Format::Toon, "a: 1\n"),
            (Format::Toml, "a = 1\n"),
            (Format::Ini, "a=1\n"),
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
        assert!(Format::Jsonc.is_cross_format_compatible());
        assert!(Format::Yaml.is_cross_format_compatible());
        assert!(Format::Toon.is_cross_format_compatible());
        assert!(!Format::Toml.is_cross_format_compatible());
        assert!(!Format::Ini.is_cross_format_compatible());
    }

    #[test]
    fn compatible_formats_are_grouped_by_conversion_family() {
        assert_eq!(
            Format::Json.compatible_formats(),
            &[
                Format::Json,
                Format::Json5,
                Format::Jsonc,
                Format::Yaml,
                Format::Toon
            ]
        );
        assert_eq!(Format::Toml.compatible_formats(), &[Format::Toml]);
        assert_eq!(Format::Ini.compatible_formats(), &[Format::Ini]);
    }

    #[test]
    fn jsonc_accepts_comments_and_trailing_commas_only() {
        let parsed = Format::Jsonc
            .parse(
                r#"{
  // JSONC comment
  "name": "demo",
  "items": [1, 2,],
}"#,
            )
            .unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({ "name": "demo", "items": [1, 2] })
        );

        let err = Format::Jsonc.parse("{ name: 'json5-only' }").unwrap_err();
        assert!(err.to_string().contains("jsonc parse error"));
    }

    #[test]
    fn conversion_rules_reject_cross_family_edges() {
        assert!(Format::Json
            .ensure_can_convert_to(Format::Yaml, ConversionOperation::Convert)
            .is_ok());
        let err = Format::Json
            .ensure_can_convert_to(Format::Toml, ConversionOperation::Convert)
            .unwrap_err();
        assert!(err.to_string().contains("TOML can only be converted"));

        let err = Format::Json
            .ensure_can_convert_to(Format::Ini, ConversionOperation::Convert)
            .unwrap_err();
        assert!(err.to_string().contains("INI can only be converted"));
    }

    #[test]
    fn split_payload_wrapping_is_capability_driven() {
        let value = serde_json::json!([{ "host": "a" }]);
        assert_eq!(Format::Json.wrap_split_payload("servers", &value), value);

        let wrapped = Format::Toml.wrap_split_payload("servers", &value);
        assert_eq!(wrapped, serde_json::json!({ "servers": value }));
        assert_eq!(
            Format::Toml
                .unwrap_split_payload("servers", "servers.toml", wrapped)
                .unwrap(),
            serde_json::json!([{ "host": "a" }])
        );

        let wrapped = Format::Ini.wrap_split_payload("servers", &value);
        assert_eq!(wrapped, serde_json::json!({ "servers": value }));
        assert_eq!(
            Format::Ini
                .unwrap_split_payload("servers", "servers.ini", wrapped)
                .unwrap(),
            serde_json::json!([{ "host": "a" }])
        );
    }

    #[test]
    fn ini_parse_maps_top_level_keys_and_sections() {
        let parsed = Format::Ini
            .parse(
                r#"
name = demo
enabled

[Database]
host = db.example.com
port = 5432
"#,
            )
            .unwrap();

        assert_eq!(
            parsed,
            serde_json::json!({
                "name": "demo",
                "enabled": null,
                "Database": {
                    "host": "db.example.com",
                    "port": "5432"
                }
            })
        );
    }

    #[test]
    fn ini_serialize_rejects_arrays_and_deeply_nested_objects() {
        let array = serde_json::json!({ "items": ["a", "b"] });
        let err = Format::Ini.serialize(&array).unwrap_err();
        assert!(err.to_string().contains("items"), "got: {err}");

        let nested = serde_json::json!({ "section": { "child": { "x": "y" } } });
        let err = Format::Ini.serialize(&nested).unwrap_err();
        assert!(err.to_string().contains("section.child"), "got: {err}");
    }

    #[test]
    fn ini_preserves_empty_sections() {
        let value = serde_json::json!({ "empty": {} });
        let out = Format::Ini.serialize(&value).unwrap();
        assert!(out.contains("[empty]"), "got: {out}");
        assert_eq!(Format::Ini.parse(&out).unwrap(), value);
    }
}
