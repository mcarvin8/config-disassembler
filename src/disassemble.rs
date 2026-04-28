//! Disassemble a JSON, JSON5, YAML, TOON, or TOML document into a directory of
//! smaller files, optionally written in a different format than the input.
//!
//! The `input` may be either a single file or a directory. When it points
//! at a directory, every file under the directory whose extension matches
//! the input format (or, when `input_format` is `None`, any of the four
//! supported formats) is disassembled in place. An optional `ignore_path`
//! can point at a `.gitignore`-style ignore file used to skip paths.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use jsonc_parser::ast;
use jsonc_parser::common::Ranged;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::format::{jsonc_parse_options, ConversionOperation, Format};
use crate::ignore_file::DEFAULT_IGNORE_FILENAME;
use crate::meta::{Meta, Root};

/// File written for object roots that contains the scalar top-level keys.
const MAIN_BASENAME: &str = "_main";

/// Options controlling disassembly.
#[derive(Debug, Clone)]
pub struct DisassembleOptions {
    /// Path to the input. May be either a single config file or a
    /// directory; when it is a directory, every matching file under it
    /// is disassembled in place (see also `ignore_path`).
    pub input: PathBuf,
    /// Format to read the input as. If `None`, the format is inferred
    /// from each file's extension.
    pub input_format: Option<Format>,
    /// Directory to write split files into. Only meaningful when
    /// `input` is a single file; for directory inputs each file's
    /// output goes into a sibling directory named after that file's
    /// stem (mirroring the XML disassembler's behavior).
    pub output_dir: Option<PathBuf>,
    /// Format to write split files in. Defaults to `input_format`.
    pub output_format: Option<Format>,
    /// For array roots, name array-element files using the value of this
    /// field if present on each element (must be a scalar).
    pub unique_id: Option<String>,
    /// If true, remove the contents of the output directory before writing.
    pub pre_purge: bool,
    /// If true, delete the input file (or input directory) after
    /// disassembling. For directory inputs the entire directory is
    /// removed only if every file in it was successfully disassembled.
    pub post_purge: bool,
    /// Optional path to a `.gitignore`-style ignore file that filters
    /// which files are processed when `input` is a directory. Pass
    /// `None` to use [`DEFAULT_IGNORE_FILENAME`] in the input directory
    /// (silently absent if the file does not exist). Ignored entirely
    /// for single-file inputs.
    pub ignore_path: Option<PathBuf>,
}

impl DisassembleOptions {
    /// Build options for a single-file disassembly with sensible
    /// defaults. Directory walks should construct `DisassembleOptions`
    /// directly so they can opt into `ignore_path`.
    pub fn for_file(input: PathBuf) -> Self {
        Self {
            input,
            input_format: None,
            output_dir: None,
            output_format: None,
            unique_id: None,
            pre_purge: false,
            post_purge: false,
            ignore_path: None,
        }
    }
}

/// Disassemble a configuration file (or directory of files) into split
/// files.
///
/// * When `opts.input` is a regular file, returns the directory the files
///   were written to (i.e. the single output directory for that file).
/// * When `opts.input` is a directory, every matching file under it is
///   disassembled in place and the input directory itself is returned.
pub fn disassemble(opts: DisassembleOptions) -> Result<PathBuf> {
    let metadata = fs::metadata(&opts.input)?;
    if metadata.is_dir() {
        return disassemble_directory(opts);
    }
    disassemble_file(opts)
}

/// Disassemble a single file. Equivalent to the previous behavior of
/// [`disassemble`].
fn disassemble_file(opts: DisassembleOptions) -> Result<PathBuf> {
    let input_format = match opts.input_format {
        Some(f) => f,
        None => Format::from_path(&opts.input)?,
    };
    let output_format = opts.output_format.unwrap_or(input_format);
    input_format.ensure_can_convert_to(output_format, ConversionOperation::Convert)?;

    let output_dir = match opts.output_dir.clone() {
        Some(d) => d,
        None => default_output_dir(&opts.input)?,
    };

    if opts.pre_purge && output_dir.exists() {
        fs::remove_dir_all(&output_dir)?;
    }
    fs::create_dir_all(&output_dir)?;

    let source_filename = opts
        .input
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    if input_format == Format::Jsonc && output_format == Format::Jsonc {
        let root =
            write_jsonc_root_preserving(&opts.input, &output_dir, opts.unique_id.as_deref())?;
        let meta = Meta {
            source_format: input_format,
            file_format: output_format,
            source_filename,
            root,
        };
        meta.write(&output_dir)?;

        if opts.post_purge {
            fs::remove_file(&opts.input)?;
        }

        return Ok(output_dir);
    }

    let value = input_format.load(&opts.input)?;

    let root = match &value {
        Value::Object(map) => write_object_root(&output_dir, map, output_format)?,
        Value::Array(items) => {
            write_array_root(&output_dir, items, output_format, opts.unique_id.as_deref())?
        }
        _ => {
            return Err(Error::Invalid(
                "top-level value must be an object or array to disassemble".into(),
            ));
        }
    };

    let meta = Meta {
        source_format: input_format,
        file_format: output_format,
        source_filename,
        root,
    };
    meta.write(&output_dir)?;

    if opts.post_purge {
        fs::remove_file(&opts.input)?;
    }

    Ok(output_dir)
}

/// Disassemble every matching file under a directory. Each file's split
/// output is placed in a sibling directory named after the file's stem,
/// matching how the XML disassembler treats directory inputs.
fn disassemble_directory(opts: DisassembleOptions) -> Result<PathBuf> {
    if opts.output_dir.is_some() {
        return Err(Error::Usage(
            "--output-dir is not supported with a directory input; each file's split output is written next to it".into(),
        ));
    }

    let root = opts.input.clone();
    let ignore = load_ignore_rules(opts.ignore_path.as_deref(), &root)?;

    let mut targets = collect_disassemble_targets(&root, &ignore, opts.input_format)?;
    targets.sort();

    for file in &targets {
        let mut child_opts = opts.clone();
        child_opts.input = file.clone();
        // Each file's output goes into <stem>/ next to the file itself,
        // never into a shared --output-dir (we rejected that above).
        child_opts.output_dir = None;
        // Per-file post_purge would only delete the file; we honor the
        // user's intent by keeping post_purge here so each input file is
        // removed if requested, then we remove the (now empty) input
        // directory at the very end below.
        disassemble_file(child_opts)?;
    }

    if opts.post_purge {
        // Only remove the input directory if it is now empty (every
        // file we looked at was post-purged and no other content
        // remains). Otherwise leave it alone so we don't clobber files
        // the user kept around (output dirs, the ignore file, etc.).
        if directory_is_empty(&root)? {
            fs::remove_dir_all(&root)?;
        }
    }

    Ok(root)
}

/// Walk `root` and collect every file whose extension matches one of the
/// supported formats (or, if `expected_format` is `Some`, only that
/// format), excluding paths matched by `ignore`.
fn collect_disassemble_targets(
    root: &Path,
    ignore: &Option<Gitignore>,
    expected_format: Option<Format>,
) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if is_ignored(ignore, root, &path, ft.is_dir()) {
                continue;
            }
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            // Only look at files whose extension parses as a known
            // format, and (when input_format was set) only the matching
            // format. Anything else is silently skipped — a directory of
            // mixed config files commonly contains README/.git/etc.
            let detected = match Format::from_path(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            if let Some(expected) = expected_format {
                if expected != detected {
                    continue;
                }
            }
            out.push(path);
        }
    }
    Ok(out)
}

fn load_ignore_rules(explicit: Option<&Path>, fallback_dir: &Path) -> Result<Option<Gitignore>> {
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => fallback_dir.join(DEFAULT_IGNORE_FILENAME),
    };
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    let anchor = path.parent().unwrap_or(Path::new("."));
    let mut builder = GitignoreBuilder::new(anchor);
    for line in content.lines() {
        // `add_line` returns a pattern-error on malformed globs; mirror
        // the XML disassembler's tolerant parsing and skip bad lines
        // rather than failing the whole run.
        let _ = builder.add_line(None, line);
    }
    Ok(builder.build().ok())
}

fn is_ignored(ignore: &Option<Gitignore>, root: &Path, path: &Path, is_dir: bool) -> bool {
    let Some(ign) = ignore.as_ref() else {
        return false;
    };
    let candidate = path.strip_prefix(root).unwrap_or(path);
    ign.matched(candidate, is_dir).is_ignore()
}

fn directory_is_empty(dir: &Path) -> Result<bool> {
    let mut entries = fs::read_dir(dir)?;
    Ok(entries.next().is_none())
}

fn default_output_dir(input: &Path) -> Result<PathBuf> {
    let stem = input.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        Error::Invalid(format!(
            "could not derive a directory name from {}",
            input.display()
        ))
    })?;
    let parent = input.parent().unwrap_or(Path::new("."));
    Ok(parent.join(stem))
}

fn write_object_root(dir: &Path, map: &Map<String, Value>, fmt: Format) -> Result<Root> {
    let mut key_order: Vec<String> = Vec::with_capacity(map.len());
    let mut key_files: BTreeMap<String, String> = BTreeMap::new();
    let mut main_object: Map<String, Value> = Map::new();
    let mut used_names: BTreeSet<String> = BTreeSet::new();
    used_names.insert(format!("{MAIN_BASENAME}.{}", fmt.extension()));

    for (key, value) in map {
        key_order.push(key.clone());
        if is_scalar(value) {
            main_object.insert(key.clone(), value.clone());
            continue;
        }

        let filename = unique_filename_for_key(key, fmt, &used_names);
        used_names.insert(filename.clone());
        let path = dir.join(&filename);
        let payload = fmt.wrap_split_payload(key, value);
        fs::write(&path, fmt.serialize(&payload)?)?;
        key_files.insert(key.clone(), filename);
    }

    let main_file = if main_object.is_empty() {
        None
    } else {
        let filename = format!("{MAIN_BASENAME}.{}", fmt.extension());
        let path = dir.join(&filename);
        fs::write(&path, fmt.serialize(&Value::Object(main_object))?)?;
        Some(filename)
    };

    Ok(Root::Object {
        key_order,
        key_files,
        main_file,
    })
}

fn write_array_root(
    dir: &Path,
    items: &[Value],
    fmt: Format,
    unique_id: Option<&str>,
) -> Result<Root> {
    let mut files = Vec::with_capacity(items.len());
    let mut used_names: BTreeSet<String> = BTreeSet::new();
    let width = digit_width(items.len());

    for (idx, item) in items.iter().enumerate() {
        let mut basename = if let Some(field) = unique_id {
            unique_id_basename(item, field)
        } else {
            None
        };
        if basename
            .as_ref()
            .map(|n| used_names.contains(&format!("{n}.{}", fmt.extension())))
            .unwrap_or(false)
        {
            basename = None;
        }
        let basename = basename.unwrap_or_else(|| format!("{:0width$}", idx + 1, width = width));

        let mut filename = format!("{basename}.{}", fmt.extension());
        if used_names.contains(&filename) {
            filename = format!("{basename}-{}.{}", hash_value(item, 8), fmt.extension());
        }
        used_names.insert(filename.clone());

        let path = dir.join(&filename);
        fs::write(&path, fmt.serialize(item)?)?;
        files.push(filename);
    }

    Ok(Root::Array { files })
}

fn write_jsonc_root_preserving(input: &Path, dir: &Path, unique_id: Option<&str>) -> Result<Root> {
    let text = fs::read_to_string(input)?;
    let ast = parse_jsonc_ast(&text)?;
    let value = Format::Jsonc.parse(&text)?;

    match (ast, value) {
        (ast::Value::Object(object), Value::Object(_)) => {
            write_jsonc_object_root(dir, &text, object)
        }
        (ast::Value::Array(array), Value::Array(items)) => {
            write_jsonc_array_root(dir, &text, array, &items, unique_id)
        }
        _ => Err(Error::Invalid(
            "top-level value must be an object or array to disassemble".into(),
        )),
    }
}

fn write_jsonc_object_root(dir: &Path, text: &str, object: ast::Object<'_>) -> Result<Root> {
    let properties = jsonc_object_properties(text, object)?;
    let mut key_order = Vec::with_capacity(properties.len());
    let mut key_files: BTreeMap<String, String> = BTreeMap::new();
    let mut main_segments = Vec::new();
    let mut used_names: BTreeSet<String> = BTreeSet::new();
    used_names.insert(format!("{MAIN_BASENAME}.{}", Format::Jsonc.extension()));

    for property in properties {
        key_order.push(property.key.clone());
        if property.is_scalar {
            main_segments.push(property.segment);
            continue;
        }

        let filename = unique_filename_for_key(&property.key, Format::Jsonc, &used_names);
        used_names.insert(filename.clone());
        fs::write(
            dir.join(&filename),
            ensure_trailing_newline(&property.value_text),
        )?;
        key_files.insert(property.key, filename);
    }

    let main_file = if main_segments.is_empty() {
        None
    } else {
        let filename = format!("{MAIN_BASENAME}.{}", Format::Jsonc.extension());
        fs::write(
            dir.join(&filename),
            render_jsonc_object(main_segments.iter()),
        )?;
        Some(filename)
    };

    Ok(Root::Object {
        key_order,
        key_files,
        main_file,
    })
}

fn write_jsonc_array_root(
    dir: &Path,
    text: &str,
    array: ast::Array<'_>,
    items: &[Value],
    unique_id: Option<&str>,
) -> Result<Root> {
    if array.elements.len() != items.len() {
        return Err(Error::Invalid(
            "JSONC AST and value model disagree on array length".into(),
        ));
    }

    let mut files = Vec::with_capacity(array.elements.len());
    let mut used_names: BTreeSet<String> = BTreeSet::new();
    let width = digit_width(array.elements.len());

    for (idx, (element, item)) in array.elements.iter().zip(items).enumerate() {
        let mut basename = unique_id.and_then(|field| unique_id_basename(item, field));
        if basename
            .as_ref()
            .map(|n| used_names.contains(&format!("{n}.{}", Format::Jsonc.extension())))
            .unwrap_or(false)
        {
            basename = None;
        }
        let basename = basename.unwrap_or_else(|| format!("{:0width$}", idx + 1, width = width));

        let mut filename = format!("{basename}.{}", Format::Jsonc.extension());
        if used_names.contains(&filename) {
            filename = format!(
                "{basename}-{}.{}",
                hash_value(item, 8),
                Format::Jsonc.extension()
            );
        }
        used_names.insert(filename.clone());

        let value_text = element.text(text).trim();
        fs::write(dir.join(&filename), ensure_trailing_newline(value_text))?;
        files.push(filename);
    }

    Ok(Root::Array { files })
}

struct JsoncPropertySyntax {
    key: String,
    is_scalar: bool,
    segment: String,
    value_text: String,
}

fn jsonc_object_properties(
    text: &str,
    object: ast::Object<'_>,
) -> Result<Vec<JsoncPropertySyntax>> {
    let mut properties = Vec::with_capacity(object.properties.len());
    for property in object.properties {
        let key = property.name.clone().into_string();
        let property_range = property.range();
        let value_range = property.value.range();
        properties.push(JsoncPropertySyntax {
            key,
            is_scalar: is_jsonc_ast_scalar(&property.value),
            segment: jsonc_property_segment(text, property_range.start, value_range.end)
                .to_string(),
            value_text: property.value.text(text).trim().to_string(),
        });
    }
    Ok(properties)
}

fn parse_jsonc_ast(text: &str) -> Result<ast::Value<'_>> {
    jsonc_parser::parse_to_ast(text, &Default::default(), &jsonc_parse_options())
        .map_err(|e| Error::Invalid(format!("jsonc parse error: {e}")))?
        .value
        .ok_or_else(|| Error::Invalid("JSONC document did not contain a value".into()))
}

fn is_jsonc_ast_scalar(value: &ast::Value<'_>) -> bool {
    !matches!(value, ast::Value::Object(_) | ast::Value::Array(_))
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

fn render_jsonc_object<'a>(segments: impl IntoIterator<Item = &'a String>) -> String {
    let mut out = String::from("{\n");
    for segment in segments {
        out.push_str(&jsonc_segment_with_comma(segment));
        out.push('\n');
    }
    out.push_str("}\n");
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

fn ensure_trailing_newline(text: &str) -> String {
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Object(_) | Value::Array(_))
}

fn digit_width(count: usize) -> usize {
    let mut w = 1;
    let mut n = count;
    while n >= 10 {
        n /= 10;
        w += 1;
    }
    w.max(4)
}

fn unique_filename_for_key(key: &str, fmt: Format, used: &BTreeSet<String>) -> String {
    let sanitized = sanitize(key);
    let base = if sanitized.is_empty() {
        hash_string(key, 12)
    } else {
        sanitized
    };
    let mut filename = format!("{base}.{}", fmt.extension());
    if used.contains(&filename) {
        filename = format!("{base}-{}.{}", hash_string(key, 8), fmt.extension());
    }
    filename
}

fn unique_id_basename(item: &Value, field: &str) -> Option<String> {
    let map = item.as_object()?;
    let raw = match map.get(field)? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return None,
    };
    let s = sanitize(&raw);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn sanitize(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string()
}

fn hash_string(input: &str, len: usize) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex.chars().take(len).collect()
}

fn hash_value(value: &Value, len: usize) -> String {
    let canonical = serde_json::to_string(value).unwrap_or_default();
    hash_string(&canonical, len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jsonc_segment_with_comma_inserts_before_trailing_line_comment() {
        assert_eq!(
            jsonc_segment_with_comma(r#"  "name": "demo" // keep this comment"#),
            r#"  "name": "demo",// keep this comment"#
        );
    }

    #[test]
    fn jsonc_segment_with_comma_ignores_comment_markers_inside_strings() {
        assert_eq!(
            jsonc_segment_with_comma(r#"  "url": "https://example.com/a""#),
            r#"  "url": "https://example.com/a","#
        );
    }

    #[test]
    fn jsonc_segment_with_comma_leaves_existing_comma_alone() {
        assert_eq!(
            jsonc_segment_with_comma("  \"enabled\": true,"),
            "  \"enabled\": true,"
        );
    }

    #[test]
    fn line_comment_start_respects_escaped_quotes() {
        let line = r#"  "text": "escaped \" quote // still string" // comment"#;
        assert_eq!(
            line_comment_start(line),
            Some(line.find(" // comment").unwrap() + 1)
        );
    }

    #[test]
    fn ensure_trailing_newline_does_not_duplicate_newline() {
        assert_eq!(ensure_trailing_newline("value\n"), "value\n");
        assert_eq!(ensure_trailing_newline("value"), "value\n");
    }

    #[test]
    fn jsonc_same_format_post_purge_removes_input_file() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("config.jsonc");
        fs::write(
            &input,
            r#"{
  "name": "demo",
  "settings": {
    "retry": 3,
  },
}"#,
        )
        .unwrap();

        let output_dir = tmp.path().join("split");
        let dir = disassemble(DisassembleOptions {
            input: input.clone(),
            input_format: Some(Format::Jsonc),
            output_dir: Some(output_dir),
            output_format: Some(Format::Jsonc),
            unique_id: None,
            pre_purge: false,
            post_purge: true,
            ignore_path: None,
        })
        .unwrap();

        assert!(!input.exists());
        assert!(dir.join("settings.jsonc").exists());
        assert!(dir.join(MAIN_BASENAME).with_extension("jsonc").exists());
    }

    #[test]
    fn write_jsonc_object_root_writes_nested_and_main_files() {
        let text = r#"{
  "name": "demo",
  "settings": {
    "retry": 3,
  },
}"#;
        let ast::Value::Object(object) = parse_jsonc_ast(text).unwrap() else {
            panic!("expected object AST");
        };
        let tmp = tempfile::tempdir().unwrap();

        let root = write_jsonc_object_root(tmp.path(), text, object).unwrap();

        let Root::Object {
            key_order,
            key_files,
            main_file,
        } = root
        else {
            panic!("expected object root");
        };
        assert_eq!(key_order, vec!["name", "settings"]);
        assert_eq!(key_files.get("settings").unwrap(), "settings.jsonc");
        assert_eq!(main_file.as_deref(), Some("_main.jsonc"));
        assert!(fs::read_to_string(tmp.path().join("settings.jsonc"))
            .unwrap()
            .contains(r#""retry": 3"#));
        assert!(fs::read_to_string(tmp.path().join("_main.jsonc"))
            .unwrap()
            .contains(r#""name": "demo","#));
    }

    #[test]
    fn write_jsonc_array_root_rejects_ast_value_length_mismatch() {
        let text = "[1, 2]";
        let ast::Value::Array(array) = parse_jsonc_ast(text).unwrap() else {
            panic!("expected array AST");
        };
        let tmp = tempfile::tempdir().unwrap();

        let err = write_jsonc_array_root(tmp.path(), text, array, &[json!(1)], None)
            .expect_err("should reject mismatched inputs");

        assert!(
            err.to_string()
                .contains("JSONC AST and value model disagree on array length"),
            "got: {err}"
        );
    }

    #[test]
    fn write_jsonc_array_root_hashes_when_unique_id_collides_with_index_name() {
        let text = r#"[
  {
    "name": "0002",
    "value": 1,
  },
  {
    "value": 2,
  },
]"#;
        let ast::Value::Array(array) = parse_jsonc_ast(text).unwrap() else {
            panic!("expected array AST");
        };
        let items = Format::Jsonc
            .parse(text)
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        let tmp = tempfile::tempdir().unwrap();

        let root = write_jsonc_array_root(tmp.path(), text, array, &items, Some("name")).unwrap();

        let Root::Array { files } = root else {
            panic!("expected array root");
        };
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "0002.jsonc");
        assert!(files[1].starts_with("0002-"), "files: {files:?}");
        assert!(tmp.path().join(&files[1]).exists());
    }
}
