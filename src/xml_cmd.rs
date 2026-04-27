//! `xml` subcommand: thin wrapper around the in-tree XML disassembler CLI.
//!
//! All arguments after `config-disassembler xml` are forwarded directly to
//! [`crate::xml::cli::run`], so every option from the original
//! `xml-disassembler` crate works unchanged here. This subcommand
//! intentionally does not duplicate any of the XML logic — it simply
//! delegates to the in-tree port.

use crate::error::{Error, Result};

/// Run the in-tree XML disassembler CLI with the provided arguments.
///
/// `args` should be the trailing arguments after `config-disassembler xml`,
/// e.g. `["disassemble", "path/to/file.xml", "--format", "json"]`.
pub async fn run(args: Vec<String>) -> Result<()> {
    let mut forwarded = Vec::with_capacity(args.len() + 1);
    forwarded.push("xml-disassembler".to_string());
    forwarded.extend(args);

    crate::xml::cli::run(forwarded)
        .await
        .map_err(|e| Error::Xml(e.to_string()))
}
