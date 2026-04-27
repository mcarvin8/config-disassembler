//! Disassemble a JSON, JSON5, or YAML document into a directory of smaller
//! files, optionally written in a different format than the input.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::format::Format;
use crate::meta::{Meta, Root};

/// File written for object roots that contains the scalar top-level keys.
const MAIN_BASENAME: &str = "_main";

/// Options controlling disassembly.
#[derive(Debug, Clone)]
pub struct DisassembleOptions {
    /// Path to the input config file.
    pub input: PathBuf,
    /// Format to read the input as. If `None`, inferred from the extension.
    pub input_format: Option<Format>,
    /// Directory to write split files into. If `None`, defaults to a
    /// directory next to the input file named after the input's file stem.
    pub output_dir: Option<PathBuf>,
    /// Format to write split files in. Defaults to `input_format`.
    pub output_format: Option<Format>,
    /// For array roots, name array-element files using the value of this
    /// field if present on each element (must be a scalar).
    pub unique_id: Option<String>,
    /// If true, remove the contents of the output directory before writing.
    pub pre_purge: bool,
    /// If true, delete the input file after disassembling.
    pub post_purge: bool,
}

/// Disassemble a configuration file into a directory of split files.
///
/// Returns the directory the files were written to.
pub fn disassemble(opts: DisassembleOptions) -> Result<PathBuf> {
    let input_format = match opts.input_format {
        Some(f) => f,
        None => Format::from_path(&opts.input)?,
    };
    let output_format = opts.output_format.unwrap_or(input_format);
    enforce_toml_isolation(input_format, output_format)?;

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
        source_format: input_format.into(),
        file_format: output_format.into(),
        source_filename,
        root,
    };
    meta.write(&output_dir)?;

    if opts.post_purge {
        fs::remove_file(&opts.input)?;
    }

    Ok(output_dir)
}

/// Enforce TOML's isolation rule: TOML can only be converted to and
/// from TOML. Mixing TOML with another format would lose information
/// (TOML cannot represent `null` or array roots) or reorder values
/// (TOML's bare-keys-before-tables rule), so refuse the operation up
/// front with a clear error.
fn enforce_toml_isolation(input: Format, output: Format) -> Result<()> {
    if (input == Format::Toml) != (output == Format::Toml) {
        return Err(Error::Invalid(format!(
            "TOML can only be converted to and from TOML; got input={input}, output={output}"
        )));
    }
    Ok(())
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
        let payload = wrap_per_key_payload(fmt, key, value);
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

/// For TOML output, wrap each per-key payload under its parent key
/// before serialization. TOML documents must have a table (object)
/// root, so writing a bare array (e.g. an array-of-tables under a
/// key like `servers`) would fail. Wrapping produces an idiomatic
/// TOML file (e.g. `[[servers]]` headers in `servers.toml`) that
/// reassembly can unwrap deterministically using the metadata.
///
/// For the other formats the payload is the value itself; cross-format
/// round-tripping continues to work unchanged.
fn wrap_per_key_payload(fmt: Format, key: &str, value: &Value) -> Value {
    if fmt == Format::Toml {
        let mut wrapper = Map::new();
        wrapper.insert(key.to_string(), value.clone());
        Value::Object(wrapper)
    } else {
        value.clone()
    }
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
