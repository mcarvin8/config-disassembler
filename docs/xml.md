# XML Support

The `xml` subcommand splits large XML files into smaller pieces and reassembles them back into the original XML.

```bash
config-disassembler xml disassemble <path> [options]
config-disassembler xml reassemble  <path> [extension] [--postpurge]
config-disassembler xml parse       <path>
```

`<path>` may be a single XML file or a directory containing XML files.

---

## Basic usage

### Disassemble

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

### Reassemble

```bash
config-disassembler xml reassemble flow
```

### Parse

`parse` inspects how the disassembler reads an XML file without writing any output. Use it to preview the internal representation before choosing a strategy or to diagnose unexpected splits.

```bash
config-disassembler xml parse flow.xml
```

Prints the parsed structure to stdout. Useful for verifying attribute and CDATA handling before committing to a disassembly run.

---

## Disassemble options

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

## Reassemble options

| Option | Description | Default |
|---|---|---|
| `<extension>` | Extension for rebuilt XML | `xml` |
| `--postpurge` | Delete disassembled directory after success | `false` |

---

## Disassembly strategies

### unique-id (default)

Each nested XML element is written to its own file using a unique identifier.

```bash
config-disassembler xml disassemble flow.xml \
  --unique-id-elements name,id
```

Best for fine-grained diffs, version control, and large metadata files.

Example:

```text
flow/
├── decisions/
│   ├── CheckAccount.flow-meta.xml
│   └── ValidateUser.flow-meta.xml
├── screens/
└── flow-meta.xml
```

If no unique identifier is found, filenames fall back to an 8-character SHA-256 hash:

```text
419e0199.flow-meta.xml
```

#### Compound unique IDs

Combine multiple fields with `+`:

```bash
config-disassembler xml disassemble profile.xml \
  --unique-id-elements \
  "actionName+pageOrSobjectType+formFactor+profile"
```

Resolved values are joined with `__`:

```text
View__Account__Large__Admin.profileActionOverrides-meta.xml
```

Useful when no single field uniquely identifies sibling elements.

---

### grouped-by-tag

Groups nested elements with the same tag into shared files.

```bash
config-disassembler xml disassemble flow.xml \
  --strategy grouped-by-tag
```

Best for fewer files, simpler layouts, and quick inspection.

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

## Split tags

With `grouped-by-tag`, specific nested tags can be split or grouped into subdirectories.

Rule format:

```text
tag:mode:field
```

Or with a path:

```text
tag:path:mode:field
```

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

## Output formats

XML can be split into:

- XML
- JSON
- JSON5
- YAML

```bash
config-disassembler xml disassemble flow.xml --format yaml
config-disassembler xml disassemble flow.xml --format json5
```

Regardless of split format, reassembly always produces XML.

---

## XML parser behavior

Parsing uses `quick-xml`. Supported features:

- CDATA preservation
- comment preservation
- attribute preservation

Attributes use `@` prefixes:

```xml
<root version="1.0">
```

Becomes:

```json
{ "@version": "1.0" }
```

CDATA is represented using `#cdata`.

---

## Filename safety

Resolved filenames are automatically sanitized for cross-platform compatibility:

- Path separators and reserved characters are replaced with `_`
- Invalid trailing characters are removed
- Sibling filename collisions are detected; SHA-based fallback is used when they occur

Guarantees deterministic, collision-free output across Windows, macOS, and Linux.

Example:

```text
TrustFile Transaction Sync/Import Complete
```

Becomes:

```text
TrustFile Transaction Sync_Import Complete.flow-meta.xml
```

---

## Ignore files

Directory disassembly supports `.gitignore`-style filtering via `.cdignore`:

```text
**/generated/
**/secret.xml
```

```bash
config-disassembler xml disassemble metadata/
```

For backward compatibility, `.xmldisassemblerignore` is also recognized when `.cdignore` is absent.

---

## Logging

```bash
RUST_LOG=debug config-disassembler xml disassemble flow.xml
```

---

## Multi-level disassembly

Multi-level disassembly further splits specific output files after the initial disassembly pass. Useful for deeply nested metadata structures.

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

1. Disassembles the top-level XML
2. Matches files containing `programProcesses`
3. Unwraps the `programProcesses` root
4. Disassembles nested items again using `parameterName` and `ruleName`

A `.multi_level.json` file is written automatically so reassembly can reconstruct the original hierarchy. No additional flags are required during reassembly.

### Multiple rules

Separate rules with `;`:

```bash
config-disassembler xml disassemble Sample.multi-meta.xml \
  --unique-id-elements "id,name,label" \
  --multi-level \
  "sectionA:sectionA:id;sectionB:sectionB:name"
```

Whitespace is trimmed. Empty trailing rules are ignored.

### Reassembly caveat

Multi-level reassembly removes intermediate directories during reconstruction, even without `--postpurge`. This is necessary so higher-level reassembly can merge rebuilt XML files correctly.

Use version control if you need to preserve intermediate disassembly trees.

---

## Sidecar elements

`--sidecar-elements` extracts the text content of named XML elements into companion files during disassembly and reinjects them during reassembly. Useful for metadata types that embed large non-XML blobs (OpenAPI schemas, WSDL, etc.) inside an XML element.

### Format

```text
element:extension[,element:extension,...]
```

Each pair names the XML element to extract and the file extension for the companion file. Multiple pairs are separated by commas.

### Disassemble

```bash
config-disassembler xml disassemble \
  BankService.externalServiceRegistration-meta.xml \
  --sidecar-elements schema:yaml
```

Result:

```text
BankService/
├── BankService.yaml       ← extracted <schema> content
├── .sidecars.json         ← auto-detect metadata for reassembly
├── .key_order.json
└── (disassembled shards)
```

- The element is removed from the disassembled XML shards.
- The companion file is named `{directory}.{extension}` and written inside the disassembled directory.
- `.sidecars.json` is written automatically so reassembly can locate and reinject sidecar files without any additional flags.

### Format conversion

The extracted text is converted to match the declared extension:

| Extension | YAML source | JSON source |
|---|---|---|
| `yaml` / `yml` | passes through unchanged | converted to YAML |
| `json` | converted to pretty JSON | prettified |
| anything else | passes through unchanged | passes through unchanged |

This mirrors the Salesforce `decomposeExternalServiceRegistrationBeta` preset: JSON schemas are always stored as YAML when `extension` is `yaml`.

### Reassemble

```bash
config-disassembler xml reassemble BankService
```

Reassembly auto-detects sidecars from `.sidecars.json` written during disassembly — no flag required. The sidecar file content is injected back verbatim into the XML element.

### Multiple sidecars

```bash
config-disassembler xml disassemble service.xml \
  --sidecar-elements "schema:yaml,wsdl:xml"
```

Each spec produces its own companion file.

---

## Examples

See [examples.md](examples.md) for complete before/after layouts and real-world metadata examples.
