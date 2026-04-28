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
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::format::{ConversionOperation, Format};
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

    let value = input_format.load(&opts.input)?;
    let source_filename = opts
        .input
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

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
