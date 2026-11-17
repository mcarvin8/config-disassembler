//! Identical repeated siblings must all survive disassemble + reassemble. Under unique-id they
//! used to resolve to the same shard filename (same content -> same hash), so each overwrote the
//! last and only one came back -- e.g. Salesforce object translations with repeated
//! `<caseValues>`.

use config_disassembler::xml::{DisassembleXmlFileHandler, ReassembleXmlFileHandler};
use std::path::Path;

const TRANSLATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomObjectTranslation xmlns="http://soap.sforce.com/2006/04/metadata">
    <startsWith>Consonant</startsWith>
    <caseValues>
        <plural>false</plural>
        <value>Fields to Exclude from Sync</value>
    </caseValues>
    <caseValues>
        <plural>false</plural>
        <value>Fields to Exclude from Sync</value>
    </caseValues>
    <caseValues>
        <plural>true</plural>
        <value>Fields to Exclude from Sync</value>
    </caseValues>
    <caseValues>
        <plural>false</plural>
        <value>Fields to Exclude from Sync</value>
    </caseValues>
</CustomObjectTranslation>
"#;

async fn roundtrip(base: &Path, format: &str, strategy: &str) -> String {
    let source = base.join("Sync__c-de.objectTranslation-meta.xml");
    std::fs::write(&source, TRANSLATION).unwrap();
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some(strategy),
            false,
            true,
            ".xmldisassemblerignore",
            format,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(!source.exists(), "post-purge should remove the original");
    ReassembleXmlFileHandler::new()
        .reassemble(
            base.join("Sync__c-de").to_str().unwrap(),
            Some("objectTranslation-meta.xml"),
            true,
            None,
        )
        .await
        .expect("reassemble");
    std::fs::read_to_string(&source).unwrap()
}

#[tokio::test]
async fn identical_siblings_survive_roundtrip_in_every_format_and_strategy() {
    for format in ["xml", "json", "json5", "yaml"] {
        for strategy in ["unique-id", "grouped-by-tag"] {
            let tmp = tempfile::tempdir().unwrap();
            let rebuilt = roundtrip(tmp.path(), format, strategy).await;
            assert_eq!(
                rebuilt, TRANSLATION,
                "format {format} / strategy {strategy}"
            );
        }
    }
}
