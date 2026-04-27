//! Metadata sidecar describing a disassembled directory.
//!
//! A `.config-disassembler.json` file is written into the disassembly output
//! directory so reassembly can reconstruct the original key order, root type,
//! and the format the split files were written in.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::format::Format;

/// File name of the metadata sidecar.
pub const META_FILENAME: &str = ".config-disassembler.json";

/// Description of how a disassembled directory was produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    /// Format the original input file was read from.
    pub source_format: SerdeFormat,
    /// Format used to write the split files on disk.
    pub file_format: SerdeFormat,
    /// Original input file name (with extension), used as a default when
    /// reassembling without an explicit output path.
    pub source_filename: Option<String>,
    /// Whether the document root was an object or an array.
    pub root: Root,
}

/// Description of the document root.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Root {
    /// The root was a JSON object.
    Object {
        /// Original key ordering at the root.
        key_order: Vec<String>,
        /// Map from key to the file containing that key's value, for keys
        /// whose value is a non-scalar (object or array). Scalars are
        /// inlined into [`main_file`].
        ///
        /// [`main_file`]: Root::Object::main_file
        key_files: std::collections::BTreeMap<String, String>,
        /// File name (relative to the meta dir) containing all scalar
        /// top-level keys, or `None` if there were none.
        main_file: Option<String>,
    },
    /// The root was a JSON array.
    Array {
        /// File names (relative to the meta dir) for each array element,
        /// in original order.
        files: Vec<String>,
    },
}

/// Serde-friendly mirror of [`Format`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerdeFormat {
    Json,
    Json5,
    Yaml,
}

impl From<Format> for SerdeFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Json => SerdeFormat::Json,
            Format::Json5 => SerdeFormat::Json5,
            Format::Yaml => SerdeFormat::Yaml,
        }
    }
}

impl From<SerdeFormat> for Format {
    fn from(f: SerdeFormat) -> Self {
        match f {
            SerdeFormat::Json => Format::Json,
            SerdeFormat::Json5 => Format::Json5,
            SerdeFormat::Yaml => Format::Yaml,
        }
    }
}

impl Meta {
    /// Write the metadata file into `dir`.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = dir.join(META_FILENAME);
        let text = serde_json::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }

    /// Read the metadata file from `dir`.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(META_FILENAME);
        let text = fs::read_to_string(&path).map_err(|e| {
            crate::error::Error::Invalid(format!(
                "could not read metadata file {}: {e}",
                path.display()
            ))
        })?;
        Ok(serde_json::from_str(&text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_format_round_trip() {
        for fmt in [Format::Json, Format::Json5, Format::Yaml] {
            let s: SerdeFormat = fmt.into();
            let back: Format = s.into();
            assert_eq!(fmt, back);
        }
    }

    #[test]
    fn read_returns_invalid_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let err = Meta::read(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("metadata"));
    }

    #[test]
    fn write_and_read_round_trip_object_root() {
        let tmp = tempfile::tempdir().unwrap();
        let meta = Meta {
            source_format: SerdeFormat::Json,
            file_format: SerdeFormat::Yaml,
            source_filename: Some("orig.json".into()),
            root: Root::Object {
                key_order: vec!["a".into(), "b".into()],
                key_files: std::collections::BTreeMap::new(),
                main_file: Some("_main.yaml".into()),
            },
        };
        meta.write(tmp.path()).unwrap();
        let back = Meta::read(tmp.path()).unwrap();
        assert!(matches!(back.root, Root::Object { .. }));
    }

    #[test]
    fn write_and_read_round_trip_array_root() {
        let tmp = tempfile::tempdir().unwrap();
        let meta = Meta {
            source_format: SerdeFormat::Yaml,
            file_format: SerdeFormat::Json5,
            source_filename: None,
            root: Root::Array {
                files: vec!["1.json5".into(), "2.json5".into()],
            },
        };
        meta.write(tmp.path()).unwrap();
        let back = Meta::read(tmp.path()).unwrap();
        match back.root {
            Root::Array { files } => assert_eq!(files.len(), 2),
            _ => panic!("expected array root"),
        }
    }
}
