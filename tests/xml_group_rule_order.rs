//! Siblings a `group` decompose rule spreads across several files (e.g. `fieldPermissions`
//! grouped by object) must reassemble in their original order, even when they alternated
//! between groups in the source. Without `.group_order.json` (older versions, or edits that
//! drop it) each group file is read in turn, in file-name order.

use config_disassembler::xml::{
    DecomposeRule, DisassembleXmlFileHandler, ReassembleXmlFileHandler,
};
use std::path::{Path, PathBuf};

// `Log__c`, `Entry__c`, `Log__c`, `Log__c`: grouping by object writes `Entry__c` and
// `Log__c` files, and reading them back one after the other loses the interleaving.
const INTERLEAVED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PermissionSet xmlns="http://soap.sforce.com/2006/04/metadata">
    <fieldPermissions>
        <editable>false</editable>
        <field>Log__c.Message__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>false</editable>
        <field>Entry__c.Detail__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>false</editable>
        <field>Log__c.Status__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>false</editable>
        <field>Log__c.Count__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <label>Log Viewer</label>
</PermissionSet>
"#;

const CONTIGUOUS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PermissionSet xmlns="http://soap.sforce.com/2006/04/metadata">
    <fieldPermissions>
        <editable>false</editable>
        <field>Entry__c.Detail__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>false</editable>
        <field>Log__c.Message__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>false</editable>
        <field>Log__c.Status__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <label>Log Viewer</label>
</PermissionSet>
"#;

fn rules() -> [DecomposeRule; 1] {
    [DecomposeRule {
        tag: "fieldPermissions".to_string(),
        path_segment: "fieldPermissions".to_string(),
        mode: "group".to_string(),
        field: "field".to_string(),
    }]
}

async fn disassemble(source: &Path, format: &str) {
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("grouped-by-tag"),
            false,
            true,
            ".xmldisassemblerignore",
            format,
            None,
            Some(&rules()),
            None,
            None,
        )
        .await
        .expect("disassemble");
}

async fn reassemble(dir: &Path) {
    ReassembleXmlFileHandler::new()
        .reassemble(
            dir.to_str().unwrap(),
            Some("permissionset-meta.xml"),
            false,
            None,
        )
        .await
        .expect("reassemble");
}

/// Writes `content` as `Viewer.permissionset-meta.xml`, disassembles it (purging the
/// original), and returns (source path, disassembled dir).
async fn write_and_disassemble(base: &Path, content: &str, format: &str) -> (PathBuf, PathBuf) {
    let source = base.join("Viewer.permissionset-meta.xml");
    std::fs::write(&source, content).unwrap();
    disassemble(&source, format).await;
    assert!(!source.exists(), "post-purge should remove the original");
    (source, base.join("Viewer"))
}

fn field_order(xml: &str) -> Vec<&str> {
    xml.lines()
        .filter_map(|l| l.trim().strip_prefix("<field>")?.strip_suffix("</field>"))
        .collect()
}

fn group_file(dir: &Path, key: &str) -> PathBuf {
    dir.join("fieldPermissions")
        .join(format!("{key}.fieldPermissions-meta.xml"))
}

#[tokio::test]
async fn interleaved_groups_keep_original_order() {
    for format in ["xml", "yaml", "json", "json5"] {
        let tmp = tempfile::tempdir().unwrap();
        let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, format).await;
        assert!(dir.join(".group_order.json").exists(), "format {format}");

        reassemble(&dir).await;
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            INTERLEAVED,
            "format {format}"
        );
    }
}

#[tokio::test]
async fn groups_already_in_file_name_order_need_no_order_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), CONTIGUOUS, "xml").await;
    assert!(!dir.join(".group_order.json").exists());

    reassemble(&dir).await;
    assert_eq!(std::fs::read_to_string(&source).unwrap(), CONTIGUOUS);
}

#[tokio::test]
async fn contiguous_groups_out_of_file_name_order_are_recorded() {
    // `Zed__c` before `Log__c`: contiguous, but not the order the files are read in.
    let swapped = CONTIGUOUS.replace("Entry__c.Detail__c", "Zed__c.Detail__c");
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), &swapped, "xml").await;
    assert!(dir.join(".group_order.json").exists());
    reassemble(&dir).await;
    assert_eq!(std::fs::read_to_string(&source).unwrap(), swapped);
}

#[tokio::test]
async fn missing_order_file_reads_groups_in_file_name_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, "xml").await;
    std::fs::remove_file(dir.join(".group_order.json")).unwrap();

    reassemble(&dir).await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(
        field_order(&out),
        [
            "Entry__c.Detail__c",
            "Log__c.Message__c",
            "Log__c.Status__c",
            "Log__c.Count__c"
        ]
    );
}

#[tokio::test]
async fn entries_and_groups_added_after_disassembly_follow_the_recorded_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, "xml").await;

    let entry_file = group_file(&dir, "Entry__c");
    let entry = std::fs::read_to_string(&entry_file).unwrap().replace(
        "    </fieldPermissions>\n</PermissionSet>",
        "    </fieldPermissions>\n    <fieldPermissions>\n        <editable>true</editable>\n        <field>Entry__c.Added__c</field>\n        <readable>true</readable>\n    </fieldPermissions>\n</PermissionSet>",
    );
    std::fs::write(&entry_file, entry).unwrap();
    std::fs::write(
        group_file(&dir, "Account"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<PermissionSet xmlns="http://soap.sforce.com/2006/04/metadata">
    <fieldPermissions>
        <editable>true</editable>
        <field>Account.Name</field>
        <readable>true</readable>
    </fieldPermissions>
</PermissionSet>
"#,
    )
    .unwrap();

    reassemble(&dir).await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(
        field_order(&out),
        [
            "Log__c.Message__c",
            "Entry__c.Detail__c",
            "Log__c.Status__c",
            "Log__c.Count__c",
            "Account.Name",
            "Entry__c.Added__c"
        ]
    );
}

#[tokio::test]
async fn removed_groups_and_entries_are_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, "xml").await;
    std::fs::remove_file(group_file(&dir, "Entry__c")).unwrap();
    let log_file = group_file(&dir, "Log__c");
    let log = std::fs::read_to_string(&log_file).unwrap().replace(
        "    <fieldPermissions>\n        <editable>false</editable>\n        <field>Log__c.Status__c</field>\n        <readable>true</readable>\n    </fieldPermissions>\n",
        "",
    );
    std::fs::write(&log_file, log).unwrap();

    reassemble(&dir).await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(field_order(&out), ["Log__c.Message__c", "Log__c.Count__c"]);
}

/// Group directories holding anything but plain group files keep the file-by-file merge.
#[tokio::test]
async fn unexpected_group_directory_contents_fall_back_to_file_name_order() {
    let grouped = [
        "Entry__c.Detail__c",
        "Log__c.Message__c",
        "Log__c.Status__c",
        "Log__c.Count__c",
    ];

    // A subdirectory.
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, "xml").await;
    std::fs::create_dir(dir.join("fieldPermissions").join("extra")).unwrap();
    reassemble(&dir).await;
    assert_eq!(
        field_order(&std::fs::read_to_string(&source).unwrap()),
        grouped
    );

    // A group file whose root holds another element.
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, "xml").await;
    let entry_file = group_file(&dir, "Entry__c");
    let entry = std::fs::read_to_string(&entry_file).unwrap().replace(
        "</PermissionSet>",
        "    <description>Hand edit</description>\n</PermissionSet>",
    );
    std::fs::write(&entry_file, entry).unwrap();
    reassemble(&dir).await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert_eq!(field_order(&out), grouped);
    assert!(out.contains("<description>Hand edit</description>"));

    // A group file that doesn't parse.
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, "xml").await;
    std::fs::write(group_file(&dir, "Bad"), "<PermissionSet>").unwrap();
    reassemble(&dir).await;
    assert_eq!(
        field_order(&std::fs::read_to_string(&source).unwrap()),
        grouped
    );
}

#[tokio::test]
async fn redisassembling_contiguous_groups_removes_a_stale_order_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), INTERLEAVED, "xml").await;
    assert!(dir.join(".group_order.json").exists());
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::write(&source, CONTIGUOUS).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".group_order.json"), "{}").unwrap();

    disassemble(&source, "xml").await;
    assert!(!dir.join(".group_order.json").exists());
}
