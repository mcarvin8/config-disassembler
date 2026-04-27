//! config-disassembler
//!
//! Disassemble configuration files (XML, JSON, JSON5, YAML, TOML) into
//! smaller files and reassemble the original on demand. The XML
//! implementation lives in the in-tree [`xml`] module (ported from the
//! upstream `xml-disassembler` crate); JSON, JSON5, and YAML share a
//! common value model so a file in one format can be split into files
//! of another format and reassembled back into any of those three.
//! TOML is intentionally isolated and can only be converted to and
//! from TOML, because TOML cannot represent `null`, forbids array
//! roots, and forces bare keys to precede tables (which would reorder
//! values on round-trip through other formats).
//!
//! Every disassemble action accepts a directory as input; when given a
//! directory the runner walks it (filtering with the optional
//! `--ignore-path`, defaulting to [`ignore_file::DEFAULT_IGNORE_FILENAME`])
//! and disassembles every matching file in place.

pub mod cli;
pub mod disassemble;
pub mod error;
pub mod format;
pub mod ignore_file;
pub mod meta;
pub mod reassemble;
pub mod xml;
pub mod xml_cmd;

pub use error::{Error, Result};

/// Entry point used by the binary. `args[0]` is the program name.
pub async fn run(args: Vec<String>) -> Result<()> {
    cli::dispatch(args).await
}
