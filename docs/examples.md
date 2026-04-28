# Examples

This page shows common disassembly layouts using small file-tree diagrams. The
examples focus on how files are split on disk and how to reassemble them.

## JSON Object

Input `config.json`:

```json
{
  "name": "demo",
  "enabled": true,
  "settings": {
    "retry": 3,
    "timeout_ms": 5000
  },
  "tags": ["alpha", "beta"]
}
```

Disassemble:

```bash
config-disassembler json disassemble config.json
```

Output:

```text
config/
├── .config-disassembler.json
├── _main.json
├── settings.json
└── tags.json
```

`_main.json` contains scalar top-level keys:

```json
{
  "name": "demo",
  "enabled": true
}
```

`settings.json` contains the nested object:

```json
{
  "retry": 3,
  "timeout_ms": 5000
}
```

Reassemble:

```bash
config-disassembler json reassemble config
```

## Array Root With Unique IDs

Input `items.yaml`:

```yaml
- name: alpha
  weight: 1
- name: beta
  weight: 2
- name: gamma
  weight: 3
```

Disassemble and name each file from the `name` field:

```bash
config-disassembler yaml disassemble items.yaml --unique-id name
```

Output:

```text
items/
├── .config-disassembler.json
├── alpha.yaml
├── beta.yaml
└── gamma.yaml
```

`alpha.yaml`:

```yaml
name: alpha
weight: 1
```

Reassemble:

```bash
config-disassembler yaml reassemble items
```

## Cross-Format Split Files

JSON, JSON5, JSONC, YAML, and TOON can be split into any format in that family
and reassembled back into any other format in that family.

Disassemble JSON into YAML split files:

```bash
config-disassembler json disassemble config.json --output-format yaml
```

Output:

```text
config/
├── .config-disassembler.json
├── _main.yaml
├── settings.yaml
└── tags.yaml
```

Reassemble the YAML split files back to JSON:

```bash
config-disassembler json reassemble config --output-format json
```

## JSONC Preserving Comments

When JSONC is disassembled to JSONC and reassembled as JSONC, comments and
trailing commas are preserved.

Input `config.jsonc`:

```jsonc
{
  // Scalars stay in _main.jsonc.
  "name": "demo",
  "settings": {
    "retry": 3,
    "features": [
      "comments",
      "trailing-commas",
    ],
  },
}
```

Disassemble:

```bash
config-disassembler jsonc disassemble config.jsonc
```

Output:

```text
config/
├── .config-disassembler.json
├── _main.jsonc
└── settings.jsonc
```

`_main.jsonc`:

```jsonc
{
  // Scalars stay in _main.jsonc.
  "name": "demo",
}
```

`settings.jsonc`:

```jsonc
{
    "retry": 3,
    "features": [
      "comments",
      "trailing-commas",
    ],
  }
```

Reassemble:

```bash
config-disassembler jsonc reassemble config
```

Cross-format JSONC conversions preserve parsed values, but JSONC-specific syntax
such as comments and trailing commas cannot be carried through formats like YAML
or JSON.

## TOML Is Same-Format Only

TOML can only be disassembled to TOML and reassembled to TOML.

Input `config.toml`:

```toml
title = "Demo"
enabled = true

[database]
server = "localhost"
ports = [8001, 8002]
```

Disassemble:

```bash
config-disassembler toml disassemble config.toml
```

Output:

```text
config/
├── .config-disassembler.json
├── _main.toml
└── database.toml
```

Reassemble:

```bash
config-disassembler toml reassemble config
```

Cross-format TOML conversion is rejected because TOML cannot represent all
values from the JSON-style family, such as `null` values or array document roots.

## Directory Input With `.cdignore`

Every `disassemble` action can process a directory. Matching config files are
disassembled in place, and each file gets a sibling directory named after its
stem.

Input tree:

```text
configs/
├── .cdignore
├── app.json
├── generated/
│   └── ignored.json
└── service.json
```

`.cdignore`:

```text
generated/
```

Disassemble all JSON files not ignored by `.cdignore`:

```bash
config-disassembler json disassemble configs
```

Output:

```text
configs/
├── .cdignore
├── app.json
├── app/
│   ├── .config-disassembler.json
│   └── ...
├── generated/
│   └── ignored.json
├── service.json
└── service/
    ├── .config-disassembler.json
    └── ...
```

## XML Unique-ID Strategy

XML uses the `xml` subcommand and can split nested elements into XML, JSON,
JSON5, or YAML files. The default strategy writes one file per nested element
using a unique identifier when possible.

```bash
config-disassembler xml disassemble metadata.xml --unique-id-elements name,id
```

Example output:

```text
metadata/
├── Account.objectPermissions-meta.xml
├── Contact.objectPermissions-meta.xml
└── metadata-meta.xml
```

Reassemble:

```bash
config-disassembler xml reassemble metadata
```

## XML Grouped-By-Tag Strategy

Use `grouped-by-tag` when you want fewer files and one file per repeated tag.

```bash
config-disassembler xml disassemble metadata.xml --strategy grouped-by-tag --format yaml
```

Example output:

```text
metadata/
├── fieldPermissions.yaml
├── objectPermissions.yaml
└── metadata.yaml
```

Reassemble:

```bash
config-disassembler xml reassemble metadata
```
