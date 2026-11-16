//! The XML declaration must survive disassemble + reassemble byte for byte, including
//! non-canonical forms such as Prettier's `encoding="UTF-8" ?>` and single quotes.

use config_disassembler::xml::{
    DecomposeRule, DisassembleXmlFileHandler, ReassembleXmlFileHandler,
};
use std::path::Path;

const BODY: &str = r#"
<PermissionSet xmlns="http://soap.sforce.com/2006/04/metadata">
    <fieldPermissions>
        <editable>true</editable>
        <field>Account.Zeta__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <fieldPermissions>
        <editable>false</editable>
        <field>Account.Alpha__c</field>
        <readable>true</readable>
    </fieldPermissions>
    <label>Declaration Test</label>
</PermissionSet>
"#;

const DECLARATIONS: &[&str] = &[
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    r#"<?xml version="1.0" encoding="UTF-8" ?>"#,
    r#"<?xml version='1.0' encoding='UTF-8'?>"#,
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>"#,
];

async fn roundtrip(base: &Path, content: &str, format: &str, grouped: bool) -> String {
    let source = base.join("Decl.permissionset-meta.xml");
    std::fs::write(&source, content).unwrap();
    let rules = [DecomposeRule {
        tag: "fieldPermissions".to_string(),
        path_segment: "fieldPermissions".to_string(),
        mode: "split".to_string(),
        field: "field".to_string(),
    }];
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            Some("field"),
            Some(if grouped {
                "grouped-by-tag"
            } else {
                "unique-id"
            }),
            false,
            true,
            ".xmldisassemblerignore",
            format,
            None,
            grouped.then_some(&rules[..]),
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(!source.exists(), "post-purge should remove the original");
    ReassembleXmlFileHandler::new()
        .reassemble(
            base.join("Decl").to_str().unwrap(),
            Some("permissionset-meta.xml"),
            true,
            None,
        )
        .await
        .expect("reassemble");
    std::fs::read_to_string(&source).unwrap()
}

#[tokio::test]
async fn declaration_survives_roundtrip_in_every_format_and_strategy() {
    for decl in DECLARATIONS {
        let original = format!("{decl}{BODY}");
        for format in ["xml", "json", "json5", "yaml"] {
            for grouped in [false, true] {
                let tmp = tempfile::tempdir().unwrap();
                let rebuilt = roundtrip(tmp.path(), &original, format, grouped).await;
                assert_eq!(
                    rebuilt, original,
                    "declaration {decl} / format {format} / grouped {grouped}"
                );
            }
        }
    }
}
