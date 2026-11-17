//! config-disassembler
//!
//! Disassemble XML files into smaller files and reassemble the original on
//! demand. The implementation lives in the in-tree [`xml`] module (ported
//! from the upstream `xml-disassembler` crate).
//!
//! Every disassemble action accepts a directory as input; when given a
//! directory the runner walks it (filtering with the optional
//! `--ignore-path`, defaulting to [`ignore_file::DEFAULT_IGNORE_FILENAME`])
//! and disassembles every matching file in place.

pub mod cli;
pub mod error;
pub mod ignore_file;
pub mod xml;
pub mod xml_cmd;

pub use error::{Error, Result};

/// Entry point used by the binary. `args[0]` is the program name.
pub async fn run(args: Vec<String>) -> Result<()> {
    cli::dispatch(args).await
}
