# Format Behavior

This document describes how `config-disassembler` handles each supported configuration format, including cross-format conversions, metadata behavior, and format-specific limitations.

---

# Supported formats

## Cross-format formats

The following formats can be freely converted between each other:

- JSON
- JSON5
- JSONC
- YAML
- TOON

Example:

```bash
# Split JSON into YAML files
config-disassembler json disassemble config.json \
  --output-format yaml

# Rebuild as JSON
config-disassembler json reassemble config \
  --output-format json
```

These conversions preserve parsed values and structure.

Format-specific syntax may not survive conversion.

Examples:

- JSONC comments are lost when rebuilding as JSON
- YAML anchors are resolved during parsing
- JSON5 trailing commas are normalized

---

# XML

XML can be split into:

- XML
- JSON
- JSON5
- YAML

…but reassembly always produces XML.

XML behavior and advanced splitting strategies are documented separately in:

- [xml.md](xml.md)

---

# TOML

TOML is intentionally isolated:

- TOML ↔ TOML only

Cross-format conversions are not supported.

Example:

```text
TOML can only be converted to and from TOML; got input=json, output=toml
```

## Why TOML is isolated

TOML has structural constraints that do not map cleanly to JSON-style formats.

### No null values

TOML has no `null` equivalent.

Example:

```json
{
  "value": null
}
```

Cannot be represented in TOML without inventing custom semantics.

---

### Root must be a table

TOML documents cannot use arrays as the root value.

Valid JSON:

```json
[
  { "name": "a" },
  { "name": "b" }
]
```

Invalid TOML.

---

### Ordering constraints

TOML requires bare keys to appear before tables in the same scope.

This JSON:

```json
{
  "section": {
    "enabled": true
  },
  "name": "example"
}
```

Would need to reorder keys during TOML serialization.

Because reassembly aims to preserve deterministic structure, TOML conversions are restricted to TOML-only workflows.

---

## TOML disassembly behavior

To ensure every split file remains a valid TOML document, each shard is wrapped using its parent key.

Example:

```toml
[dependencies]
serde = "1"
tokio = "1"
```

Rather than writing a bare mapping fragment.

Arrays are similarly wrapped using TOML table-array syntax.

Example:

```toml
[[servers]]
name = "api"
```

Reassembly automatically unwraps these structures.

---

# INI

INI is also intentionally isolated:

- INI ↔ INI only

Cross-format conversions are not supported.

Example:

```text
INI can only be converted to and from INI; got input=json, output=ini
```

## Why INI is isolated

INI cannot reliably represent modern structured data formats.

Typical INI limitations include:

- values are strings
- no native arrays
- shallow nesting only
- inconsistent parser behavior across ecosystems

Example:

```ini
[settings]
enabled=true
```

This does not distinguish between:

- boolean `true`
- string `"true"`

Nested JSON-style structures also cannot be represented without inventing custom encoding rules.

---

## INI disassembly behavior

Like TOML, INI split files are wrapped using their parent section name.

Example:

```ini
[database]
host=localhost
port=5432
```

Reassembly unwraps sections automatically.

---

# Root splitting behavior

Disassembly behavior depends on the root document type.

---

## Object roots

For object-style roots:

- nested objects and arrays become separate files
- scalar values remain in `_main.<ext>`

Example input:

```json
{
  "database": {
    "host": "localhost"
  },
  "features": {
    "beta": true
  },
  "version": 1
}
```

Result:

```text
config/
├── database.json
├── features.json
├── _main.json
└── .config-disassembler.json
```

Where `_main.json` contains:

```json
{
  "version": 1
}
```

---

## Array roots

For array-style roots:

- each array item becomes its own file

By default, files use zero-padded numeric indexes.

Example:

```text
0000.json
0001.json
0002.json
```

---

## Unique IDs for arrays

Use `--unique-id <field>` to name array items using a field value.

Example:

```bash
config-disassembler json disassemble users.json \
  --unique-id id
```

Result:

```text
users/
├── 1001.json
├── 1002.json
└── 1003.json
```

This is supported for:

- JSON
- JSON5
- JSONC
- YAML
- TOON

It is not applicable to TOML or INI.

---

# Metadata sidecar

Every disassembly writes a metadata file:

```text
.config-disassembler.json
```

This stores information required for deterministic reassembly.

Typical metadata includes:

- original filename
- root type
- source format
- output format
- original key ordering
- array ordering

Example:

```json
{
  "source_format": "json",
  "output_format": "yaml",
  "root_type": "object"
}
```

Users normally do not need to edit this file manually.

Removing it may prevent correct reassembly.

---

# JSONC behavior

JSONC supports:

- comments
- trailing commas

When JSONC is:

- disassembled as JSONC
- reassembled as JSONC

…comments and trailing commas are preserved where possible.

Example:

```jsonc
{
  // API settings
  "port": 8080,
}
```

When converting JSONC into another format, only parsed values are preserved.

Comments and JSONC-specific syntax are discarded.

---

# YAML behavior

YAML parsing preserves parsed values and structure.

YAML-specific features such as:

- anchors
- aliases
- tags
- comments

may not survive cross-format conversions.

Example:

```yaml
defaults: &defaults
  enabled: true

service:
  <<: *defaults
```

Anchors are resolved during parsing and are not reconstructed during reassembly.

---

# JSON5 behavior

JSON5 supports:

- comments
- trailing commas
- unquoted keys
- single-quoted strings

Example:

```json5
{
  server: 'localhost',
}
```

These features are preserved when rebuilding as JSON5.

Cross-format conversions preserve values but normalize syntax.

---

# TOON behavior

TOON behaves similarly to JSON/YAML-style structured formats.

It supports:

- object roots
- array roots
- nested structures

Cross-format conversions are fully supported between:

- JSON
- JSON5
- JSONC
- YAML
- TOON

---

# Directory disassembly

All non-XML formats support directory input.

Example:

```bash
config-disassembler yaml disassemble envs/
```

Behavior:

- recursively walks the directory
- disassembles matching files in place
- creates sibling output directories

Example:

```text
envs/
├── dev.yaml
├── prod.yaml
├── dev/
└── prod/
```

---

# Ignore files

Directory traversal supports `.gitignore`-style filtering using `.cdignore`.

Example:

```text
**/generated/
**/secret.yaml
```

Custom paths may be supplied using:

```bash
--ignore-path custom.ignore
```

---

# Purge behavior

## `--prepurge`

Deletes existing output before writing.

Useful for:

- clean rebuilds
- CI pipelines

Example:

```bash
config-disassembler json disassemble config.json \
  --prepurge
```

---

## `--postpurge`

Deletes the input after successful completion.

Examples:

```bash
config-disassembler json disassemble config.json \
  --postpurge
```

```bash
config-disassembler json reassemble config \
  --postpurge
```

Use with care.

---

# Logging

Logging uses:

- `log`
- `env_logger`

Enable verbose output:

```bash
RUST_LOG=debug config-disassembler json disassemble config.json
```

---

# Deterministic reassembly

Reassembly preserves:

- original ordering
- root structure
- nested hierarchy
- split-file relationships

The goal is deterministic round-tripping between:

1. original source
2. disassembled representation
3. rebuilt output

Exact formatting may differ depending on the serializer used by the target format.

Examples:

- indentation
- quote style
- trailing commas
- whitespace
- comment placement

Structural equivalence is preserved even when formatting changes.
