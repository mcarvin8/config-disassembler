//! Top-level command-line dispatcher.
//!
//! ```text
//! config-disassembler <subcommand> [args...]
//!
//! Subcommands:
//!   xml      Disassemble or reassemble an XML file.
//!   help     Show this help text.
//! ```

use crate::error::{Error, Result};
use crate::xml_cmd;

/// Dispatch a full argv (including program name at `args[0]`).
pub async fn dispatch(args: Vec<String>) -> Result<()> {
    let mut iter = args.into_iter();
    let _program = iter.next();
    let subcommand = match iter.next() {
        Some(s) => s,
        None => {
            print_help();
            return Ok(());
        }
    };
    let rest: Vec<String> = iter.collect();

    match subcommand.as_str() {
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        "xml" => xml_cmd::run(rest).await,
        other => Err(Error::Usage(format!(
            "unknown subcommand `{other}`. run `config-disassembler help` for usage."
        ))),
    }
}

fn print_help() {
    eprintln!(
        "config-disassembler {ver}\n\
\n\
Disassemble XML files into smaller files and reassemble the original on demand.\n\
\n\
USAGE:\n\
    config-disassembler <subcommand> [args...]\n\
\n\
SUBCOMMANDS:\n\
    xml      Disassemble or reassemble an XML file.\n\
    help     Show this help text.\n\
\n\
Run `config-disassembler xml --help` for subcommand details.\n",
        ver = env!("CARGO_PKG_VERSION")
    );
}
