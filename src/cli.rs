//! Top-level command-line dispatcher.
//!
//! ```text
//! config-disassembler <subcommand> [args...]
//!
//! Subcommands:
//!   xml      Forward to the bundled xml-disassembler CLI.
//!   json     Disassemble or reassemble a JSON file.
//!   json5    Disassemble or reassemble a JSON5 file.
//!   yaml     Disassemble or reassemble a YAML file.
//!   help     Show this help text.
//! ```

use std::path::PathBuf;

use crate::disassemble::{self, DisassembleOptions};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::reassemble::{self, ReassembleOptions};
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
        "json" => run_format(Format::Json, rest),
        "json5" => run_format(Format::Json5, rest),
        "yaml" | "yml" => run_format(Format::Yaml, rest),
        other => Err(Error::Usage(format!(
            "unknown subcommand `{other}`. run `config-disassembler help` for usage."
        ))),
    }
}

fn run_format(default_format: Format, args: Vec<String>) -> Result<()> {
    let mut iter = args.into_iter();
    let action = iter.next().ok_or_else(|| {
        Error::Usage(format!(
            "{default_format} subcommand requires `disassemble` or `reassemble`"
        ))
    })?;
    let rest: Vec<String> = iter.collect();

    match action.as_str() {
        "disassemble" => run_disassemble(default_format, rest),
        "reassemble" => run_reassemble(default_format, rest),
        "help" | "-h" | "--help" => {
            print_format_help(default_format);
            Ok(())
        }
        other => Err(Error::Usage(format!(
            "unknown action `{other}` for `{default_format}`; expected `disassemble` or `reassemble`"
        ))),
    }
}

fn run_disassemble(default_format: Format, args: Vec<String>) -> Result<()> {
    let mut input: Option<PathBuf> = None;
    let mut output_dir: Option<PathBuf> = None;
    let mut output_format: Option<Format> = None;
    let mut input_format: Option<Format> = None;
    let mut unique_id: Option<String> = None;
    let mut pre_purge = false;
    let mut post_purge = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--output-dir" | "-o" => {
                output_dir = Some(PathBuf::from(expect_value(&mut iter, "--output-dir")?));
            }
            "--output-format" => {
                output_format = Some(expect_value(&mut iter, "--output-format")?.parse()?);
            }
            "--input-format" => {
                input_format = Some(expect_value(&mut iter, "--input-format")?.parse()?);
            }
            "--unique-id" => {
                unique_id = Some(expect_value(&mut iter, "--unique-id")?);
            }
            "--pre-purge" => pre_purge = true,
            "--post-purge" => post_purge = true,
            "-h" | "--help" => {
                print_format_help(default_format);
                return Ok(());
            }
            other if other.starts_with('-') => {
                return Err(Error::Usage(format!("unknown option `{other}`")));
            }
            _ if input.is_none() => input = Some(PathBuf::from(arg)),
            _ => {
                return Err(Error::Usage(format!("unexpected argument `{arg}`")));
            }
        }
    }

    let input = input.ok_or_else(|| Error::Usage("missing <input> file path".into()))?;
    let opts = DisassembleOptions {
        input,
        input_format: input_format.or(Some(default_format)),
        output_dir,
        output_format,
        unique_id,
        pre_purge,
        post_purge,
    };
    let dir = disassemble::disassemble(opts)?;
    println!("disassembled into {}", dir.display());
    Ok(())
}

fn run_reassemble(default_format: Format, args: Vec<String>) -> Result<()> {
    let mut input_dir: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut output_format: Option<Format> = None;
    let mut post_purge = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                output = Some(PathBuf::from(expect_value(&mut iter, "--output")?));
            }
            "--output-format" => {
                output_format = Some(expect_value(&mut iter, "--output-format")?.parse()?);
            }
            "--post-purge" => post_purge = true,
            "-h" | "--help" => {
                print_format_help(default_format);
                return Ok(());
            }
            other if other.starts_with('-') => {
                return Err(Error::Usage(format!("unknown option `{other}`")));
            }
            _ if input_dir.is_none() => input_dir = Some(PathBuf::from(arg)),
            _ => {
                return Err(Error::Usage(format!("unexpected argument `{arg}`")));
            }
        }
    }

    let input_dir = input_dir.ok_or_else(|| Error::Usage("missing <input-dir> path".into()))?;
    let opts = ReassembleOptions {
        input_dir,
        output,
        output_format: output_format.or(Some(default_format)),
        post_purge,
    };
    let path = reassemble::reassemble(opts)?;
    println!("reassembled to {}", path.display());
    Ok(())
}

fn expect_value<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> Result<String> {
    iter.next()
        .ok_or_else(|| Error::Usage(format!("`{flag}` expects a value")))
}

fn print_help() {
    eprintln!(
        "config-disassembler {ver}\n\
\n\
Disassemble configuration files (XML, JSON, JSON5, YAML) into smaller files\n\
and reassemble the original on demand.\n\
\n\
USAGE:\n\
    config-disassembler <subcommand> [args...]\n\
\n\
SUBCOMMANDS:\n\
    xml      Forward to the bundled xml-disassembler CLI.\n\
    json     Disassemble or reassemble a JSON file.\n\
    json5    Disassemble or reassemble a JSON5 file.\n\
    yaml     Disassemble or reassemble a YAML file.\n\
    help     Show this help text.\n\
\n\
Run `config-disassembler <subcommand> --help` for subcommand details.\n",
        ver = env!("CARGO_PKG_VERSION")
    );
}

fn print_format_help(format: Format) {
    eprintln!(
        "config-disassembler {format} <action> [options]\n\
\n\
ACTIONS:\n\
    disassemble <input>   Split <input> into a directory of smaller files.\n\
    reassemble  <dir>     Rebuild the original file from <dir>.\n\
\n\
DISASSEMBLE OPTIONS:\n\
    -o, --output-dir <dir>      Output directory (default: <input-stem> next to input).\n\
    --input-format <fmt>        Override the input format (default: inferred from extension or `{format}`).\n\
    --output-format <fmt>       Format used for the split files (default: same as input).\n\
    --unique-id <field>         For array roots, name files by this field on each element.\n\
    --pre-purge                 Remove the output directory before writing.\n\
    --post-purge                Delete the input file after disassembly.\n\
\n\
REASSEMBLE OPTIONS:\n\
    -o, --output <file>         Output file (default: derived from metadata next to input dir).\n\
    --output-format <fmt>       Format to write the reassembled file in (default: original source format).\n\
    --post-purge                Remove the input directory after reassembly.\n\
\n\
<fmt> is one of: json, json5, yaml.\n"
    );
}
