//! config-disassembler
//!
//! Disassemble configuration files (XML, JSON, JSON5, YAML) into smaller
//! files and reassemble the original on demand. The XML implementation is
//! delegated to the [`xml_disassembler`] crate; JSON, JSON5, and YAML share
//! a common value model so a file in one format can be split into files of
//! another format and reassembled back into any supported format.

pub mod cli;
pub mod disassemble;
pub mod error;
pub mod format;
pub mod meta;
pub mod reassemble;
pub mod xml_cmd;

pub use error::{Error, Result};

/// Entry point used by the binary. `args[0]` is the program name.
pub async fn run(args: Vec<String>) -> Result<()> {
    cli::dispatch(args).await
}
