//! Text must come back escaped the way the source escaped it. The writer used to apply its own
//! escaping to everything, so a raw `'` came back as `&apos;` and `&#39;` as `&apos;`: the same
//! data, different bytes (seen in Salesforce translations, labels and quick actions).

use config_disassembler::xml::{
    DecomposeRule, DisassembleXmlFileHandler, ReassembleXmlFileHandler,
};
use std::path::Path;

const LABELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomLabels xmlns="http://soap.sforce.com/2006/04/metadata">
    <labels>
        <fullName>Raw_Apostrophe</fullName>
        <value>Select a date that's after the start date.</value>
    </labels>
    <labels>
        <fullName>Numeric_Reference</fullName>
        <value>&#39;New Program Engagement&#39;</value>
    </labels>
    <labels>
        <fullName>Canonical</fullName>
        <value>It&apos;s &quot;quoted&quot; &amp; &lt;tagged&gt;</value>
    </labels>
    <labels>
        <fullName>Raw_Quotes</fullName>
        <value>Say "hello" &gt; 1</value>
    </labels>
</CustomLabels>
"#;

async fn roundtrip(base: &Path, format: &str, grouped: bool) -> String {
    let source = base.join("CustomLabels.labels-meta.xml");
    std::fs::write(&source, LABELS).unwrap();
    let rules = [DecomposeRule {
        tag: "labels".to_string(),
        path_segment: "labels".to_string(),
        mode: "split".to_string(),
        field: "fullName".to_string(),
    }];
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            Some("fullName"),
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
            base.join("CustomLabels").to_str().unwrap(),
            Some("labels-meta.xml"),
            true,
            None,
        )
        .await
        .expect("reassemble");
    std::fs::read_to_string(&source).unwrap()
}

#[tokio::test]
async fn text_keeps_its_source_escaping_in_every_format_and_strategy() {
    for format in ["xml", "json", "json5", "yaml"] {
        for grouped in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let rebuilt = roundtrip(tmp.path(), format, grouped).await;
            assert_eq!(rebuilt, LABELS, "format {format} / grouped {grouped}");
        }
    }
}
