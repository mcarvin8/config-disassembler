//! Round-trip verification: disassemble + reassemble an XML file in an
//! isolated temp directory and report whether the reconstructed file
//! matches the original, ignoring sibling/attribute reordering.

use crate::xml::handlers::{DisassembleXmlFileHandler, ReassembleXmlFileHandler};
use crate::xml::parsers::parse_xml_from_str;
use crate::xml::types::{DecomposeRule, MultiLevelRule, SidecarSpec, XmlElement};
use serde_json::{Map, Value};
use tokio::fs;

/// Outcome of a round-trip verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundtripStatus {
    /// Reconstructed file is byte-identical to the original.
    Identical,
    /// Reconstructed file differs only in sibling/attribute order; content
    /// is semantically equal.
    Reordered,
    /// Reconstructed file lost or changed content. The `String` names the reason.
    Drift(String),
}

/// Options forwarded to the underlying disassemble/reassemble calls.
#[derive(Debug, Clone, Default)]
pub struct VerifyOptions<'a> {
    pub unique_id_elements: Option<&'a str>,
    pub strategy: Option<&'a str>,
    pub ignore_path: &'a str,
    /// Extension (may itself contain dots, e.g. `"permissionset-meta.xml"`)
    /// passed to the round-trip's `reassemble()` call — same parameter as
    /// [`crate::xml::ReassembleXmlFileHandler::reassemble`]'s
    /// `file_extension`. The disassembled directory is named after the
    /// original filename with its last dot segment stripped (e.g.
    /// `HR_Admin.permissionset-meta.xml` disassembles into `HR_Admin/`), so
    /// reassembling with the wrong extension produces a reconstructed
    /// filename that doesn't match the original. Defaults to whatever
    /// suffix the original filename has beyond the disassembled directory's
    /// name, which reproduces the original filename exactly.
    pub file_extension: Option<&'a str>,
    pub multi_level_rules: Option<&'a [MultiLevelRule]>,
    pub decompose_rules: Option<&'a [DecomposeRule]>,
    pub sidecar_specs: Option<&'a [SidecarSpec]>,
}

/// Disassemble and reassemble `file_path` inside an isolated temp
/// directory, then compare the reconstructed XML against the original.
/// The caller's file is never modified.
pub async fn verify_roundtrip(
    file_path: &str,
    options: VerifyOptions<'_>,
) -> Result<RoundtripStatus, Box<dyn std::error::Error + Send + Sync>> {
    let original_content = fs::read_to_string(file_path).await?;
    let original_parsed = parse_xml_from_str(&original_content, file_path);

    let base_name = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input.xml")
        .to_string();

    let temp_dir = tempfile::tempdir()?;
    let temp_copy = temp_dir.path().join(&base_name);
    fs::copy(file_path, &temp_copy).await?;

    DisassembleXmlFileHandler::new()
        .disassemble(
            temp_copy.to_string_lossy().as_ref(),
            options.unique_id_elements,
            options.strategy,
            true,
            true,
            options.ignore_path,
            "xml",
            options.multi_level_rules,
            options.decompose_rules,
            options.sidecar_specs,
        )
        .await?;

    let disassembled_dir = find_only_subdirectory(temp_dir.path()).await?;
    let Some(disassembled_dir) = disassembled_dir else {
        return Ok(RoundtripStatus::Drift(
            "missing in round-trip output".to_string(),
        ));
    };

    let dir_base_name = disassembled_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output");

    // The disassembled dir's basename need not match the original filename:
    // the crate strips the last dot segment off the file stem when naming
    // it (e.g. `HR_Admin.permissionset-meta.xml` disassembles into
    // `HR_Admin/`). Default the reassemble extension to whatever suffix the
    // original filename has beyond that basename, so the reconstructed file
    // matches the original filename unless the caller overrides it.
    let file_extension: String = options
        .file_extension
        .map(str::to_string)
        .unwrap_or_else(|| {
            base_name
                .strip_prefix(&format!("{dir_base_name}."))
                .map(str::to_string)
                .unwrap_or_else(|| "xml".to_string())
        });

    ReassembleXmlFileHandler::new()
        .reassemble(
            disassembled_dir.to_string_lossy().as_ref(),
            Some(&file_extension),
            true,
            options.sidecar_specs,
        )
        .await?;

    let reconstructed_path = disassembled_dir
        .parent()
        .unwrap_or(temp_dir.path())
        .join(format!("{dir_base_name}.{file_extension}"));
    let reconstructed_content = match fs::read_to_string(&reconstructed_path).await {
        Ok(c) => c,
        Err(_) => {
            return Ok(RoundtripStatus::Drift(
                "missing in round-trip output".to_string(),
            ));
        }
    };

    if original_content == reconstructed_content {
        return Ok(RoundtripStatus::Identical);
    }

    let reconstructed_parsed = parse_xml_from_str(
        &reconstructed_content,
        &reconstructed_path.to_string_lossy(),
    );
    match (original_parsed, reconstructed_parsed) {
        (Some(orig), Some(recon)) if canonicalize(&orig) == canonicalize(&recon) => {
            Ok(RoundtripStatus::Reordered)
        }
        _ => Ok(RoundtripStatus::Drift("content drift".to_string())),
    }
}

/// Returns the single directory entry directly under `dir`, if exactly one
/// exists. `verify_roundtrip` copies only one file into an otherwise-empty
/// temp dir before disassembling, so the disassembled tree is always the
/// only subdirectory produced.
async fn find_only_subdirectory(
    dir: &std::path::Path,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
    let mut read_dir = fs::read_dir(dir).await?;
    let mut found = None;
    while let Some(entry) = read_dir.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            found = Some(entry.path());
        }
    }
    Ok(found)
}

/// Recursively normalize a parsed XML value so structurally-equal-but-reordered
/// trees compare equal: object keys are sorted, and array elements are sorted
/// by the canonical JSON string of each (already-canonicalized) element.
fn canonicalize(value: &XmlElement) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), canonicalize(&map[key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            let mut canonical: Vec<Value> = items.iter().map(canonicalize).collect();
            canonical.sort_by(|a, b| {
                serde_json::to_string(a)
                    .unwrap_or_default()
                    .cmp(&serde_json::to_string(b).unwrap_or_default())
            });
            Value::Array(canonical)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_ignores_object_key_order() {
        let a = serde_json::json!({ "b": 1, "a": 2 });
        let b = serde_json::json!({ "a": 2, "b": 1 });
        assert_eq!(canonicalize(&a), canonicalize(&b));
    }

    #[test]
    fn canonicalize_ignores_array_element_order() {
        let a = serde_json::json!([{ "id": 1 }, { "id": 2 }]);
        let b = serde_json::json!([{ "id": 2 }, { "id": 1 }]);
        assert_eq!(canonicalize(&a), canonicalize(&b));
    }

    #[test]
    fn canonicalize_distinguishes_different_content() {
        let a = serde_json::json!({ "a": 1 });
        let b = serde_json::json!({ "a": 2 });
        assert_ne!(canonicalize(&a), canonicalize(&b));
    }

    #[test]
    fn canonicalize_recurses_into_nested_arrays_and_objects() {
        let a = serde_json::json!({ "items": [{ "x": [2, 1] }, { "x": [1, 2] }] });
        let b = serde_json::json!({ "items": [{ "x": [1, 2] }, { "x": [2, 1] }] });
        assert_eq!(canonicalize(&a), canonicalize(&b));
    }

    #[tokio::test]
    async fn verify_roundtrip_identical_for_simple_xml() {
        // The disassembler's own serialization (attribute/element formatting)
        // need not match arbitrary hand-written input byte-for-byte even when
        // no data is lost — that's exactly the "Reordered" case. To exercise
        // a genuine `Identical` result, first normalize the input by running
        // it through one disassemble/reassemble pass directly (not through
        // `verify_roundtrip`, which never mutates its input); feeding that
        // canonical output back in is deterministic and must round-trip
        // byte-identical.
        let tmp = tempfile::tempdir().unwrap();
        let xml_path = tmp.path().join("Simple.xml");
        tokio::fs::write(
            &xml_path,
            r#"<?xml version="1.0" encoding="UTF-8"?><Root xmlns="http://example.com"><Child><Name>hello</Name></Child></Root>"#,
        )
        .await
        .unwrap();

        DisassembleXmlFileHandler::new()
            .disassemble(
                xml_path.to_str().unwrap(),
                None,
                None,
                true,
                true,
                "",
                "xml",
                None,
                None,
                None,
            )
            .await
            .unwrap();
        ReassembleXmlFileHandler::new()
            .reassemble(
                tmp.path().join("Simple").to_str().unwrap(),
                Some("xml"),
                true,
                None,
            )
            .await
            .unwrap();

        let status = verify_roundtrip(xml_path.to_str().unwrap(), VerifyOptions::default())
            .await
            .unwrap();
        assert_eq!(status, RoundtripStatus::Identical);
    }

    #[tokio::test]
    async fn verify_roundtrip_reordered_when_sibling_order_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let xml_path = tmp.path().join("Multi.xml");
        // Repeated same-tag children keyed by name: the unique-id strategy
        // splits these into separate files and re-merges by filename order,
        // which need not match the original document order.
        tokio::fs::write(
            &xml_path,
            r#"<?xml version="1.0" encoding="UTF-8"?><Root xmlns="http://example.com"><child><name>zebra</name></child><child><name>apple</name></child></Root>"#,
        )
        .await
        .unwrap();

        let status = verify_roundtrip(
            xml_path.to_str().unwrap(),
            VerifyOptions {
                unique_id_elements: Some("name"),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(status, RoundtripStatus::Reordered);
    }

    #[tokio::test]
    async fn verify_roundtrip_drift_when_input_unparseable() {
        let tmp = tempfile::tempdir().unwrap();
        let xml_path = tmp.path().join("Bad.xml");
        tokio::fs::write(&xml_path, "<<not xml").await.unwrap();

        let status = verify_roundtrip(xml_path.to_str().unwrap(), VerifyOptions::default())
            .await
            .unwrap();
        assert_eq!(
            status,
            RoundtripStatus::Drift("missing in round-trip output".to_string())
        );
    }

    #[tokio::test]
    async fn verify_roundtrip_handles_dotted_meta_filename() {
        // Regression test: the disassembled directory for a stem like
        // `HR_Admin.permissionset-meta` is named `HR_Admin` (the crate
        // strips the last dot segment). Without a correct default
        // `file_extension`, reassemble would write `HR_Admin.xml` — a
        // filename that does NOT match the original
        // `HR_Admin.permissionset-meta.xml` — and `verify_roundtrip` would
        // look for the wrong reconstructed file, falsely reporting
        // `Drift("missing in round-trip output")` for every dotted `-meta`
        // filename (the common case for real Salesforce metadata).
        let status = verify_roundtrip(
            "fixtures/xml/general/HR_Admin.permissionset-meta.xml",
            VerifyOptions {
                unique_id_elements: Some(
                    "application,apexClass,name,externalDataSource,flow,object,apexPage,recordType,tab,field",
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(status, RoundtripStatus::Identical);
    }
}
