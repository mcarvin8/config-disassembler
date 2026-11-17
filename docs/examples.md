# Examples

This page shows common disassembly layouts using small file-tree diagrams. The
examples focus on how files are split on disk and how to reassemble them.

For every option, see [xml.md](xml.md).

## Unique-ID Strategy

The default strategy writes one file per nested element, named from a unique
identifier when one is found.

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

## Grouped-By-Tag Strategy

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

## Directory Input With `.cdignore`

`disassemble` also accepts a directory. Every XML file beneath it is
disassembled in place, and each file gets a sibling directory named after its
stem.

Input tree:

```text
permissionsets/
├── .cdignore
├── HR_Admin.permissionset-meta.xml
├── Legacy_Admin.permissionset-meta.xml
└── Sales_Ops.permissionset-meta.xml
```

`.cdignore`:

```text
Legacy_*.permissionset-meta.xml
```

Disassemble every file not ignored by `.cdignore`:

```bash
config-disassembler xml disassemble permissionsets
```

Output:

```text
permissionsets/
├── .cdignore
├── HR_Admin/
│   └── ...
├── HR_Admin.permissionset-meta.xml
├── Legacy_Admin.permissionset-meta.xml
├── Sales_Ops/
│   └── ...
└── Sales_Ops.permissionset-meta.xml
```

Pass `--postpurge` to delete each source file after it is split.
