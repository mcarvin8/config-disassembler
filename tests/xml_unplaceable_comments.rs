//! Files with comments disassembly can't keep in place must be left alone, even with post-purge.
//! They used to be disassembled: a comment between elements moved after the children, and one in
//! the root was written back as JSON text (`["\n    "]<!--["..."]-->`), corrupting the file.

use config_disassembler::xml::DisassembleXmlFileHandler;
use std::path::Path;

/// Shapes from SalesforceFoundation/NPSP.
const FILES: &[(&str, &str)] = &[
    (
        "GetStarted.flexipage-meta.xml",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<FlexiPage xmlns="http://soap.sforce.com/2006/04/metadata">
    <flexiPageRegions>
        <name>region1</name>
        <type>Region</type>
    </flexiPageRegions>
    <!-- <flexiPageRegions>
        <name>region2</name>
        <type>Region</type>
    </flexiPageRegions> -->
    <flexiPageRegions>
        <name>region3</name>
        <type>Region</type>
    </flexiPageRegions>
    <masterLabel>Get Started</masterLabel>
</FlexiPage>
"#,
    ),
    (
        "CustomLabels.labels-meta.xml",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomLabels xmlns="http://soap.sforce.com/2006/04/metadata">
    <labels>
        <fullName>geA11yAdded</fullName>
        <language>en_US</language>
        <protected>true</protected>
        <shortDescription>Screen reader message.</shortDescription>
        <!-- {0} is a string. Label for a form field -->
        <!-- {1} is a string. Label for a form section -->
        <value>{0} added to the {1} section.</value>
    </labels>
    <labels>
        <fullName>other</fullName>
        <language>en_US</language>
        <protected>true</protected>
        <shortDescription>Other.</shortDescription>
        <value>Other</value>
    </labels>
</CustomLabels>
"#,
    ),
];

async fn disassemble(path: &Path, strategy: &str) {
    DisassembleXmlFileHandler::new()
        .disassemble(
            path.to_str().unwrap(),
            Some("fullName,name"),
            Some(strategy),
            false,
            true,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
            None,
            None,
        )
        .await
        .expect("disassemble");
}

#[tokio::test]
async fn files_with_unplaceable_comments_are_left_in_place() {
    for (name, content) in FILES {
        for strategy in ["unique-id", "grouped-by-tag"] {
            let tmp = tempfile::tempdir().unwrap();
            let source = tmp.path().join(name);
            std::fs::write(&source, content).unwrap();

            disassemble(&source, strategy).await;

            assert_eq!(
                std::fs::read_to_string(&source).unwrap(),
                *content,
                "{name} / {strategy}"
            );
            let stem = name.split('.').next().unwrap();
            assert!(
                !tmp.path().join(stem).exists(),
                "no output dir: {name} / {strategy}"
            );
        }
    }
}

#[tokio::test]
async fn files_with_unplaceable_comments_are_left_in_place_in_directory_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("flexipages");
    std::fs::create_dir_all(&dir).unwrap();
    let (name, content) = FILES[0];
    let source = dir.join(name);
    std::fs::write(&source, content).unwrap();

    disassemble(&dir, "unique-id").await;

    assert_eq!(std::fs::read_to_string(&source).unwrap(), content);
}
