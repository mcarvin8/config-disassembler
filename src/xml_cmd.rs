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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_forwards_args_and_succeeds_for_valid_disassemble() {
        // Round-trip through the public entry point so a regression that
        // short-circuits this function to `Ok(())` without actually invoking
        // the in-tree XML CLI would be caught: the assertion below depends on
        // files written by the underlying disassembler.
        let tmp = tempfile::tempdir().unwrap();
        let xml_path = tmp.path().join("sample.xml");
        std::fs::write(
            &xml_path,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Root xmlns="http://example.com">
  <child><name>one</name></child>
  <child><name>two</name></child>
</Root>"#,
        )
        .unwrap();
        run(vec![
            "disassemble".to_string(),
            xml_path.to_string_lossy().to_string(),
        ])
        .await
        .unwrap();
        assert!(tmp.path().join("sample").exists());
    }

    #[tokio::test]
    async fn run_propagates_unknown_subcommand_as_ok() {
        // The underlying CLI treats unknown subcommands as a non-error
        // (prints a message and returns Ok). Asserting it explicitly here
        // pins down the no-op shim behavior.
        run(vec!["definitely-not-a-real-subcommand".to_string()])
            .await
            .unwrap();
    }
}
