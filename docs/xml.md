# XML Support

The `xml` subcommand splits large XML files into smaller, version-control–friendly pieces and reassembles them back into the original XML.

```bash
config-disassembler xml disassemble <path> [options]
config-disassembler xml reassemble  <path> [extension] [--postpurge]
config-disassembler xml parse       <path>
```

`<path>` may be either:

- a single XML file
- a directory containing XML files

---

# Basic usage

## Disassemble

```bash
config-disassembler xml disassemble flow.xml
```

Result:

```text
flow/
├── assignments/
├── decisions/
├── screens/
└── flow-meta.xml
```

## Reassemble

```bash
config-disassembler xml reassemble flow
```

---

# Disassemble options

| Option | Description | Default |
|---|---|---|
| `--unique-id-elements <list>` | Comma-separated fields used to derive filenames | none |
| `--strategy <name>` | `unique-id` or `grouped-by-tag` | `unique-id` |
| `--format <fmt>` | Output format: `xml`, `json`, `json5`, `yaml` | `xml` |
| `--prepurge` | Remove existing output before writing | `false` |
| `--postpurge` | Delete source after success | `false` |
| `--ignore-path <path>` | Ignore file path | `.cdignore` |
| `-p`, `--split-tags <spec>` | Split/group nested tags into subdirectories | none |
| `--multi-level <spec>` | Further disassemble matching files | none |

---

# Reassemble options

| Option | Description | Default |
|---|---|---|
| `<extension>` | Extension for rebuilt XML | `xml` |
| `--postpurge` | Delete disassembled directory after success | `false` |

---

# Disassembly strategies

## unique-id (default)

Each nested XML element is written to its own file using a unique identifier.

```bash
config-disassembler xml disassemble flow.xml \
  --unique-id-elements name,id
```

Best for:

- fine-grained diffs
- version control
- large metadata files

Example:

```text
flow/
├── decisions/
│   ├── CheckAccount.flow-meta.xml
│   └── ValidateUser.flow-meta.xml
├── screens/
└── flow-meta.xml
```

If no unique identifier is found, filenames fall back to an 8-character SHA-256 hash.

Example:

```text
419e0199.flow-meta.xml
```

## Compound unique IDs

Unique ID candidates may combine multiple fields using `+`.

Example:

```bash
config-disassembler xml disassemble profile.xml \
  --unique-id-elements \
  "actionName+pageOrSobjectType+formFactor+profile"
```

Resolved values are joined with `__`.

Example filename:

```text
View__Account__Large__Admin.profileActionOverrides-meta.xml
```

This is useful when no single field uniquely identifies sibling elements.

---

# Filename safety

Resolved filenames are automatically sanitized for cross-platform compatibility.

The disassembler:

- replaces path separators and reserved characters with `_`
- removes invalid trailing characters
- detects sibling filename collisions
- falls back to SHA-based filenames when collisions occur

This guarantees:

- deterministic output
- no silent overwrites
- identical layouts across Windows, macOS, and Linux

Example:

```text
TrustFile Transaction Sync/Import Complete
```

Becomes:

```text
TrustFile Transaction Sync_Import Complete.flow-meta.xml
```

---

## grouped-by-tag

Groups nested elements with the same tag into shared files.

```bash
config-disassembler xml disassemble flow.xml \
  --strategy grouped-by-tag
```

Best for:

- fewer files
- simpler layouts
- quick inspection

Example:

```text
flow/
├── assignments.flow-meta.xml
├── decisions.flow-meta.xml
├── screens.flow-meta.xml
└── flow-meta.xml
```

Reassembly preserves the original structure and ordering.

---

# Split tags

With `grouped-by-tag`, specific nested tags can be split or grouped into subdirectories.

Useful for:

- Salesforce permission sets
- large nested collections
- object-based metadata

Rule format:

```text
tag:mode:field
```

Or:

```text
tag:path:mode:field
```

Where:

- `mode=split` → one file per item
- `mode=group` → one file per grouped field value

Example:

```bash
config-disassembler xml disassemble permissionset.xml \
  --strategy grouped-by-tag \
  -p "objectPermissions:split:object,fieldPermissions:group:field"
```

Result:

```text
permissionset/
├── objectPermissions/
│   ├── Account.objectPermissions-meta.xml
│   └── Contact.objectPermissions-meta.xml
├── fieldPermissions/
└── permissionset-meta.xml
```

Reassembly automatically merges split directories back into the original XML.

---

# Multi-level disassembly

Multi-level disassembly further splits specific output files after the initial disassembly pass.

Useful for deeply nested metadata structures.

Rule format:

```text
file_pattern:root_to_strip:unique_id_elements
```

Example:

```bash
config-disassembler xml disassemble \
  Cloud_Kicks_Inner_Circle.loyaltyProgramSetup-meta.xml \
  --unique-id-elements "fullName,name,processName" \
  --multi-level \
  "programProcesses:programProcesses:parameterName,ruleName"
```

This:

1. disassembles the top-level XML
2. matches files containing `programProcesses`
3. unwraps the `programProcesses` root
4. disassembles nested items again using:
   - `parameterName`
   - `ruleName`

A `.multi_level.json` file is written automatically so reassembly can reconstruct the original hierarchy.

No additional flags are required during reassembly.

---

## Multiple multi-level rules

Multiple rules may be separated with `;`.

Example:

```bash
config-disassembler xml disassemble Sample.multi-meta.xml \
  --unique-id-elements "id,name,label" \
  --multi-level \
  "sectionA:sectionA:id;sectionB:sectionB:name"
```

Whitespace is trimmed automatically.

Empty trailing rules are ignored.

---

# Ignore files

Directory disassembly supports `.gitignore`-style filtering using `.cdignore`.

Example:

```text
**/generated/
**/secret.xml
```

Usage:

```bash
config-disassembler xml disassemble metadata/
```

For backward compatibility, `.xmldisassemblerignore` is also recognized when `.cdignore` is absent.

---

# Output formats

XML can be split into:

- XML
- JSON
- JSON5
- YAML

Examples:

```bash
config-disassembler xml disassemble flow.xml --format yaml
```

```bash
config-disassembler xml disassemble flow.xml --format json5
```

Regardless of split format, reassembly always produces XML.

---

# XML parser behavior

Parsing uses `quick-xml`.

Supported features include:

- CDATA preservation
- comment preservation
- attribute preservation

Attributes are represented using `@` prefixes.

Example:

```xml
<root version="1.0">
```

Becomes:

```json
{
  "@version": "1.0"
}
```

CDATA is represented using `#cdata`.

---

# Logging

Logging uses the `log` crate with `env_logger`.

Enable verbose output:

```bash
RUST_LOG=debug config-disassembler xml disassemble flow.xml
```

---

# Reassembly caveat

Multi-level reassembly removes intermediate directories during reconstruction, even without `--postpurge`.

This is necessary so higher-level reassembly can merge rebuilt XML files correctly.

Use version control if you need to preserve intermediate disassembly trees.

---

# Examples

See:

- [examples.md](examples.md)

For complete before/after layouts and real-world metadata examples.
