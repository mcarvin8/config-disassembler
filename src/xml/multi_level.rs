//! Multi-level disassembly: strip a root element and re-disassemble with different unique-id elements.

use serde_json::{Map, Value};

use crate::xml::builders::build_xml_string;
use crate::xml::types::{MultiLevelConfig, XmlElement};

/// Strip the given element and build a new XML string.
/// - If it is the root element: its inner content becomes the new document (with ?xml preserved).
/// - If it is a child of the root (e.g. programProcesses under LoyaltyProgramSetup): unwrap it so
///   its inner content becomes the direct children of the root; the root element is kept.
pub fn strip_root_and_build_xml(parsed: &XmlElement, element_to_strip: &str) -> Option<String> {
    let obj = parsed.as_object()?;
    let root_key = obj.keys().find(|k| *k != "?xml")?.clone();
    let root_val = obj.get(&root_key)?.as_object()?;
    let decl = obj.get("?xml").cloned().unwrap_or_else(|| {
        let mut d = Map::new();
        d.insert("@version".to_string(), Value::String("1.0".to_string()));
        d.insert("@encoding".to_string(), Value::String("UTF-8".to_string()));
        Value::Object(d)
    });

    if root_key == element_to_strip {
        // Strip the root: new doc = ?xml + inner content of root (element keys only, not @attributes)
        let mut new_obj = Map::new();
        new_obj.insert("?xml".to_string(), decl);
        for (k, v) in root_val {
            if !k.starts_with('@') {
                new_obj.insert(k.clone(), v.clone());
            }
        }
        return Some(build_xml_string(&Value::Object(new_obj)));
    }

    // Strip a child of the root: unwrap it so its inner content becomes direct children of the root
    let inner = root_val.get(element_to_strip)?.as_object()?;
    let mut new_root_val = Map::new();
    for (k, v) in root_val {
        if k != element_to_strip {
            new_root_val.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in inner {
        new_root_val.insert(k.clone(), v.clone());
    }
    let mut new_obj = Map::new();
    new_obj.insert("?xml".to_string(), decl);
    new_obj.insert(root_key, Value::Object(new_root_val));
    Some(build_xml_string(&Value::Object(new_obj)))
}

/// Capture xmlns from the root element (e.g. LoyaltyProgramSetup) for later wrap.
pub fn capture_xmlns_from_root(parsed: &XmlElement) -> Option<String> {
    let obj = parsed.as_object()?;
    let root_key = obj.keys().find(|k| *k != "?xml")?.clone();
    let root_val = obj.get(&root_key)?.as_object()?;
    let xmlns = root_val.get("@xmlns")?.as_str()?;
    Some(xmlns.to_string())
}

/// Derive path_segment from file_pattern (e.g. "programProcesses-meta" -> "programProcesses").
pub fn path_segment_from_file_pattern(file_pattern: &str) -> String {
    // `split('-').next()` always returns `Some(_)` for any string - even an empty one -
    // so falling back to the original `file_pattern` is unreachable.
    file_pattern
        .split('-')
        .next()
        .unwrap_or(file_pattern)
        .to_string()
}

/// Load multi-level config from a directory (reads .multi_level.json).
pub async fn load_multi_level_config(dir_path: &std::path::Path) -> Option<MultiLevelConfig> {
    let path = dir_path.join(".multi_level.json");
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// Persist multi-level config to a directory.
pub async fn save_multi_level_config(
    dir_path: &std::path::Path,
    config: &MultiLevelConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = dir_path.join(".multi_level.json");
    let content = serde_json::to_string_pretty(config)?;
    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Ensure all XML files in a segment directory have structure:
/// document_root (with xmlns) > inner_wrapper (no xmlns) > content.
/// Used after inner-level reassembly for multi-level (e.g. LoyaltyProgramSetup > programProcesses).
pub async fn ensure_segment_files_structure(
    dir_path: &std::path::Path,
    document_root: &str,
    inner_wrapper: &str,
    xmlns: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use crate::xml::parsers::parse_xml_from_str;
    use serde_json::Map;

    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(dir_path).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        entries.push(entry);
    }
    // Sort for deterministic cross-platform ordering
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".xml") {
            continue;
        }
        let path_str = path.to_string_lossy();
        // Read errors on a file the walker just reported as present are essentially impossible
        // (concurrent deletion); treat the content as empty so downstream lookups skip naturally.
        let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        let Some(parsed) = parse_xml_from_str(&content, &path_str) else {
            continue;
        };
        // parse_xml_from_str always yields a JSON object when it returns Some; fall back to an
        // empty map for any unexpected shape so subsequent lookups simply produce None.
        let obj = parsed.as_object().cloned().unwrap_or_default();
        let Some(current_root_key) = obj.keys().find(|k| *k != "?xml").cloned() else {
            continue;
        };
        let root_val = obj
            .get(&current_root_key)
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let decl = obj.get("?xml").cloned().unwrap_or_else(|| {
            let mut d = Map::new();
            d.insert(
                "@version".to_string(),
                serde_json::Value::String("1.0".to_string()),
            );
            d.insert(
                "@encoding".to_string(),
                serde_json::Value::String("UTF-8".to_string()),
            );
            serde_json::Value::Object(d)
        });

        let non_attr_keys: Vec<&String> = root_val.keys().filter(|k| *k != "@xmlns").collect();
        let single_inner = non_attr_keys.len() == 1 && non_attr_keys[0].as_str() == inner_wrapper;
        let inner_content: serde_json::Value = if current_root_key == document_root && single_inner
        {
            let inner_obj = root_val
                .get(inner_wrapper)
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_else(Map::new);
            let mut inner_clean = Map::new();
            for (k, v) in &inner_obj {
                if k != "@xmlns" {
                    inner_clean.insert(k.clone(), v.clone());
                }
            }
            serde_json::Value::Object(inner_clean)
        } else {
            serde_json::Value::Object(root_val.clone())
        };

        let already_correct = current_root_key == document_root
            && root_val.get("@xmlns").is_some()
            && single_inner
            && root_val
                .get(inner_wrapper)
                .and_then(|v| v.as_object())
                .map(|o| !o.contains_key("@xmlns"))
                .unwrap_or(true);
        if already_correct {
            continue;
        }

        // Build document_root (with @xmlns only on root) > inner_wrapper (no xmlns) > content
        let mut root_val_new = Map::new();
        if !xmlns.is_empty() {
            root_val_new.insert(
                "@xmlns".to_string(),
                serde_json::Value::String(xmlns.to_string()),
            );
        }
        root_val_new.insert(inner_wrapper.to_string(), inner_content);

        let mut top = Map::new();
        top.insert("?xml".to_string(), decl);
        top.insert(
            document_root.to_string(),
            serde_json::Value::Object(root_val_new),
        );
        let wrapped = serde_json::Value::Object(top);
        let xml_string = build_xml_string(&wrapped);
        tokio::fs::write(&path, xml_string).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn path_segment_from_file_pattern_strips_suffix() {
        assert_eq!(
            path_segment_from_file_pattern("programProcesses-meta"),
            "programProcesses"
        );
    }

    #[test]
    fn path_segment_from_file_pattern_no_dash() {
        assert_eq!(path_segment_from_file_pattern("foo"), "foo");
    }

    #[test]
    fn strip_root_and_build_xml_strips_child_not_root() {
        let parsed = json!({
            "?xml": { "@version": "1.0" },
            "Root": {
                "programProcesses": { "a": "1", "b": "2" },
                "label": "x"
            }
        });
        let out = strip_root_and_build_xml(&parsed, "programProcesses").unwrap();
        assert!(out.contains("<Root>"));
        assert!(out.contains("<a>1</a>"));
        assert!(out.contains("<b>2</b>"));
        assert!(out.contains("<label>x</label>"));
    }

    #[test]
    fn strip_root_and_build_xml_strips_root_excludes_attributes() {
        let parsed = json!({
            "?xml": { "@version": "1.0" },
            "LoyaltyProgramSetup": {
                "@xmlns": "http://example.com",
                "programProcesses": { "x": "1" }
            }
        });
        let out = strip_root_and_build_xml(&parsed, "LoyaltyProgramSetup").unwrap();
        assert!(!out.contains("@xmlns"));
        assert!(out.contains("programProcesses"));
    }

    #[test]
    fn capture_xmlns_from_root_returns_some() {
        let parsed = json!({
            "Root": { "@xmlns": "http://ns.example.com" }
        });
        assert_eq!(
            capture_xmlns_from_root(&parsed),
            Some("http://ns.example.com".to_string())
        );
    }

    #[test]
    fn capture_xmlns_from_root_returns_none_when_absent() {
        let parsed = json!({ "Root": { "child": "x" } });
        assert!(capture_xmlns_from_root(&parsed).is_none());
    }

    #[tokio::test]
    async fn save_and_load_multi_level_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = MultiLevelConfig {
            rules: vec![crate::xml::types::MultiLevelRule {
                file_pattern: "test-meta".to_string(),
                root_to_strip: "Root".to_string(),
                unique_id_elements: "id".to_string(),
                path_segment: "test".to_string(),
                wrap_root_element: "Root".to_string(),
                wrap_xmlns: "http://example.com".to_string(),
            }],
        };
        save_multi_level_config(dir.path(), &config).await.unwrap();
        let loaded = load_multi_level_config(dir.path()).await.unwrap();
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(loaded.rules[0].path_segment, "test");
    }

    #[tokio::test]
    async fn load_multi_level_config_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_multi_level_config(dir.path()).await.is_none());
    }

    #[tokio::test]
    async fn ensure_segment_files_structure_adds_xmlns_and_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Root>
  <programProcesses><x>1</x></programProcesses>
</Root>"#;
        let path = dir.path().join("segment.xml");
        tokio::fs::write(&path, xml).await.unwrap();
        ensure_segment_files_structure(
            dir.path(),
            "Root",
            "programProcesses",
            "http://example.com",
        )
        .await
        .unwrap();
        let out = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(out.contains("http://example.com"));
        assert!(out.contains("<programProcesses>"));
        assert!(out.contains("<x>1</x>"));
    }

    #[tokio::test]
    async fn ensure_segment_files_structure_skips_already_correct_files() {
        // Root wraps inner_wrapper and has xmlns; inner has no xmlns -> no rewrite.
        let dir = tempfile::tempdir().unwrap();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Root xmlns="http://example.com"><programProcesses><x>1</x></programProcesses></Root>"#;
        let path = dir.path().join("ok.xml");
        tokio::fs::write(&path, xml).await.unwrap();
        let before = tokio::fs::metadata(&path).await.unwrap().modified().ok();
        ensure_segment_files_structure(
            dir.path(),
            "Root",
            "programProcesses",
            "http://example.com",
        )
        .await
        .unwrap();
        let after = tokio::fs::metadata(&path).await.unwrap().modified().ok();
        assert_eq!(before, after, "already-correct files must be left as-is");
    }

    #[tokio::test]
    async fn ensure_segment_files_structure_skips_non_xml_and_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir(dir.path().join("nested"))
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("notes.txt"), "hello")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("broken.xml"), "<<not xml>")
            .await
            .unwrap();
        // No XML payload that matches; should succeed without writing anything.
        ensure_segment_files_structure(
            dir.path(),
            "Root",
            "programProcesses",
            "http://example.com",
        )
        .await
        .unwrap();
        // broken.xml remains unchanged
        let raw = tokio::fs::read_to_string(dir.path().join("broken.xml"))
            .await
            .unwrap();
        assert_eq!(raw, "<<not xml>");
    }

    #[tokio::test]
    async fn ensure_segment_files_structure_skips_xml_missing_root() {
        // Only a declaration, no root element (empty document)
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("empty.xml"), "")
            .await
            .unwrap();
        ensure_segment_files_structure(dir.path(), "Root", "programProcesses", "")
            .await
            .unwrap();
    }
}
