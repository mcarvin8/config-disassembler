//! Empty elements among repeated siblings must survive disassemble + reassemble. They used
//! to be dropped at parse time, which silently changed Salesforce page layouts: empty
//! `<layoutColumns/>` entries set a section's column count.

use config_disassembler::xml::{
    DecomposeRule, DisassembleXmlFileHandler, ReassembleXmlFileHandler,
};
use std::path::Path;

/// Empty elements written in the form the reassembler emits (`<a></a>`), so the round trip
/// can be compared byte for byte.
const LAYOUT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Layout xmlns="http://soap.sforce.com/2006/04/metadata">
    <layoutSections>
        <label>Information</label>
        <layoutColumns>
            <layoutItems>
                <field>Name</field>
            </layoutItems>
        </layoutColumns>
        <layoutColumns></layoutColumns>
        <style>TwoColumnsTopToBottom</style>
    </layoutSections>
    <layoutSections>
        <label>Custom Links</label>
        <layoutColumns></layoutColumns>
        <layoutColumns></layoutColumns>
        <layoutColumns></layoutColumns>
        <style>CustomLinks</style>
    </layoutSections>
    <layoutSections>
        <label>Single</label>
        <layoutColumns></layoutColumns>
        <style>OneColumn</style>
    </layoutSections>
</Layout>
"#;

async fn roundtrip(base: &Path, content: &str, format: &str, grouped: bool) -> String {
    let source = base.join("Acct.layout-meta.xml");
    std::fs::write(&source, content).unwrap();
    let rules = [DecomposeRule {
        tag: "layoutSections".to_string(),
        path_segment: "layoutSections".to_string(),
        mode: "split".to_string(),
        field: "label".to_string(),
    }];
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            None,
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
            base.join("Acct").to_str().unwrap(),
            Some("layout-meta.xml"),
            true,
            None,
        )
        .await
        .expect("reassemble");
    std::fs::read_to_string(&source).unwrap()
}

#[tokio::test]
async fn empty_repeated_siblings_survive_roundtrip_in_every_format_and_strategy() {
    for format in ["xml", "json", "json5", "yaml"] {
        for grouped in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let rebuilt = roundtrip(tmp.path(), LAYOUT, format, grouped).await;
            assert_eq!(rebuilt, LAYOUT, "format {format} / grouped {grouped}");
        }
    }
}

#[tokio::test]
async fn every_empty_element_form_survives_roundtrip() {
    // Salesforce's `<a/>`, Prettier's `<a />` and `<a></a>` each come back as written.
    for form in ["<layoutColumns/>", "<layoutColumns />"] {
        let original = LAYOUT.replace("<layoutColumns></layoutColumns>", form);
        for format in ["xml", "json", "json5", "yaml"] {
            for grouped in [false, true] {
                let tmp = tempfile::tempdir().unwrap();
                let rebuilt = roundtrip(tmp.path(), &original, format, grouped).await;
                assert_eq!(
                    rebuilt, original,
                    "form {form} / format {format} / grouped {grouped}"
                );
            }
        }
    }
}
