# config-disassembler

[![Crates.io](https://img.shields.io/crates/v/config-disassembler.svg)](https://crates.io/crates/config-disassembler)
[![Docs.rs](https://docs.rs/config-disassembler/badge.svg)](https://docs.rs/config-disassembler)
[![CI](https://github.com/mcarvin8/config-disassembler/workflows/CI/badge.svg)](https://github.com/mcarvin8/config-disassembler/actions)
[![codecov](https://codecov.io/gh/mcarvin8/config-disassembler/graph/badge.svg?token=XZYXBXGENK)](https://codecov.io/gh/mcarvin8/config-disassembler)

Disassemble configuration files into smaller, version-control–friendly pieces and reassemble the original on demand. Supported formats: **XML**, **JSON**, **JSON5**, and **YAML**. A file in any one format can be split into files in any other supported format and then reassembled back to any supported format.

XML support is provided by the bundled [`xml-disassembler`](https://crates.io/crates/xml-disassembler) crate; the JSON, JSON5, and YAML disassembler is implemented in this crate.

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

`<fmt>` is one of `json`, `json5`, `yaml`.

### Example: disassemble a JSON file into YAML, then rebuild as JSON

```bash
# split config.json into per-key YAML files under ./config/
config-disassembler json disassemble config.json --output-format yaml

# rebuild a config.json from those YAML files
config-disassembler json reassemble config --output-format json
```

## How disassembly works (JSON / JSON5 / YAML)

* **Object roots** – Every top-level key whose value is an object or array
  is written to its own file (`<key>.<ext>`). Top-level keys with scalar
  values (string, number, boolean, null) are bundled together into
  `_main.<ext>`.
* **Array roots** – Each array element is written to its own file. With
  `--unique-id <field>` the file is named by that field's value on each
  element; otherwise files are named by zero-padded index.
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
