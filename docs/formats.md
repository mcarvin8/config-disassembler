# Format Behavior

Describes how `config-disassembler` handles each format: conversions, splitting behavior, metadata, and format-specific limits.

---

## Cross-format formats

These formats convert freely between each other:

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

Conversions preserve parsed values and structure. Format-specific syntax may not survive:

- JSONC comments are lost when rebuilding as JSON
- YAML anchors are resolved during parsing
- JSON5 trailing commas are normalized

---

## Root splitting behavior

Disassembly behavior depends on the root document type.

### Object roots

- Nested objects and arrays become separate files
- Scalar values remain in `_main.<ext>`

Example input:

```json
{
  "database": { "host": "localhost" },
  "features": { "beta": true },
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

`_main.json` contains:

```json
{ "version": 1 }
```

### Array roots

Each array item becomes its own file, named by zero-padded index:

```text
0000.json
0001.json
0002.json
```

Use `--unique-id <field>` to name items by a field value instead:

```bash
config-disassembler json disassemble users.json --unique-id id
```

Result:

```text
users/
├── 1001.json
├── 1002.json
└── 1003.json
```

Supported for JSON, JSON5, JSONC, YAML, TOON. Not applicable to TOML or INI.

---

## Metadata sidecar

Every disassembly writes `.config-disassembler.json` alongside the split files.

Stores information required for deterministic reassembly:

- original filename and root type
- source and output format
- original key and array ordering

Do not edit or remove this file. Missing it may prevent correct reassembly.

---

## XML

XML splits into XML, JSON, JSON5, or YAML — but reassembly always produces XML.

See [xml.md](xml.md) for strategies, split tags, and multi-level disassembly.

---

## TOML

TOML is isolated: **TOML ↔ TOML only**.

Cross-format conversion is not supported because:

- No `null` equivalent
- Root must be a table (arrays as root are invalid TOML)
- Key ordering constraints (bare keys must precede tables in the same scope)

### TOML disassembly behavior

Each split file is wrapped using its parent key so the file remains valid TOML:

```toml
[dependencies]
serde = "1"
tokio = "1"
```

Arrays use TOML table-array syntax:

```toml
[[servers]]
name = "api"
```

Reassembly automatically unwraps these structures.

---

## INI

INI is isolated: **INI ↔ INI only**.

Cross-format conversion is not supported because:

- All values are strings — no booleans, numbers, or `null`
- No native arrays
- Shallow nesting only
- Inconsistent parser behavior across ecosystems

### INI disassembly behavior

Split files are wrapped using their parent section name:

```ini
[database]
host=localhost
port=5432
```

Reassembly unwraps sections automatically.

---

## JSONC behavior

JSONC supports comments and trailing commas.

When disassembled and reassembled as JSONC, these are preserved where possible:

```jsonc
{
  // API settings
  "port": 8080,
}
```

Cross-format conversions discard comments and JSONC-specific syntax.

---

## YAML behavior

YAML-specific features — anchors, aliases, tags, comments — may not survive cross-format conversions.

Anchors are resolved during parsing and are not reconstructed during reassembly.

---

## JSON5 behavior

JSON5 features (comments, trailing commas, unquoted keys, single-quoted strings) are preserved when rebuilding as JSON5.

Cross-format conversions preserve values but normalize syntax.

---

## TOON behavior

TOON behaves like JSON/YAML-style formats. Supports object roots, array roots, and nested structures. Cross-format conversions fully supported within the JSON/YAML/TOON family.

---

## Directory disassembly

All non-XML formats support directory input:

```bash
config-disassembler yaml disassemble envs/
```

Recursively walks the directory, disassembles matching files in place, and creates sibling output directories:

```text
envs/
├── dev.yaml
├── prod.yaml
├── dev/
└── prod/
```

---

## Ignore files

Directory traversal supports `.gitignore`-style filtering via `.cdignore`:

```text
**/generated/
**/secret.yaml
```

Supply a custom path:

```bash
--ignore-path custom.ignore
```

---

## Purge behavior

### `--prepurge`

Deletes existing output before writing. Useful for clean rebuilds and CI pipelines:

```bash
config-disassembler json disassemble config.json --prepurge
```

### `--postpurge`

Deletes the input after successful completion:

```bash
config-disassembler json disassemble config.json --postpurge
config-disassembler json reassemble config --postpurge
```

Use with care.

---

## Logging

```bash
RUST_LOG=debug config-disassembler json disassemble config.json
```

---

## Deterministic reassembly

Reassembly preserves original ordering, root structure, nested hierarchy, and split-file relationships.

Exact formatting may differ (indentation, quote style, whitespace) depending on the serializer. Structural equivalence is always preserved.
