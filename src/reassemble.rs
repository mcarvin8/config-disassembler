//! Reassemble a directory of split files (produced by [`disassemble`])
//! back into a single configuration file.
//!
//! [`disassemble`]: crate::disassemble::disassemble

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::format::Format;
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
    let file_format: Format = meta.file_format.into();
    let output_format: Format = opts
        .output_format
        .unwrap_or_else(|| meta.source_format.into());

    let value = match &meta.root {
        Root::Object {
            key_order,
            key_files,
            main_file,
        } => assemble_object(dir, key_order, key_files, main_file.as_deref(), file_format)?,
        Root::Array { files } => assemble_array(dir, files, file_format)?,
    };

    let output_path = match opts.output.clone() {
        Some(p) => p,
        None => default_output_path(dir, &meta, output_format)?,
    };
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&output_path, output_format.serialize(&value)?)?;

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
            let value = file_format.load(&dir.join(filename))?;
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

fn assemble_array(dir: &Path, files: &[String], file_format: Format) -> Result<Value> {
    let mut items = Vec::with_capacity(files.len());
    for name in files {
        items.push(file_format.load(&dir.join(name))?);
    }
    Ok(Value::Array(items))
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
