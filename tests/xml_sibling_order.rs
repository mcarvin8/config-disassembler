//! Repeated sibling elements must reassemble in their original document order,
//! not in the filename order of their shard files (issue #176). Directories
//! written without `.element_order.json` (older versions) keep filename order.

use config_disassembler::xml::{
    DecomposeRule, DisassembleXmlFileHandler, ReassembleXmlFileHandler,
};
use std::path::Path;

const PERMSET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PermissionSet xmlns="http://soap.sforce.com/2006/04/metadata">
    <fieldPermissions>
        <editable>true</editable>
        <field>Account.Zeta__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>true</editable>
        <field>Account.Alpha__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>false</editable>
        <field>Account.Mid__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <label>Order Test</label>
</PermissionSet>
"#;

// No unique-id field resolves for <keywords>, so shards get content-hash names.
const KEYWORDS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<KeywordList xmlns="http://soap.sforce.com/2006/04/metadata">
    <description>Blocked words</description>
    <keywords>
        <keyword>foo</keyword>
    </keywords>
    <keywords>
        <keyword>bar</keyword>
    </keywords>
    <keywords>
        <keyword>baz</keyword>
    </keywords>
    <masterLabel>Bad Words</masterLabel>
</KeywordList>
"#;

async fn disassemble(
    source: &Path,
    unique_id: Option<&str>,
    format: &str,
    rules: Option<&[DecomposeRule]>,
) {
    let strategy = if rules.is_some() {
        "grouped-by-tag"
    } else {
        "unique-id"
    };
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            unique_id,
            Some(strategy),
            false,
            true,
            ".xmldisassemblerignore",
            format,
            None,
            rules,
            None,
            None,
        )
        .await
        .expect("disassemble");
}

async fn reassemble(dir: &Path, extension: &str) {
    ReassembleXmlFileHandler::new()
        .reassemble(dir.to_str().unwrap(), Some(extension), false, None)
        .await
        .expect("reassemble");
}

/// Writes `content` as `<name>.<suffix>-meta.xml`, disassembles it (purging the
/// original), and returns (source path, disassembled dir).
async fn write_and_disassemble(
    base: &Path,
    name: &str,
    suffix: &str,
    content: &str,
    unique_id: Option<&str>,
    format: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let source = base.join(format!("{name}.{suffix}-meta.xml"));
    std::fs::write(&source, content).unwrap();
    disassemble(&source, unique_id, format, None).await;
    assert!(!source.exists(), "post-purge should remove the original");
    (source, base.join(name))
}

fn field_order(xml: &str) -> Vec<&str> {
    xml.lines()
        .filter_map(|l| l.trim().strip_prefix("<field>")?.strip_suffix("</field>"))
        .collect()
}

#[tokio::test]
async fn unique_id_named_siblings_keep_original_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(
        tmp.path(),
        "Order",
        "permissionset",
        PERMSET,
        Some("field"),
        "xml",
    )
    .await;
    assert!(dir.join(".element_order.json").exists());

    reassemble(&dir, "permissionset-meta.xml").await;
    assert_eq!(std::fs::read_to_string(&source).unwrap(), PERMSET);
}

#[tokio::test]
async fn hash_named_siblings_keep_original_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) =
        write_and_disassemble(tmp.path(), "Bad_Words", "keywords", KEYWORDS, None, "xml").await;

    reassemble(&dir, "keywords-meta.xml").await;
    assert_eq!(std::fs::read_to_string(&source).unwrap(), KEYWORDS);
}

#[tokio::test]
async fn non_xml_formats_keep_original_order() {
    for format in ["yaml", "json", "json5"] {
        let tmp = tempfile::tempdir().unwrap();
        let (source, dir) = write_and_disassemble(
            tmp.path(),
            "Order",
            "permissionset",
            PERMSET,
            Some("field"),
            format,
        )
        .await;
        reassemble(&dir, "permissionset-meta.xml").await;
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            PERMSET,
            "format {format}"
        );
    }
}

#[tokio::test]
async fn missing_order_file_falls_back_to_filename_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(
        tmp.path(),
        "Order",
        "permissionset",
        PERMSET,
        Some("field"),
        "xml",
    )
    .await;
    // A directory written by an older version has no element order file.
    std::fs::remove_file(dir.join(".element_order.json")).unwrap();

    reassemble(&dir, "permissionset-meta.xml").await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(
        field_order(&out),
        ["Account.Alpha__c", "Account.Mid__c", "Account.Zeta__c"]
    );
}

#[tokio::test]
async fn unparseable_order_file_falls_back_to_filename_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(
        tmp.path(),
        "Order",
        "permissionset",
        PERMSET,
        Some("field"),
        "xml",
    )
    .await;
    std::fs::write(dir.join(".element_order.json"), "not json").unwrap();

    reassemble(&dir, "permissionset-meta.xml").await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(
        field_order(&out),
        ["Account.Alpha__c", "Account.Mid__c", "Account.Zeta__c"]
    );
}

#[tokio::test]
async fn fragments_missing_from_the_order_file_follow_the_known_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(
        tmp.path(),
        "Order",
        "permissionset",
        PERMSET,
        Some("field"),
        "xml",
    )
    .await;
    // A fragment added by hand after disassembly isn't in the order file.
    std::fs::write(
        dir.join("fieldPermissions")
            .join("Account.Added__c.fieldPermissions-meta.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<PermissionSet xmlns="http://soap.sforce.com/2006/04/metadata">
    <fieldPermissions>
        <editable>true</editable>
        <field>Account.Added__c</field>
        <readable>true</readable>
    </fieldPermissions>
</PermissionSet>
"#,
    )
    .unwrap();

    reassemble(&dir, "permissionset-meta.xml").await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(
        field_order(&out),
        [
            "Account.Zeta__c",
            "Account.Alpha__c",
            "Account.Mid__c",
            "Account.Added__c"
        ]
    );
}

#[tokio::test]
async fn deleted_fragments_listed_in_the_order_file_are_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(
        tmp.path(),
        "Order",
        "permissionset",
        PERMSET,
        Some("field"),
        "xml",
    )
    .await;
    std::fs::remove_file(
        dir.join("fieldPermissions")
            .join("Account.Alpha__c.fieldPermissions-meta.xml"),
    )
    .unwrap();

    reassemble(&dir, "permissionset-meta.xml").await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(field_order(&out), ["Account.Zeta__c", "Account.Mid__c"]);
}

#[tokio::test]
async fn split_decompose_rule_keeps_original_order() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Order.permissionset-meta.xml");
    std::fs::write(&source, PERMSET).unwrap();
    let rules = [DecomposeRule {
        tag: "fieldPermissions".to_string(),
        path_segment: "fieldPermissions".to_string(),
        mode: "split".to_string(),
        field: "field".to_string(),
    }];
    disassemble(&source, None, "xml", Some(&rules)).await;
    let dir = tmp.path().join("Order");
    assert!(dir.join(".element_order.json").exists());

    reassemble(&dir, "permissionset-meta.xml").await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(
        field_order(&out),
        ["Account.Zeta__c", "Account.Alpha__c", "Account.Mid__c"]
    );
}

#[tokio::test]
async fn redisassembling_without_per_element_files_removes_a_stale_order_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(
        tmp.path(),
        "Order",
        "permissionset",
        PERMSET,
        Some("field"),
        "xml",
    )
    .await;
    reassemble(&dir, "permissionset-meta.xml").await;
    // grouped-by-tag without rules writes one file per tag: no per-element order to record.
    disassemble(&source, None, "xml", Some(&[])).await;
    assert!(!dir.join(".element_order.json").exists());
}
