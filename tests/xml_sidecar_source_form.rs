//! A sidecar element must come back as the exact text it was. Salesforce retrieves an
//! ExternalServiceRegistration's `<schema>` as compact JSON with `&quot;` entities; extracted to a
//! YAML sidecar, it used to come back as pretty JSON with raw quotes. Content no sidecar form can
//! reproduce stays in the XML instead.

use config_disassembler::xml::{DisassembleXmlFileHandler, ReassembleXmlFileHandler, SidecarSpec};
use std::path::{Path, PathBuf};

const EXT: &str = "externalServiceRegistration-meta.xml";

fn esr(schema: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ExternalServiceRegistration xmlns="http://soap.sforce.com/2006/04/metadata">
    <label>Search</label>
    <operations>
        <active>true</active>
        <name>search</name>
    </operations>
    <schema>{schema}</schema>
    <status>Complete</status>
</ExternalServiceRegistration>
"#
    )
}

const COMPACT_ENTITIES: &str = "{&quot;openapi&quot;:&quot;3.0.1&quot;,&quot;info&quot;:{&quot;title&quot;:&quot;Search&quot;,&quot;description&quot;:&quot;&quot;},&quot;paths&quot;:{&quot;/search&quot;:{&quot;get&quot;:{&quot;parameters&quot;:[{&quot;name&quot;:&quot;q&quot;,&quot;required&quot;:true}]}}}}";

async fn disassemble(source: &Path) {
    let spec = SidecarSpec {
        element: "schema".to_string(),
        extension: "yaml".to_string(),
        original_format: None,
    };
    DisassembleXmlFileHandler::new()
        .disassemble(
            source.to_str().unwrap(),
            Some("name"),
            Some("unique-id"),
            false,
            true,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
            Some(std::slice::from_ref(&spec)),
            None,
        )
        .await
        .expect("disassemble");
}

async fn reassemble(dir: &Path) {
    ReassembleXmlFileHandler::new()
        .reassemble(dir.to_str().unwrap(), Some(EXT), true, None)
        .await
        .expect("reassemble");
}

/// Writes `xml` as `Search.<EXT>` and disassembles it; returns (source, disassembled dir).
async fn write_and_disassemble(base: &Path, xml: &str) -> (PathBuf, PathBuf) {
    let source = base.join(format!("Search.{EXT}"));
    std::fs::write(&source, xml).unwrap();
    disassemble(&source).await;
    assert!(!source.exists(), "post-purge removes the original");
    (source, base.join("Search"))
}

#[tokio::test]
async fn retrieved_compact_json_schema_round_trips_byte_for_byte() {
    let original = esr(COMPACT_ENTITIES);
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), &original).await;

    let sidecar = std::fs::read_to_string(dir.join("Search.yaml")).unwrap();
    assert!(sidecar.starts_with("openapi: \"3.0.1\"\n"), "{sidecar}");
    let recorded = std::fs::read_to_string(dir.join(".sidecars.json")).unwrap();
    assert!(
        recorded.contains(r#""json_layout":"compact","escape":"entities""#),
        "{recorded}"
    );

    reassemble(&dir).await;
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
}

#[tokio::test]
async fn other_schema_forms_round_trip_byte_for_byte() {
    for schema in [
        // Pretty JSON with raw quotes: the default form, nothing extra recorded.
        "{\n  \"openapi\": \"3.0.1\"\n}",
        // Compact JSON with raw quotes.
        r#"{"openapi":"3.0.1","paths":{}}"#,
        // Pretty JSON with entities.
        "{\n  &quot;openapi&quot;: &quot;3.0.1&quot;\n}",
        // YAML, which passes through.
        "openapi: 3.0.1\ninfo:\n  title: Search\n",
    ] {
        let original = esr(schema);
        let tmp = tempfile::tempdir().unwrap();
        let (source, dir) = write_and_disassemble(tmp.path(), &original).await;
        assert!(dir.join("Search.yaml").exists(), "{schema}");
        reassemble(&dir).await;
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            original,
            "{schema}"
        );
    }
}

#[tokio::test]
async fn a_file_without_a_trailing_newline_keeps_ending_without_one() {
    let original = esr(COMPACT_ENTITIES).trim_end().to_string();
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), &original).await;
    reassemble(&dir).await;
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
}

#[tokio::test]
async fn a_schema_no_sidecar_form_reproduces_stays_in_the_xml() {
    // `1.50` comes back from YAML as `1.5`, and `: ` spacing in compact JSON isn't reproduced.
    for schema in [
        r#"{"version":1.50}"#,
        "{&quot;openapi&quot;: &quot;3.0.1&quot;}",
    ] {
        let original = esr(schema);
        let tmp = tempfile::tempdir().unwrap();
        let (source, dir) = write_and_disassemble(tmp.path(), &original).await;
        assert!(!dir.join("Search.yaml").exists(), "{schema}");
        let leaf = std::fs::read_to_string(dir.join(format!("Search.{EXT}"))).unwrap();
        assert!(
            leaf.contains(&format!("<schema>{schema}</schema>")),
            "{leaf}"
        );

        reassemble(&dir).await;
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            original,
            "{schema}"
        );
    }
}

#[tokio::test]
async fn directories_written_before_source_forms_reassemble_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let (source, dir) = write_and_disassemble(tmp.path(), &esr(COMPACT_ENTITIES)).await;
    // An older version recorded only the spec and the original format.
    std::fs::write(
        dir.join(".sidecars.json"),
        r#"[{"element":"schema","extension":"yaml","original_format":"json"}]"#,
    )
    .unwrap();

    reassemble(&dir).await;
    let out = std::fs::read_to_string(&source).unwrap();
    assert!(
        out.contains("<schema>{\n  \"openapi\": \"3.0.1\",\n  \"info\": {"),
        "{out}"
    );
}
