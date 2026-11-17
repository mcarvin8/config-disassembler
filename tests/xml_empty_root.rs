//! A file whose root has no child elements has nothing to disassemble and nothing that could
//! rebuild it, so disassemble must leave it in place even with post-purge. It used to write no
//! shards and then delete the original -- e.g. Salesforce's empty
//! `<CustomObjectTranslation xmlns="..."/>` translation files.

use config_disassembler::xml::DisassembleXmlFileHandler;
use std::path::Path;

const EMPTY_ROOTS: &[&str] = &[
    r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomObjectTranslation xmlns="http://soap.sforce.com/2006/04/metadata"/>
"#,
    r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomObjectTranslation xmlns="http://soap.sforce.com/2006/04/metadata"></CustomObjectTranslation>
"#,
    "<Root/>",
];

async fn disassemble(path: &Path, format: &str, strategy: &str) {
    DisassembleXmlFileHandler::new()
        .disassemble(
            path.to_str().unwrap(),
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
}

#[tokio::test]
async fn empty_root_file_is_kept_with_post_purge() {
    for content in EMPTY_ROOTS {
        for format in ["xml", "json", "yaml"] {
            for strategy in ["unique-id", "grouped-by-tag"] {
                let tmp = tempfile::tempdir().unwrap();
                let source = tmp.path().join("Contact-iw.objectTranslation-meta.xml");
                std::fs::write(&source, content).unwrap();

                disassemble(&source, format, strategy).await;

                let label = format!("{content:?} / {format} / {strategy}");
                assert_eq!(
                    std::fs::read_to_string(&source).unwrap(),
                    *content,
                    "{label}"
                );
                assert!(
                    !tmp.path().join("Contact-iw").exists(),
                    "no output dir: {label}"
                );
            }
        }
    }
}

#[tokio::test]
async fn empty_root_file_is_kept_when_disassembling_its_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("objectTranslations");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("Contact-iw.objectTranslation-meta.xml");
    std::fs::write(&source, EMPTY_ROOTS[0]).unwrap();

    disassemble(&dir, "xml", "unique-id").await;

    assert_eq!(std::fs::read_to_string(&source).unwrap(), EMPTY_ROOTS[0]);
}
