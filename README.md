# config-disassembler

[![Crates.io](https://img.shields.io/crates/v/config-disassembler.svg)](https://crates.io/crates/config-disassembler)
[![Docs.rs](https://docs.rs/config-disassembler/badge.svg)](https://docs.rs/config-disassembler)
[![CI](https://github.com/mcarvin8/config-disassembler/workflows/CI/badge.svg)](https://github.com/mcarvin8/config-disassembler/actions)
[![codecov](https://codecov.io/gh/mcarvin8/config-disassembler/graph/badge.svg?token=XZYXBXGENK)](https://codecov.io/gh/mcarvin8/config-disassembler)
[![Mutation score](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/mcarvin8/aafbb17a2728e06732803dfc9a1d7978/raw/mutants-badge.json)](https://github.com/mcarvin8/config-disassembler/actions/workflows/mutation.yml)
[![CodSpeed Badge](https://img.shields.io/endpoint?url=https://app.codspeed.io//badge.json)](https://app.codspeed.io//mcarvin8/config-disassembler?utm_source=badge)

Split large XML files into smaller, version-control–friendly pieces and reassemble them back into the original file.

Built for deeply nested XML such as Salesforce metadata (permission sets, profiles, flows, bots), where a single file can run to tens of thousands of lines and every change produces a noisy diff.

> **Breaking change in 0.12.0:** support for JSON, JSON5, JSONC, YAML, TOON, TOML, and INI *input* files was removed. Those formats are far simpler to split and merge than XML, and the project now focuses on XML. The `json`, `json5`, `jsonc`, `yaml`, `toon`, `toml`, and `ini` subcommands and the `disassemble`, `reassemble`, `format`, and `meta` library modules no longer exist. If you need them, pin `config-disassembler = "0.11"`. XML can still be split into JSON, JSON5, or YAML files with `--format`.

## Installation

```bash
cargo install config-disassembler
```

## Quick start

Split an XML file:

```bash
config-disassembler xml disassemble flow.xml   --unique-id-elements name,id
```

Result:

```text
flow/
├── assignments/
├── decisions/
├── screens/
└── flow-meta.xml
```

Rebuild it:

```bash
config-disassembler xml reassemble flow
```

The output matches the original `flow.xml`.

## CLI overview

```text
config-disassembler xml <command> [options]

Commands:
  disassemble <path>   Split an XML file, or every XML file under a directory
  reassemble  <path>   Rebuild the original XML file from a disassembled directory
```

Run `config-disassembler xml --help` for every option.

## Common options

| Option | Description |
|---|---|
| `--unique-id-elements <list>` | Fields used to name split files |
| `--strategy <name>` | `unique-id` (default) or `grouped-by-tag` |
| `--format <fmt>` | Split-file format: `xml` (default), `json`, `json5`, `yaml` |
| `--ignore-path <path>` | Ignore file path (default `.cdignore`) |
| `--prepurge` | Remove existing output before writing |
| `--postpurge` | Remove input after success |

XML can be split into XML, JSON, JSON5, or YAML files and is always reassembled back into XML.

See [docs/xml.md](docs/xml.md) for every option, plus:

- unique-id and grouped-by-tag strategies
- split tags
- multi-level disassembly
- sidecar elements
- round-trip verification

## Ignore files

Directory disassembly supports `.gitignore`-style filtering using `.cdignore`.

Example:

```text
**/generated/
**/Legacy_*.permissionset-meta.xml
```

Usage:

```bash
config-disassembler xml disassemble force-app/main/default/permissionsets
```

## Library usage

```rust
use config_disassembler::xml::{
    DisassembleXmlFileHandler, ReassembleXmlFileHandler,
    verify_roundtrip, RoundtripStatus, VerifyOptions,
};
```

See [docs/xml.md](docs/xml.md#round-trip-verification-library-api) for round-trip verification.

## Node.js bindings

Node.js support is available via napi-rs bindings: [config-disassembler-node](https://github.com/mcarvin8/config-disassembler-node)

## Documentation

- [Examples](docs/examples.md)
- [XML features](docs/xml.md)

## License

[MIT](LICENSE.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
