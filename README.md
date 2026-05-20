# config-disassembler

[![Crates.io](https://img.shields.io/crates/v/config-disassembler.svg)](https://crates.io/crates/config-disassembler)
[![Docs.rs](https://docs.rs/config-disassembler/badge.svg)](https://docs.rs/config-disassembler)
[![CI](https://github.com/mcarvin8/config-disassembler/workflows/CI/badge.svg)](https://github.com/mcarvin8/config-disassembler/actions)
[![codecov](https://codecov.io/gh/mcarvin8/config-disassembler/graph/badge.svg?token=XZYXBXGENK)](https://codecov.io/gh/mcarvin8/config-disassembler)
[![Mutation score](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/mcarvin8/aafbb17a2728e06732803dfc9a1d7978/raw/mutants-badge.json)](https://github.com/mcarvin8/config-disassembler/actions/workflows/mutation.yml)

Split large configuration files into smaller, version-control–friendly pieces and reassemble them back into the original file.

Supported formats:

- XML
- JSON
- JSON5
- JSONC
- YAML
- TOON
- TOML
- INI

## Installation

```bash
cargo install config-disassembler
```

## Quick start

### JSON

```bash
# Split config.json into ./config/
config-disassembler json disassemble config.json

# Rebuild config.json
config-disassembler json reassemble config
```

Result:

```text
config/
├── database.json
├── features.json
├── users.json
└── .config-disassembler.json
```

### XML

```bash
config-disassembler xml disassemble flow.xml \
  --unique-id-elements name,id
```

Result:

```text
flow/
├── assignments/
├── decisions/
├── screens/
└── flow-meta.xml
```

## Format support

### Cross-format conversions

These formats can be converted freely between each other:

- JSON
- JSON5
- JSONC
- YAML
- TOON

Example:

```bash
# Split JSON into YAML files
config-disassembler json disassemble config.json --output-format yaml

# Rebuild as JSON
config-disassembler json reassemble config --output-format json
```

### XML

XML can be split into:

- XML
- JSON
- JSON5
- YAML

…and reassembled back into XML.

Advanced XML features are documented separately:

- unique-id strategy
- grouped-by-tag strategy
- split tags
- multi-level disassembly

See [docs/xml.md](docs/xml.md).

### TOML and INI

TOML and INI are intentionally isolated:

- TOML ↔ TOML only
- INI ↔ INI only

This avoids lossy or invalid conversions.

See [docs/formats.md](docs/formats.md) for details.

## CLI overview

```text
config-disassembler <format> <command>

Formats:
  xml
  json
  json5
  jsonc
  yaml
  toon
  toml
  ini

Commands:
  disassemble
  reassemble
  parse (XML only)
```

Examples:

```bash
config-disassembler yaml disassemble envs/
config-disassembler toml reassemble Cargo
config-disassembler xml parse flow.xml
```

## Common options

| Option | Description |
|---|---|
| `--output-format <fmt>` | Output format |
| `--unique-id <field>` | Name array items using a field |
| `--ignore-path <path>` | Ignore file path |
| `--prepurge` | Remove existing output before writing |
| `--postpurge` | Remove input after success |

## Ignore files

Directory disassembly supports `.gitignore`-style filtering using `.cdignore`.

Example:

```text
**/secret.json
**/generated/
```

Usage:

```bash
config-disassembler yaml disassemble envs/
```

## XML strategies

### unique-id (default)

Each nested XML element is written to its own file using a unique identifier.

```bash
config-disassembler xml disassemble flow.xml \
  --unique-id-elements name,id
```

Best for:

- fine-grained diffs
- version control
- large metadata files

### grouped-by-tag

Groups nested elements by tag into shared files.

```bash
config-disassembler xml disassemble flow.xml \
  --strategy grouped-by-tag
```

Best for:

- fewer files
- quick inspection
- simpler layouts

See [docs/xml.md](docs/xml.md) for advanced XML configuration.

## Library usage

```rust
use config_disassembler::disassemble::{disassemble, DisassembleOptions};
use config_disassembler::reassemble::{reassemble, ReassembleOptions};
use config_disassembler::format::Format;
```

## Node.js bindings

Node.js support is available via napi-rs bindings:

- https://github.com/mcarvin8/config-disassembler-node

## Documentation

- [Examples](docs/examples.md)
- [XML features](docs/xml.md)
- [Format behavior](docs/formats.md)

## License

[MIT](LICENSE.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
