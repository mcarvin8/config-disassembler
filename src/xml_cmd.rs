//! `xml` subcommand: thin wrapper around the [`xml_disassembler`] CLI.
//!
//! All arguments after `config-disassembler xml` are forwarded directly to
//! [`xml_disassembler::cli::run`], so all of that crate's options work
//! unchanged here. This subcommand intentionally does not duplicate any of
//! the XML logic — it simply delegates.

use crate::error::{Error, Result};

/// Run the embedded `xml-disassembler` CLI with the provided arguments.
///
/// `args` should be the trailing arguments after `config-disassembler xml`,
/// e.g. `["disassemble", "path/to/file.xml", "--format", "json"]`.
pub async fn run(args: Vec<String>) -> Result<()> {
    let mut forwarded = Vec::with_capacity(args.len() + 1);
    forwarded.push("xml-disassembler".to_string());
    forwarded.extend(args);

    xml_disassembler::cli::run(forwarded)
        .await
        .map_err(|e| Error::Xml(e.to_string()))
}
