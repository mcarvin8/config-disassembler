# XML Support

The `xml` subcommand splits large XML files into smaller pieces and reassembles them back into the original XML.

```bash
config-disassembler xml disassemble <path> [options]
config-disassembler xml reassemble  <path> [extension] [--postpurge]
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

#### Sibling order

Filenames are chosen for stability, not order: unique-id names sort alphabetically and hash names sort arbitrarily. So that reassembly restores the original document order of repeated siblings, disassembly also writes `.element_order.json`, listing each tag's shard files in source order:

```json
{
  "decisions": [
    "decisions/ValidateUser.flow-meta.xml",
    "decisions/CheckAccount.flow-meta.xml"
  ]
}
```

This matters wherever sibling order carries meaning (picklist values, page-layout sections, approval steps). It is also written for grouped-by-tag `split` rules, and per level for multi-level disassembly.

On reassembly, listed shards come first in their recorded order. Files the list doesn't mention (for example a fragment added by hand) follow in filename order, and listed files that no longer exist are skipped. A directory without `.element_order.json`, such as one written by an earlier version, reassembles in filename order as before. Edit or delete the file to change the order deliberately.

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

`split` files keep their source order through `.element_order.json` (see [Sibling order](#sibling-order)). A `group` file keeps its own items in order, but group files are read back one after another, in filename order. When that would change the source order (siblings alternating between groups, such as `fieldPermissions` for `Log__c`, then `Entry__c`, then `Log__c` again), disassembly also writes `.group_order.json`, recording each group directory's sequence as runs of consecutive items from the same file:

```json
{
  "fieldPermissions": {
    "tag": "fieldPermissions",
    "runs": [["Log__c", 1], ["Entry__c", 1], ["Log__c", 2]]
  }
}
```

Reassembly takes items from each group file in that sequence. Items or group files added after disassembly follow in filename order, and removed ones are skipped. Without `.group_order.json` (an earlier version, or groups already in filename order), group files are read one after another as before.

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
├── .element_order.json    ← original order of repeated siblings
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

Reassembly auto-detects sidecars from `.sidecars.json` written during disassembly — no flag required. The sidecar content is converted back to the format it had inside the XML and injected into the element.

So that it comes back as the exact text it was, disassembly also records the form that reproduces it: compact or pretty JSON (`json_layout`), and whether quotes are written as `&quot;` / `&apos;` (`escape`). Salesforce, for example, retrieves an External Service schema as compact JSON with `&quot;`. Disassembly checks the recorded form by converting the sidecar back. When no form reproduces the text exactly (a number like `1.50` comes back as `1.5`, or the element has attributes or comments), the element stays in the XML instead of going to a sidecar, with a warning. Sidecars written by earlier versions, without these fields, come back as pretty JSON with raw quotes, as before.

### Multiple sidecars

```bash
config-disassembler xml disassemble service.xml \
  --sidecar-elements "schema:yaml,wsdl:xml"
```

Each spec produces its own companion file.

---

---

## Round-trip verification (library API)

`verify_roundtrip` disassembles and reassembles an XML file inside an isolated temp directory, then reports whether the reconstructed file matches the original. It never touches the caller's file. This is a library-only API — no CLI subcommand yet.

```rust
use config_disassembler::xml::{verify_roundtrip, RoundtripStatus, VerifyOptions};

let status = verify_roundtrip("flow.xml", VerifyOptions::default()).await?;
match status {
    RoundtripStatus::Identical => println!("byte-identical round trip"),
    RoundtripStatus::Reordered => println!("semantically equal; only sibling/attribute order changed"),
    RoundtripStatus::Drift(reason) => println!("genuine content loss: {reason}"),
}
```

`VerifyOptions` mirrors the disassemble options that affect structure (`unique_id_elements`, `strategy`, `ignore_path`, `multi_level_rules`, `decompose_rules`, `sidecar_specs`) — set the same values you'd pass to disassembly so the round trip exercises the same rules.

Comparison ignores sibling/attribute order (`Reordered`) since element order is not guaranteed to survive a disassemble/reassemble cycle, but reports `Drift` when content is genuinely lost or changed. Useful for CI dry-run checks: verify a metadata file round-trips cleanly without writing anything to the working tree.

---

## Examples

See [examples.md](examples.md) for complete before/after layouts and real-world metadata examples.
