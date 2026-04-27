# config-disassembler

[![Crates.io](https://img.shields.io/crates/v/config-disassembler.svg)](https://crates.io/crates/config-disassembler)
[![Docs.rs](https://docs.rs/config-disassembler/badge.svg)](https://docs.rs/config-disassembler)
[![CI](https://github.com/mcarvin8/config-disassembler/workflows/CI/badge.svg)](https://github.com/mcarvin8/config-disassembler/actions)
[![codecov](https://codecov.io/gh/mcarvin8/config-disassembler/graph/badge.svg?token=XZYXBXGENK)](https://codecov.io/gh/mcarvin8/config-disassembler)

Disassemble configuration files into smaller, version-control–friendly pieces and reassemble the original on demand. Supported formats: **XML**, **JSON**, **JSON5**, **YAML**, and **TOML**.

A JSON, JSON5, or YAML file can be split into files in any of those three formats and reassembled back into any of them. **TOML is intentionally isolated** — it can only be split into TOML files and reassembled to TOML. See [TOML isolation](#toml-isolation) below for the rationale.

XML support is provided by the bundled [`xml-disassembler`](https://crates.io/crates/xml-disassembler) crate; the JSON, JSON5, YAML, and TOML disassembler is implemented in this crate.

## Installation

### Cargo

* Install the rust toolchain in order to have cargo installed by following
  [this](https://www.rust-lang.org/tools/install) guide.
* run `cargo install config-disassembler`

## CLI overview

```
config-disassembler <subcommand> [args...]

Subcommands:
  xml      Forward to the bundled xml-disassembler CLI.
  json     Disassemble or reassemble a JSON file.
  json5    Disassemble or reassemble a JSON5 file.
  yaml     Disassemble or reassemble a YAML file.
  toml     Disassemble or reassemble a TOML file (TOML <-> TOML only).
  help     Show top-level help.
```

### XML

Arguments after `xml` are forwarded directly to `xml-disassembler`. See its
[README](https://github.com/mcarvin8/xml-disassembler-rust) for the full list of
options.

```bash
config-disassembler xml disassemble path/to/file.xml --format json
config-disassembler xml reassemble  path/to/file    --postpurge
```

### JSON / JSON5 / YAML

```bash
config-disassembler <fmt> disassemble <input> [options]
config-disassembler <fmt> reassemble  <dir>   [options]
```

Common options:

| Option | Applies to | Description |
| ------ | ---------- | ----------- |
| `-o, --output-dir <dir>` | disassemble | Directory for split files. Defaults to `<input-stem>` next to the input. |
| `--input-format <fmt>`   | disassemble | Override input format. Defaults to the file extension or the subcommand. |
| `--output-format <fmt>`  | both        | Format used for the split files (disassemble) or rebuilt file (reassemble). |
| `--unique-id <field>`    | disassemble | For array roots, name files by this field on each element. |
| `--pre-purge`            | disassemble | Remove the output directory before writing. |
| `--post-purge`           | both        | Delete the input file/directory after the operation succeeds. |
| `-o, --output <file>`    | reassemble  | Output file path. Defaults to the original file name from the metadata. |

`<fmt>` is one of `json`, `json5`, `yaml`. (TOML is excluded from these flags — use the dedicated `toml` subcommand.)

### Example: disassemble a JSON file into YAML, then rebuild as JSON

```bash
# split config.json into per-key YAML files under ./config/
config-disassembler json disassemble config.json --output-format yaml

# rebuild a config.json from those YAML files
config-disassembler json reassemble config --output-format json
```

### TOML

```bash
config-disassembler toml disassemble <input.toml> [options]
config-disassembler toml reassemble  <dir>        [options]
```

The `toml` subcommand is identical to the JSON/JSON5/YAML subcommands except `--input-format` and `--output-format` are not accepted: TOML files can only be split into TOML files and reassembled into TOML.

```bash
# split Cargo.toml into per-table files under ./Cargo/
config-disassembler toml disassemble Cargo.toml

# rebuild Cargo.toml from the split directory
config-disassembler toml reassemble Cargo
```

#### TOML isolation

TOML cannot participate in cross-format conversions because:

* TOML has no `null` value (JSON/JSON5/YAML do).
* TOML's document root must be a table; array roots are forbidden.
* TOML requires bare keys to come *before* any tables in a given mapping, so round-tripping a JSON object like `{"section": {...}, "name": "x"}` through TOML and back would reorder the keys to `{"name": "x", "section": {...}}`.

Trying to mix formats with TOML returns a clear error:

```text
TOML can only be converted to and from TOML; got input=json, output=toml
```

To preserve TOML's table-vs-bare-key ordering rule, the TOML disassembler wraps each per-key split file under its parent key. For example, disassembling a Cargo.toml produces files like `dependencies.toml` containing `[dependencies]` headers (not a bare value list), which is the idiomatic TOML representation. Reassembly unwraps them automatically using the metadata sidecar.

## How disassembly works (JSON / JSON5 / YAML / TOML)

* **Object roots** – Every top-level key whose value is an object or array
  is written to its own file (`<key>.<ext>`). Top-level keys with scalar
  values (string, number, boolean, null) are bundled together into
  `_main.<ext>`.
* **Array roots** – Each array element is written to its own file. With
  `--unique-id <field>` the file is named by that field's value on each
  element; otherwise files are named by zero-padded index. (Not applicable
  to TOML — TOML forbids array roots.)
* **TOML wrapping** – For TOML output only, each per-key split file is
  written as a single-table document keyed by its parent (e.g.
  `servers.toml` contains `[[servers]]` headers, not a bare array). This
  keeps every split file a valid TOML document and is unwrapped during
  reassembly.
* **Metadata** – A `.config-disassembler.json` sidecar is written into the
  output directory recording the original key order, root type, source
  format, and the format the split files were written in. Reassembly uses
  this to rebuild the original document deterministically.

## Library

The crate also exposes a library API:

```rust
use config_disassembler::disassemble::{disassemble, DisassembleOptions};
use config_disassembler::reassemble::{reassemble, ReassembleOptions};
use config_disassembler::format::Format;
```

## License

[MIT](LICENSE.md)

## Contribution

See [CONTRIBUTING.md](CONTRIBUTING.md).
