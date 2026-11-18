//! Post-purge must remove the original file for types with sidecar extraction too. With
//! sidecars, the disassembler runs on a temp copy with the sidecar element stripped, so the purge
//! used to hit that copy and leave the original in place (e.g. ExternalServiceRegistration's
//! `<schema>`), silently ignoring post-purge.

use config_disassembler::xml::{DisassembleXmlFileHandler, ReassembleXmlFileHandler, SidecarSpec};
use std::path::Path;

const FIXTURE: &str =
    "fixtures/xml/sidecar/DropboxFileManagerHandler.externalServiceRegistration-meta.xml";
const EXT: &str = "externalServiceRegistration-meta.xml";

fn schema_sidecar() -> SidecarSpec {
    SidecarSpec {
        element: "schema".to_string(),
        extension: "yaml".to_string(),
        original_format: None,
    }
}

async fn disassemble(source: &Path, post_purge: bool) {
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            post_purge,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
            Some(std::slice::from_ref(&schema_sidecar())),
            None,
        )
        .await
        .expect("disassemble");
}

#[tokio::test]
async fn post_purge_removes_the_original_and_reassembly_restores_it() {
    assert!(Path::new(FIXTURE).exists(), "run from the project root");
    let original = std::fs::read_to_string(FIXTURE).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join(format!("DropboxFileManagerHandler.{EXT}"));
    std::fs::write(&source, &original).unwrap();

    disassemble(&source, true).await;

    assert!(!source.exists(), "post-purge must remove the original");
    let dir = tmp.path().join("DropboxFileManagerHandler");
    assert!(
        dir.join("DropboxFileManagerHandler.yaml").exists(),
        "sidecar written"
    );
    // No leftover temp copy beside the original.
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    assert!(leftovers.is_empty(), "no stray files: {leftovers:?}");

    ReassembleXmlFileHandler::new()
        .reassemble(dir.to_str().unwrap(), Some(EXT), true, None)
        .await
        .expect("reassemble");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
}

#[tokio::test]
async fn post_purge_keeps_a_sidecar_file_that_is_not_disassembled() {
    // Only leaf elements besides the sidecar element: nothing to disassemble, so it must stay.
    let leaf_only = r#"<?xml version="1.0" encoding="UTF-8"?>
<ExternalServiceRegistration xmlns="http://soap.sforce.com/2006/04/metadata">
    <label>LeafOnly</label>
    <schema>openapi: 3.0.1</schema>
    <status>Complete</status>
</ExternalServiceRegistration>
"#;
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join(format!("LeafOnly.{EXT}"));
    std::fs::write(&source, leaf_only).unwrap();

    disassemble(&source, true).await;

    assert_eq!(std::fs::read_to_string(&source).unwrap(), leaf_only);
}
