//! Reassemble a directory of split files (produced by [`disassemble`])
//! back into a single configuration file.
//!
//! [`disassemble`]: crate::disassemble::disassemble

use std::fs;
use std::path::{Path, PathBuf};

use jsonc_parser::ast;
use jsonc_parser::common::Ranged;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::format::{jsonc_parse_options, ConversionOperation, Format};
use crate::meta::{Meta, Root};

/// Options controlling reassembly.
#[derive(Debug, Clone)]
pub struct ReassembleOptions {
    /// Directory containing the disassembled files and metadata.
    pub input_dir: PathBuf,
    /// Path to write the reassembled file to. If `None`, written next to
    /// the input directory using the original source filename (or the
    /// directory name with the chosen format's extension).
    pub output: Option<PathBuf>,
    /// Format to write the reassembled file in. Defaults to the format
    /// recorded as the original source format in the metadata.
    pub output_format: Option<Format>,
    /// Remove the input directory after a successful reassembly.
    pub post_purge: bool,
}

/// Reassemble a configuration file from a disassembled directory.
///
/// Returns the path of the reassembled output file.
pub fn reassemble(opts: ReassembleOptions) -> Result<PathBuf> {
    let dir = &opts.input_dir;
    if !dir.is_dir() {
        return Err(Error::Invalid(format!(
            "input is not a directory: {}",
            dir.display()
        )));
    }
    let meta = Meta::read(dir)?;
    let file_format = meta.file_format;
    let output_format: Format = opts.output_format.unwrap_or(meta.source_format);

    file_format.ensure_can_convert_to(output_format, ConversionOperation::Reassemble)?;

    let output_path = match opts.output.clone() {
        Some(p) => p,
        None => default_output_path(dir, &meta, output_format)?,
    };
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    if file_format == Format::Jsonc && output_format == Format::Jsonc {
        fs::write(&output_path, assemble_jsonc_preserving(dir, &meta)?)?;
    } else {
        let value = match &meta.root {
            Root::Object {
                key_order,
                key_files,
                main_file,
            } => assemble_object(dir, key_order, key_files, main_file.as_deref(), file_format)?,
            Root::Array { files } => assemble_array(dir, files, file_format)?,
        };
        fs::write(&output_path, output_format.serialize(&value)?)?;
    }

    if opts.post_purge {
        fs::remove_dir_all(dir)?;
    }
    Ok(output_path)
}

fn assemble_object(
    dir: &Path,
    key_order: &[String],
    key_files: &std::collections::BTreeMap<String, String>,
    main_file: Option<&str>,
    file_format: Format,
) -> Result<Value> {
    let main_object: Map<String, Value> = match main_file {
        Some(name) => match file_format.load(&dir.join(name))? {
            Value::Object(map) => map,
            _ => {
                return Err(Error::Invalid(format!(
                    "main scalar file {name} did not contain an object"
                )));
            }
        },
        None => Map::new(),
    };

    let mut out = Map::new();
    for key in key_order {
        if let Some(filename) = key_files.get(key) {
            let loaded = file_format.load(&dir.join(filename))?;
            let value = unwrap_per_key_payload(file_format, key, filename, loaded)?;
            out.insert(key.clone(), value);
        } else if let Some(value) = main_object.get(key) {
            out.insert(key.clone(), value.clone());
        } else {
            return Err(Error::Invalid(format!(
                "metadata references key `{key}` but no file or scalar found"
            )));
        }
    }
    Ok(Value::Object(out))
}

fn unwrap_per_key_payload(
    file_format: Format,
    key: &str,
    filename: &str,
    loaded: Value,
) -> Result<Value> {
    file_format.unwrap_split_payload(key, filename, loaded)
}

fn assemble_array(dir: &Path, files: &[String], file_format: Format) -> Result<Value> {
    let mut items = Vec::with_capacity(files.len());
    for name in files {
        items.push(file_format.load(&dir.join(name))?);
    }
    Ok(Value::Array(items))
}

fn assemble_jsonc_preserving(dir: &Path, meta: &Meta) -> Result<String> {
    match &meta.root {
        Root::Object {
            key_order,
            key_files,
            main_file,
        } => assemble_jsonc_object(dir, key_order, key_files, main_file.as_deref()),
        Root::Array { files } => assemble_jsonc_array(dir, files),
    }
}

fn assemble_jsonc_object(
    dir: &Path,
    key_order: &[String],
    key_files: &std::collections::BTreeMap<String, String>,
    main_file: Option<&str>,
) -> Result<String> {
    let main_properties = match main_file {
        Some(name) => {
            let text = fs::read_to_string(dir.join(name))?;
            let ast = parse_jsonc_ast(&text)?;
            let ast::Value::Object(object) = ast else {
                return Err(Error::Invalid(format!(
                    "main scalar file {name} did not contain an object"
                )));
            };
            jsonc_object_properties(&text, object)
        }
        None => Vec::new(),
    };

    let mut segments = Vec::with_capacity(key_order.len());
    for key in key_order {
        if let Some(filename) = key_files.get(key) {
            let path = dir.join(filename);
            let text = fs::read_to_string(&path)?;
            Format::Jsonc.load(&path)?;
            segments.push(render_jsonc_property(key, &text)?);
        } else if let Some(property) = main_properties.iter().find(|property| &property.key == key)
        {
            segments.push(property.segment.clone());
        } else {
            return Err(Error::Invalid(format!(
                "metadata references key `{key}` but no file or scalar found"
            )));
        }
    }

    Ok(render_jsonc_object(segments.iter()))
}

fn assemble_jsonc_array(dir: &Path, files: &[String]) -> Result<String> {
    let mut segments = Vec::with_capacity(files.len());
    for name in files {
        let path = dir.join(name);
        let text = fs::read_to_string(&path)?;
        Format::Jsonc.load(&path)?;
        segments.push(render_jsonc_array_element(&text));
    }
    Ok(render_jsonc_array(segments.iter()))
}

struct JsoncPropertySyntax {
    key: String,
    segment: String,
}

fn jsonc_object_properties(text: &str, object: ast::Object<'_>) -> Vec<JsoncPropertySyntax> {
    object
        .properties
        .into_iter()
        .map(|property| {
            let key = property.name.clone().into_string();
            let property_range = property.range();
            let value_range = property.value.range();
            JsoncPropertySyntax {
                key,
                segment: jsonc_property_segment(text, property_range.start, value_range.end)
                    .to_string(),
            }
        })
        .collect()
}

fn parse_jsonc_ast(text: &str) -> Result<ast::Value<'_>> {
    jsonc_parser::parse_to_ast(text, &Default::default(), &jsonc_parse_options())
        .map_err(|e| Error::Invalid(format!("jsonc parse error: {e}")))?
        .value
        .ok_or_else(|| Error::Invalid("JSONC document did not contain a value".into()))
}

fn jsonc_property_segment(text: &str, property_start: usize, value_end: usize) -> &str {
    let start = leading_comment_start(text, line_start(text, property_start));
    let end = line_end(text, value_end);
    &text[start..end]
}

fn leading_comment_start(text: &str, mut start: usize) -> usize {
    while start > 0 {
        let previous_line_end = start.saturating_sub(1);
        let previous_line_start = line_start(text, previous_line_end);
        let line = &text[previous_line_start..previous_line_end];
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/")
        {
            start = previous_line_start;
        } else {
            break;
        }
    }
    start
}

fn line_start(text: &str, pos: usize) -> usize {
    text[..pos].rfind('\n').map(|idx| idx + 1).unwrap_or(0)
}

fn line_end(text: &str, pos: usize) -> usize {
    text[pos..]
        .find('\n')
        .map(|idx| pos + idx)
        .unwrap_or(text.len())
}

fn render_jsonc_property(key: &str, value_text: &str) -> Result<String> {
    let key = serde_json::to_string(key)?;
    let value_text = value_text.trim_matches(|c| c == '\r' || c == '\n');
    let mut lines = value_text.lines();
    let first = lines.next().unwrap_or("");
    let mut out = format!("  {key}: {first}");
    for line in lines {
        out.push('\n');
        out.push_str(line);
    }
    Ok(jsonc_segment_with_comma(&out))
}

fn render_jsonc_array_element(value_text: &str) -> String {
    let value_text = value_text.trim_matches(|c| c == '\r' || c == '\n');
    let mut out = String::new();
    for (idx, line) in value_text.lines().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str("  ");
        out.push_str(line);
    }
    jsonc_segment_with_comma(&out)
}

fn render_jsonc_object<'a>(segments: impl IntoIterator<Item = &'a String>) -> String {
    let mut out = String::from("{\n");
    for segment in segments {
        out.push_str(&jsonc_segment_with_comma(segment));
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

fn render_jsonc_array<'a>(segments: impl IntoIterator<Item = &'a String>) -> String {
    let mut out = String::from("[\n");
    for segment in segments {
        out.push_str(&jsonc_segment_with_comma(segment));
        out.push('\n');
    }
    out.push_str("]\n");
    out
}

fn jsonc_segment_with_comma(segment: &str) -> String {
    let segment = segment.trim_matches(|c| c == '\r' || c == '\n');
    if segment.trim_end().ends_with(',') {
        return segment.to_string();
    }

    let last_line_start = segment.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let last_line = &segment[last_line_start..];
    if let Some(comment_start) = line_comment_start(last_line) {
        let comment_start = last_line_start + comment_start;
        let (before_comment, comment) = segment.split_at(comment_start);
        return format!("{},{}", before_comment.trim_end(), comment);
    }

    format!("{segment},")
}

fn line_comment_start(line: &str) -> Option<usize> {
    let mut chars = line.char_indices().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some((idx, ch)) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
        } else if ch == '/' && matches!(chars.peek(), Some((_, '/'))) {
            return Some(idx);
        }
    }

    None
}

fn default_output_path(dir: &Path, meta: &Meta, output_format: Format) -> Result<PathBuf> {
    let parent = dir.parent().unwrap_or(Path::new("."));
    let mut name = meta
        .source_filename
        .clone()
        .or_else(|| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| Error::Invalid("could not determine output file name".into()))?;
    let stem = match Path::new(&name).file_stem().and_then(|s| s.to_str()) {
        Some(s) => s.to_string(),
        None => name.clone(),
    };
    name = format!("{stem}.{}", output_format.extension());
    Ok(parent.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unwrap_per_key_payload_passes_through_non_toml() {
        let v = json!({"unrelated": 1});
        let out = unwrap_per_key_payload(Format::Json, "key", "k.json", v.clone()).unwrap();
        assert_eq!(out, v);
    }

    #[test]
    fn unwrap_per_key_payload_extracts_wrapper_key_for_toml() {
        let v = json!({"servers": [{"host": "a"}]});
        let out = unwrap_per_key_payload(Format::Toml, "servers", "servers.toml", v).unwrap();
        assert_eq!(out, json!([{"host": "a"}]));
    }

    #[test]
    fn unwrap_per_key_payload_extracts_wrapper_key_for_ini() {
        let v = json!({"settings": {"host": "db.example.com"}});
        let out = unwrap_per_key_payload(Format::Ini, "settings", "settings.ini", v).unwrap();
        assert_eq!(out, json!({"host": "db.example.com"}));
    }

    #[test]
    fn unwrap_per_key_payload_errors_when_wrapper_key_missing() {
        let v = json!({"wrong": 1});
        let err =
            unwrap_per_key_payload(Format::Toml, "right", "x.toml", v).expect_err("should error");
        let msg = err.to_string();
        assert!(
            msg.contains("does not contain expected wrapper key"),
            "got: {msg}"
        );
        assert!(msg.contains("right"), "got: {msg}");
        assert!(msg.contains("x.toml"), "got: {msg}");
    }

    #[test]
    fn unwrap_per_key_payload_errors_when_ini_wrapper_key_missing() {
        let v = json!({"wrong": 1});
        let err =
            unwrap_per_key_payload(Format::Ini, "right", "x.ini", v).expect_err("should error");
        let msg = err.to_string();
        assert!(
            msg.contains("does not contain expected wrapper key"),
            "got: {msg}"
        );
        assert!(msg.contains("right"), "got: {msg}");
        assert!(msg.contains("x.ini"), "got: {msg}");
    }

    #[test]
    fn unwrap_per_key_payload_errors_on_non_object_for_toml() {
        // TOML's grammar guarantees this never occurs through Format::load,
        // but the defensive arm is still exercised here so any future
        // refactor that reaches it returns a clear error rather than
        // panicking.
        let err = unwrap_per_key_payload(Format::Toml, "k", "k.toml", json!([1, 2, 3]))
            .expect_err("should error");
        assert!(
            err.to_string().contains("did not deserialize to a table"),
            "got: {err}"
        );
    }

    #[test]
    fn leading_comment_start_at_zero_returns_zero_without_looping() {
        // Mutating the `start > 0` loop guard to `start >= 0` would hang here
        // because `saturating_sub(1)` keeps `start` pinned at 0.
        assert_eq!(leading_comment_start("any leading text", 0), 0);
        assert_eq!(leading_comment_start("", 0), 0);
    }

    #[test]
    fn leading_comment_start_walks_through_consecutive_line_comments() {
        let text = "// first comment\n// second comment\n  \"a\": 1\n";
        let property_line_start = text.find("  \"a\"").unwrap();
        // All preceding lines are comments, so the function walks all the way
        // back to position 0. A replacement returning `1` would not match.
        assert_eq!(leading_comment_start(text, property_line_start), 0);
    }

    #[test]
    fn line_end_returns_pos_plus_newline_offset() {
        assert_eq!(line_end("abc\ndef", 0), 3);
        assert_eq!(line_end("abc\ndef", 1), 3);
        assert_eq!(line_end("abc\ndef", 2), 3);
        assert_eq!(line_end("no-newline", 0), 10);
    }

    #[test]
    fn render_jsonc_property_normalizes_crlf_line_endings_in_value() {
        // The `trim_matches(|c| c == '\r' || c == '\n')` collapses CRLF wrapping
        // around the value. Mutating `||` to `&&` would leave the wrapping in
        // place because no single character is both \r AND \n.
        let rendered = render_jsonc_property("name", "\r\n\"demo\"\r\n").unwrap();
        assert!(
            !rendered.contains('\r'),
            "expected CR stripped: {rendered:?}"
        );
        assert!(rendered.starts_with("  \"name\": \"demo\""));
        assert!(rendered.ends_with(','));
    }

    #[test]
    fn render_jsonc_array_element_first_line_has_no_leading_newline() {
        // The `if idx > 0 { push('\n') }` guard would push a leading newline
        // for the first line if mutated to `>=`.
        let rendered = render_jsonc_array_element("{\n  \"a\": 1\n}");
        assert!(
            !rendered.starts_with('\n'),
            "first line should not be prefixed with newline: {rendered:?}"
        );
        // Subsequent lines still get newline separators.
        assert!(rendered.contains("\n"));
    }

    #[test]
    fn jsonc_segment_with_comma_strips_surrounding_newlines_before_appending_comma() {
        // Mutating the trim_matches `||` to `&&` would leave the surrounding
        // newlines in place because a char can't be both \r and \n.
        let with_lf = "\n  \"name\": \"demo\"\n";
        let out = jsonc_segment_with_comma(with_lf);
        assert!(!out.starts_with('\n'), "stripped leading LF: {out:?}");
        assert!(out.ends_with(','), "appended trailing comma: {out:?}");

        let with_crlf = "\r\n  \"x\": 1\r\n";
        let out = jsonc_segment_with_comma(with_crlf);
        assert!(!out.starts_with('\r'), "stripped leading CRLF: {out:?}");
        assert!(!out.starts_with('\n'), "stripped leading CRLF: {out:?}");
    }

    #[test]
    fn default_output_path_uses_meta_source_filename_with_output_extension() {
        // The function must return a sibling path of `dir` whose stem matches
        // the original source file and whose extension matches `output_format`.
        // A `Ok(Default::default())` mutant would return an empty PathBuf.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("config-out");
        let meta = Meta {
            source_format: Format::Json,
            file_format: Format::Json,
            source_filename: Some("orig.json".into()),
            root: Root::Object {
                key_order: vec![],
                key_files: std::collections::BTreeMap::new(),
                main_file: None,
            },
        };
        let out = default_output_path(&dir, &meta, Format::Yaml).unwrap();
        let expected = tmp.path().join("orig.yaml");
        assert_eq!(out, expected);
    }

    #[test]
    fn default_output_path_falls_back_to_dir_name_when_source_filename_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("settings");
        let meta = Meta {
            source_format: Format::Json,
            file_format: Format::Json,
            source_filename: None,
            root: Root::Object {
                key_order: vec![],
                key_files: std::collections::BTreeMap::new(),
                main_file: None,
            },
        };
        let out = default_output_path(&dir, &meta, Format::Json).unwrap();
        assert_eq!(out, tmp.path().join("settings.json"));
    }

    #[test]
    fn reassemble_creates_missing_parent_directory_for_output_path() {
        // The `if !parent.as_os_str().is_empty()` guard exists so we don't try
        // to create a parent for a bare-filename path. Deleting the `!` would
        // skip directory creation for normal paths, and the subsequent
        // `fs::write` would fail with "path not found".
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // Disassemble a tiny JSON file so the metadata + part files exist.
        let input = tmp.path().join("orig.json");
        std::fs::write(&input, r#"{"a": 1}"#).unwrap();
        crate::disassemble::disassemble(crate::disassemble::DisassembleOptions {
            input: input.clone(),
            input_format: Some(Format::Json),
            output_dir: Some(src_dir.clone()),
            output_format: Some(Format::Json),
            unique_id: None,
            pre_purge: false,
            post_purge: false,
            ignore_path: None,
        })
        .unwrap();

        // Reassemble into a subdirectory that does not yet exist.
        let nested_target = tmp.path().join("nested").join("output").join("out.json");
        let out = reassemble(ReassembleOptions {
            input_dir: src_dir,
            output: Some(nested_target.clone()),
            output_format: Some(Format::Json),
            post_purge: false,
        })
        .unwrap();
        assert_eq!(out, nested_target);
        assert!(nested_target.exists());
    }

    #[test]
    fn jsonc_segment_with_comma_inserts_before_trailing_line_comment() {
        assert_eq!(
            jsonc_segment_with_comma(r#"  "name": "demo" // keep this comment"#),
            r#"  "name": "demo",// keep this comment"#
        );
    }

    #[test]
    fn jsonc_segment_with_comma_ignores_urls_inside_strings() {
        assert_eq!(
            jsonc_segment_with_comma(r#"  "url": "https://example.com/a""#),
            r#"  "url": "https://example.com/a","#
        );
    }

    #[test]
    fn assemble_jsonc_object_errors_when_main_file_is_not_object() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("_main.jsonc"), "[]\n").unwrap();

        let err = assemble_jsonc_object(tmp.path(), &[], &Default::default(), Some("_main.jsonc"))
            .expect_err("should reject non-object main file");

        assert!(
            err.to_string().contains("did not contain an object"),
            "got: {err}"
        );
    }

    #[test]
    fn assemble_jsonc_object_errors_when_metadata_key_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("_main.jsonc"), "{}\n").unwrap();

        let err = assemble_jsonc_object(
            tmp.path(),
            &["missing".into()],
            &Default::default(),
            Some("_main.jsonc"),
        )
        .expect_err("should reject missing scalar key");

        assert!(
            err.to_string()
                .contains("metadata references key `missing`"),
            "got: {err}"
        );
    }
}
