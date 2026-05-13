//! Top-level command-line dispatcher.
//!
//! ```text
//! config-disassembler <subcommand> [args...]
//!
//! Subcommands:
//!   xml      Disassemble or reassemble an XML file (in-tree port of xml-disassembler).
//!   json     Disassemble or reassemble a JSON file.
//!   json5    Disassemble or reassemble a JSON5 file.
//!   jsonc    Disassemble or reassemble a JSONC file.
//!   yaml     Disassemble or reassemble a YAML file.
//!   toon     Disassemble or reassemble a TOON file.
//!   toml     Disassemble or reassemble a TOML file (TOML <-> TOML only).
//!   ini      Disassemble or reassemble an INI file (INI <-> INI only).
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
        "jsonc" => run_format(Format::Jsonc, rest),
        "yaml" | "yml" => run_format(Format::Yaml, rest),
        "toon" => run_format(Format::Toon, rest),
        "toml" => run_format(Format::Toml, rest),
        "ini" => run_format(Format::Ini, rest),
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
    let allows_format_overrides = default_format.allows_format_overrides();
    let mut input: Option<PathBuf> = None;
    let mut output_dir: Option<PathBuf> = None;
    let mut output_format: Option<Format> = None;
    let mut input_format: Option<Format> = None;
    let mut unique_id: Option<String> = None;
    let mut pre_purge = false;
    let mut post_purge = false;
    let mut ignore_path: Option<PathBuf> = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--output-dir" | "-o" => {
                output_dir = Some(PathBuf::from(expect_value(&mut iter, "--output-dir")?));
            }
            "--output-format" if !allows_format_overrides => {
                return Err(Error::Usage(format!(
                    "--output-format is not supported for `{default_format}`; {} can only be converted to {}",
                    default_format.display_name(),
                    default_format.display_name()
                )));
            }
            "--output-format" => {
                output_format = Some(expect_value(&mut iter, "--output-format")?.parse()?);
            }
            "--input-format" if !allows_format_overrides => {
                return Err(Error::Usage(format!(
                    "--input-format is not supported for `{default_format}`; {} can only be converted from {}",
                    default_format.display_name(),
                    default_format.display_name()
                )));
            }
            "--input-format" => {
                input_format = Some(expect_value(&mut iter, "--input-format")?.parse()?);
            }
            "--unique-id" => {
                unique_id = Some(expect_value(&mut iter, "--unique-id")?);
            }
            "--ignore-path" => {
                ignore_path = Some(PathBuf::from(expect_value(&mut iter, "--ignore-path")?));
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
        ignore_path,
    };
    let dir = disassemble::disassemble(opts)?;
    println!("disassembled into {}", dir.display());
    Ok(())
}

fn run_reassemble(default_format: Format, args: Vec<String>) -> Result<()> {
    let allows_format_overrides = default_format.allows_format_overrides();
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
            "--output-format" if !allows_format_overrides => {
                return Err(Error::Usage(format!(
                    "--output-format is not supported for `{default_format}`; {} can only be reassembled to {}",
                    default_format.display_name(),
                    default_format.display_name()
                )));
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
Disassemble configuration files (XML, JSON, JSON5, JSONC, YAML, TOON, TOML, INI) into smaller\n\
files and reassemble the original on demand.\n\
\n\
USAGE:\n\
    config-disassembler <subcommand> [args...]\n\
\n\
SUBCOMMANDS:\n\
    xml      Disassemble or reassemble an XML file.\n\
    json     Disassemble or reassemble a JSON file.\n\
    json5    Disassemble or reassemble a JSON5 file.\n\
    jsonc    Disassemble or reassemble a JSONC file.\n\
    yaml     Disassemble or reassemble a YAML file.\n\
    toon     Disassemble or reassemble a TOON file.\n\
    toml     Disassemble or reassemble a TOML file (TOML <-> TOML only).\n\
    ini      Disassemble or reassemble an INI file (INI <-> INI only).\n\
    help     Show this help text.\n\
\n\
Run `config-disassembler <subcommand> --help` for subcommand details.\n",
        ver = env!("CARGO_PKG_VERSION")
    );
}

fn print_format_help(format: Format) {
    // Dispatch up front so every branch is reachable: the same-format-only
    // formats (TOML, INI) get their dedicated help text, and the
    // cross-format family shares one help body.
    match format {
        Format::Toml => {
            print_same_format_help(
                format,
                "TOML can only be converted to and from TOML. Cross-format conversion with\n\
JSON, JSON5, JSONC, YAML, TOON, or INI is rejected because TOML cannot represent `null`,\n\
forbids array roots, and forces bare keys to precede tables (which would\n\
reorder values on round-trip).",
                "                                (TOML disallows array roots, so this only applies to nested arrays.)",
            );
        }
        Format::Ini => {
            print_same_format_help(
                format,
                "INI can only be converted to and from INI. Cross-format conversion with\n\
JSON, JSON5, JSONC, YAML, TOON, or TOML is rejected because INI stores section\n\
values as strings or valueless keys and cannot represent arrays or deeper nesting.",
                "                                (INI cannot represent arrays, so this normally does not apply.)",
            );
        }
        Format::Json | Format::Json5 | Format::Jsonc | Format::Yaml | Format::Toon => {
            print_cross_format_help(format);
        }
    }
}

fn print_same_format_help(format: Format, explanation: &str, unique_id_note: &str) {
    eprintln!(
        "config-disassembler {format} <action> [options]\n\
\n\
{explanation}\n\
\n\
ACTIONS:\n\
    disassemble <input>   Split <input>.{extension} into a directory of {display_name} files.\n\
                          <input> may also be a directory; every .{extension} file\n\
                          beneath it is disassembled in place.\n\
    reassemble  <dir>     Rebuild the original {display_name} file from <dir>.\n\
\n\
DISASSEMBLE OPTIONS:\n\
    -o, --output-dir <dir>      Output directory (default: <input-stem> next to input).\n\
                                Not allowed when <input> is a directory.\n\
    --unique-id <field>         For array roots, name files by this field on each element.\n\
{unique_id_note}\n\
    --ignore-path <path>        Path to a .gitignore-style file used when <input> is a\n\
                                directory (default: .cdignore in the input directory).\n\
    --pre-purge                 Remove the output directory before writing.\n\
    --post-purge                Delete the input file after disassembly.\n\
\n\
REASSEMBLE OPTIONS:\n\
    -o, --output <file>         Output file (default: derived from metadata next to input dir).\n\
    --post-purge                Remove the input directory after reassembly.\n",
        extension = format.extension(),
        display_name = format.display_name()
    );
}

fn print_cross_format_help(format: Format) {
    let compatible_formats = format_list(format.compatible_formats());
    eprintln!(
        "config-disassembler {format} <action> [options]\n\
\n\
ACTIONS:\n\
    disassemble <input>   Split <input> into a directory of smaller files.\n\
                          <input> may also be a directory; every matching\n\
                          file beneath it is disassembled in place.\n\
    reassemble  <dir>     Rebuild the original file from <dir>.\n\
\n\
DISASSEMBLE OPTIONS:\n\
    -o, --output-dir <dir>      Output directory (default: <input-stem> next to input).\n\
                                Not allowed when <input> is a directory.\n\
    --input-format <fmt>        Override the input format (default: inferred from extension or `{format}`).\n\
    --output-format <fmt>       Format used for the split files (default: same as input).\n\
    --unique-id <field>         For array roots, name files by this field on each element.\n\
    --ignore-path <path>        Path to a .gitignore-style file used when <input> is a\n\
                                directory (default: .cdignore in the input directory).\n\
    --pre-purge                 Remove the output directory before writing.\n\
    --post-purge                Delete the input file after disassembly.\n\
\n\
REASSEMBLE OPTIONS:\n\
    -o, --output <file>         Output file (default: derived from metadata next to input dir).\n\
    --output-format <fmt>       Format to write the reassembled file in (default: original source format).\n\
    --post-purge                Remove the input directory after reassembly.\n\
\n\
<fmt> is one of: {compatible_formats}. (TOML and INI are excluded -- use their dedicated subcommands.)\n"
    );
}

fn format_list(formats: &[Format]) -> String {
    formats
        .iter()
        .map(|format| format.canonical_name())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_reassemble_rejects_second_positional_argument() {
        // Pins `_ if input_dir.is_none() => input_dir = Some(...)` against
        // a `with true` mutant on the match guard. With the original guard,
        // the second positional argument falls through to the `_ =>`
        // catch-all and returns `Error::Usage("unexpected argument
        // `dir2`")`. With the mutant the guard is always `true`, so `dir2`
        // silently overwrites `input_dir` and the function then tries to
        // reassemble a non-existent directory, returning a filesystem
        // error variant instead (or `Ok(())` if the path happened to
        // exist). Either way, the surfaced error is no longer
        // `Error::Usage` with this exact prefix.
        let args = vec!["dir1".to_string(), "dir2".to_string()];
        let err = run_reassemble(Format::Json, args)
            .expect_err("two positional dirs must be rejected as a usage error");
        match err {
            Error::Usage(msg) => assert!(
                msg.contains("unexpected argument `dir2`"),
                "expected usage error to mention the second positional arg, got: {msg}"
            ),
            other => panic!("expected Error::Usage, got {other:?}"),
        }
    }

    #[test]
    fn run_reassemble_first_positional_accepted_as_input_dir() {
        // Sibling sanity check: confirms the guard's `is_none()` *true*
        // branch is the one wired to assignment. If the guard were
        // inverted (a `!` insertion mutant), the first positional would
        // also fall through to the catch-all and we'd see a usage error
        // here too. The reassemble call itself will fail because
        // "missing-dir" does not exist, but the error variant must not
        // be `Error::Usage("unexpected argument ...")`.
        let args = vec!["missing-dir".to_string()];
        let err = run_reassemble(Format::Json, args)
            .expect_err("non-existent input dir must surface a reassemble error");
        if let Error::Usage(msg) = &err {
            assert!(
                !msg.contains("unexpected argument"),
                "first positional should not be flagged as unexpected: {msg}"
            );
        }
    }
}
