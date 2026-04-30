# config-disassembler

[![Crates.io](https://img.shields.io/crates/v/config-disassembler.svg)](https://crates.io/crates/config-disassembler)
[![Docs.rs](https://docs.rs/config-disassembler/badge.svg)](https://docs.rs/config-disassembler)
[![CI](https://github.com/mcarvin8/config-disassembler/workflows/CI/badge.svg)](https://github.com/mcarvin8/config-disassembler/actions)
[![codecov](https://codecov.io/gh/mcarvin8/config-disassembler/graph/badge.svg?token=XZYXBXGENK)](https://codecov.io/gh/mcarvin8/config-disassembler)

Disassemble configuration files into smaller, version-control–friendly pieces and reassemble the original on demand. Supported formats:

1. **XML**
1. **JSON**
1. **JSON5**
1. **JSONC**
1. **YAML**
1. **TOON**
1. **TOML**
1. **INI**

JSON, JSON5, JSONC, YAML, and TOON files can be split and reassembled among those five formats. XML files can be split into XML, JSON, JSON5, or YAML files and reassembled from any of those split-file formats back to XML. **TOML and INI are intentionally isolated** — each can only be split and reassembled within the same format. See [TOML and INI isolation](#toml-and-ini-isolation) below for the rationale.

For file-tree diagrams and before/after layouts, see [Examples](docs/examples.md).

> 💡 **Node.js support available**  
> This crate can also be used in Node.js environments via Neon bindings:  
> 👉 [config-disassembler-node](https://github.com/mcarvin8/config-disassembler-node)

## Installation

### Cargo

* Install the rust toolchain in order to have cargo installed by following
  [this](https://www.rust-lang.org/tools/install) guide.
* run `cargo install config-disassembler`

## CLI overview

```
config-disassembler <subcommand> [args...]

Subcommands:
  xml      Disassemble or reassemble an XML file.
  json     Disassemble or reassemble a JSON file.
  json5    Disassemble or reassemble a JSON5 file.
  jsonc    Disassemble or reassemble a JSONC file.
  yaml     Disassemble or reassemble a YAML file.
  toon     Disassemble or reassemble a TOON file.
  toml     Disassemble or reassemble a TOML file (TOML <-> TOML only).
  ini      Disassemble or reassemble an INI file (INI <-> INI only).
  help     Show top-level help.
```

### Ignore file

Every `disassemble` action accepts an `--ignore-path` flag pointing at a `.gitignore`-style file used to exclude paths when the input is a directory. The default filename is `.cdignore`, located in the input directory. For backward compatibility the `xml` subcommand also falls back to `.xmldisassemblerignore` (with a deprecation warning) if `.cdignore` is missing — rename or pass `--ignore-path` to silence the warning.

```text
# .cdignore
**/secret.json
**/generated/
```

### Logging

Logging uses the [log](https://crates.io/crates/log) crate with [env_logger](https://crates.io/crates/env_logger). Control verbosity for any subcommand via the `RUST_LOG` environment variable.

```bash
# Verbose logging (debug level)
RUST_LOG=debug config-disassembler <subcommand> ...
```

### XML

```bash
config-disassembler xml disassemble <path> [options]
config-disassembler xml reassemble  <path> [extension] [--postpurge]
config-disassembler xml parse       <path>
```

`<path>` may be a single XML file or a directory of XML files. The command set, flags, and on-disk layout match the standalone [`xml-disassembler`](https://github.com/mcarvin8/xml-disassembler-rust) CLI; this section is the inline reference.

#### Disassemble options

| Option | Description | Default |
|--------|-------------|---------|
| `--unique-id-elements <list>` | Comma-separated element names used to derive filenames for nested elements | (none) |
| `--prepurge` | Remove existing disassembly output before running | false |
| `--postpurge` | Delete original file/directory after disassembling | false |
| `--ignore-path <path>` | Path to the ignore file | `.cdignore` (falls back to `.xmldisassemblerignore`) |
| `--format <fmt>` | Output format: `xml`, `json`, `json5`, `yaml` | `xml` |
| `--strategy <name>` | `unique-id` or `grouped-by-tag` | `unique-id` |
| `-p`, `--split-tags <spec>` | With `grouped-by-tag`: split or group nested tags into subdirs (e.g. `objectPermissions:split:object,fieldPermissions:group:field`) | (none) |
| `--multi-level <spec>` | Further disassemble matching files: `file_pattern:root_to_strip:unique_id_elements`. Multiple rules separated by `;`. | (none) |

#### Reassemble options

| Option | Description | Default |
|--------|-------------|---------|
| `<extension>` | File extension/suffix for the rebuilt XML (e.g. `permissionset-meta.xml`) | `xml` |
| `--postpurge` | Delete disassembled directory after successful reassembly | false |

#### Disassembly strategies

##### `unique-id` (default)

Each nested element is written to its own file, named by a unique identifier (or an 8-character SHA-256 hash if no UID is available). Leaf content stays in a file named after the original XML.

Best for fine-grained diffs and version control.

* **UID-based layout** – When you provide `--unique-id-elements` (e.g. `name,id,apexClass`), nested elements are named by the first matching field value. For Salesforce flows, a typical list might be: `apexClass,name,object,field,layout,actionName,targetReference,assignToReference,choiceText,promptText`. Using unique-id elements also ensures predictable sorting in the reassembled output.
* **Hash-based layout** – When no unique ID is found, elements are named with an 8-character hash of their content (e.g. `419e0199.botMlDomain-meta.xml`).

##### `grouped-by-tag`

All nested elements with the same tag go into one file per tag. Leaf content stays in the base file named after the original XML.

Best for fewer files and quick inspection.

```bash
config-disassembler xml disassemble ./my.xml --strategy grouped-by-tag --format yaml
```

Reassembly preserves element content and structure.

###### Split tags (`-p` / `--split-tags`)

With `--strategy grouped-by-tag`, you can optionally **split** or **group** specific nested tags into subdirectories instead of a single file per tag. Useful for permission sets and similar metadata: e.g. one file per `objectPermissions` under `objectPermissions/`, and `fieldPermissions` grouped by object under `fieldPermissions/`.

The spec is a comma-separated list of rules. Each rule is `tag:mode:field` or `tag:path:mode:field` (path defaults to tag). **mode** is `split` (one file per array item, filename from `field`) or `group` (group array items by `field`, one file per group).

```bash
# Permission set: objectPermissions -> one file per object;
# fieldPermissions -> one file per field value
config-disassembler xml disassemble fixtures/split-tags/HR_Admin.permissionset-meta.xml \
  --strategy grouped-by-tag \
  -p "objectPermissions:split:object,fieldPermissions:group:field"
```

This creates `HR_Admin/` with files like `objectPermissions/Job_Request__c.objectPermissions-meta.xml`, `objectPermissions/Account.objectPermissions-meta.xml`, `fieldPermissions/<fieldValue>.fieldPermissions-meta.xml`, plus the main `HR_Admin.permissionset-meta.xml` with the rest. Reassembly requires no extra flags: `xml reassemble` merges subdirs and files back into one XML.

##### Multi-level disassembly

For advanced use cases (e.g. Salesforce Loyalty Program Setup metadata), you can further disassemble specific output files by stripping a root element and re-running disassembly with different unique-id elements.

Use `--multi-level <spec>` where the spec is:

`file_pattern:root_to_strip:unique_id_elements`

* **file_pattern** – Match XML files whose name or path contains this (e.g. `programProcesses` or `programProcesses-meta`).
* **root_to_strip** – Element to strip/unwrap: if it is the root, its inner content becomes the new document; if it is a child (e.g. `programProcesses` under `LoyaltyProgramSetup`), it is unwrapped so its inner content becomes the root's direct children.
* **unique_id_elements** – Comma-separated element names for the second-level disassembly (e.g. `parameterName,ruleName`).

Example (loyalty program): strip the child `programProcesses` in each process file so parameters/rules can be disassembled:

```bash
config-disassembler xml disassemble ./Cloud_Kicks_Inner_Circle.loyaltyProgramSetup-meta.xml \
  --unique-id-elements "fullName,name,processName" \
  --multi-level "programProcesses:programProcesses:parameterName,ruleName"
```

A `.multi_level.json` config is written in the disassembly root so **reassemble** automatically does inner-level reassembly first, wraps files with the original root, then reassembles the top level. No extra flags are needed for reassembly.

###### Multiple multi-level rules

When a single XML file has more than one nested-array section that you want to split (e.g. an Agentforce bot version with `botDialogs` and `mlIntents`, or a custom metadata type with `sectionA` and `sectionB`), pass multiple rules separated by `;`. Each rule produces its own segment subdirectory under the disassembly root and is persisted as a separate entry in `.multi_level.json`; reassembly walks the rules in order.

```bash
config-disassembler xml disassemble ./Sample.multi-meta.xml \
  --unique-id-elements "id,name,label" \
  --multi-level "sectionA:sectionA:id;sectionB:sectionB:name"
```

Each rule still has the same shape (`file_pattern:root_to_strip:unique_id_elements`); the third part is comma-separated, which is why rules are joined with `;`. Whitespace around each rule is trimmed; trailing or empty rules (e.g. `a:R:id;`) are skipped silently.

> **Caveat:** Multi-level reassembly removes disassembled directories after reassembling each level, even when you do not pass `--postpurge`. This is required so the next level can merge the reassembled XML files. Use version control (e.g. Git) to recover the tree if needed, or run reassembly only in a pipeline where these changes can be discarded.

#### XML parser notes

Parsing is done with [quick-xml](https://github.com/tafia/quick-xml), with support for:

* **CDATA** – Preserved and output as `#cdata` in the parsed structure.
* **Comments** – Preserved in the XML output.
* **Attributes** – Stored with `@` prefix (e.g. `@version`, `@encoding`).

### JSON / JSON5 / JSONC / YAML / TOON

```bash
config-disassembler <fmt> disassemble <input> [options]
config-disassembler <fmt> reassemble  <dir>   [options]
```

`<input>` may be a single file or a directory. When it points at a directory, every file under the directory whose extension matches the input format is disassembled in place; each file's split output is written into a sibling directory named after that file's stem.

Common options:

| Option | Applies to | Description |
| ------ | ---------- | ----------- |
| `-o, --output-dir <dir>` | disassemble (file input only) | Directory for split files. Defaults to `<input-stem>` next to the input. Rejected when `<input>` is a directory. |
| `--input-format <fmt>`   | disassemble | Override input format. Defaults to the file extension or the subcommand. |
| `--output-format <fmt>`  | both        | Format used for the split files (disassemble) or rebuilt file (reassemble). |
| `--unique-id <field>`    | disassemble | For array roots, name files by this field on each element. |
| `--ignore-path <path>`   | disassemble (directory input) | Path to a `.gitignore`-style file used to filter the directory walk. Defaults to `.cdignore` in the input directory. |
| `--pre-purge`            | disassemble | Remove the output directory before writing. |
| `--post-purge`           | both        | Delete the input file/directory after the operation succeeds. |
| `-o, --output <file>`    | reassemble  | Output file path. Defaults to the original file name from the metadata. |

`<fmt>` is one of `json`, `json5`, `jsonc`, `yaml`, `toon`. (TOML and INI are excluded from these flags — use their dedicated subcommands.)

JSONC input accepts comments and trailing commas. When JSONC is disassembled to JSONC and reassembled as JSONC, comments and trailing commas are preserved. Cross-format conversions preserve the parsed values, not JSONC-specific syntax.

### Example: disassemble a JSON file into YAML, then rebuild as JSON

```bash
# split config.json into per-key YAML files under ./config/
config-disassembler json disassemble config.json --output-format yaml

# rebuild a config.json from those YAML files
config-disassembler json reassemble config --output-format json
```

### Example: disassemble a JSON file into TOON, then rebuild as YAML

```bash
# split config.json into per-key TOON files under ./config/
config-disassembler json disassemble config.json --output-format toon

# rebuild a config.yaml from those TOON files
config-disassembler json reassemble config --output-format yaml
```

### Example: disassemble a directory of YAML files, skipping some

```bash
# .cdignore in ./envs/
echo 'secrets.yaml' > envs/.cdignore

# walks envs/, splits every *.yaml file in place, except secrets.yaml
config-disassembler yaml disassemble envs/
```

### TOML / INI

```bash
config-disassembler toml disassemble <input> [options]
config-disassembler toml reassemble  <dir>   [options]
config-disassembler ini  disassemble <input> [options]
config-disassembler ini  reassemble  <dir>   [options]
```

The `toml` and `ini` subcommands are identical to the JSON/JSON5/JSONC/YAML/TOON subcommands (single-file or directory input, `--ignore-path`, etc.) except `--input-format` and `--output-format` are not accepted: each format can only be split and reassembled within itself.

```bash
# split Cargo.toml into per-table files under ./Cargo/
config-disassembler toml disassemble Cargo.toml

# rebuild Cargo.toml from the split directory
config-disassembler toml reassemble Cargo
```

```bash
# split app.ini into per-section files under ./app/
config-disassembler ini disassemble app.ini

# rebuild app.ini from the split directory
config-disassembler ini reassemble app
```

#### TOML and INI isolation

TOML cannot participate in cross-format conversions because:

* TOML has no `null` value (JSON/JSON5/JSONC/YAML/TOON do).
* TOML's document root must be a table; array roots are forbidden.
* TOML requires bare keys to come *before* any tables in a given mapping, so round-tripping a JSON object like `{"section": {...}, "name": "x"}` through TOML and back would reorder the keys to `{"name": "x", "section": {...}}`.

INI is also same-format-only because section values are strings or valueless keys, and INI cannot represent arrays or deeper nesting without inventing a custom encoding.

Trying to mix formats with TOML or INI returns a clear error:

```text
TOML can only be converted to and from TOML; got input=json, output=toml
INI can only be converted to and from INI; got input=json, output=ini
```

To keep every split file valid for table-style formats, TOML and INI wrap each per-key split file under its parent key. For example, disassembling a Cargo.toml produces files like `dependencies.toml` containing `[dependencies]` headers, and disassembling an app.ini produces `settings.ini` containing `[settings]`. Reassembly unwraps them automatically using the metadata sidecar.

## How disassembly works (JSON / JSON5 / JSONC / YAML / TOON / TOML / INI)

* **Object roots** – Every top-level key whose value is an object or array
  is written to its own file (`<key>.<ext>`). Top-level keys with scalar
  values (string, number, boolean, null) are bundled together into
  `_main.<ext>`.
* **Array roots** – Each array element is written to its own file. With
  `--unique-id <field>` the file is named by that field's value on each
  element; otherwise files are named by zero-padded index. (Not applicable
  to TOML or INI.)
* **TOML/INI wrapping** – For TOML and INI output, each per-key split file is
  written as a single-table document keyed by its parent (e.g.
  `servers.toml` contains `[[servers]]` headers, not a bare array, and
  `settings.ini` contains `[settings]`). This keeps every split file a valid document and is unwrapped during
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
