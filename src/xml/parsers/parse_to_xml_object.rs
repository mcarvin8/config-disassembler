//! Parse file to XmlElement - supports XML, YAML, JSON.

use crate::xml::parsers::{extract_xml_declaration_from_raw, parse_xml_from_str};
use crate::xml::types::XmlElement;
use tokio::fs;

pub async fn parse_to_xml_object(file_path: &str) -> Option<XmlElement> {
    if file_path.to_lowercase().ends_with(".xml") {
        let content = fs::read_to_string(file_path).await.ok()?;
        let mut parsed = parse_xml_from_str(&content, file_path)?;
        // The declaration is not preserved by the parser - extract from raw XML.
        // `parse_xml_from_str` always yields an object, so `as_object_mut` is safe.
        if let (Some(obj), Some(decl)) = (
            parsed.as_object_mut(),
            extract_xml_declaration_from_raw(&content),
        ) {
            obj.insert("?xml".to_string(), decl);
        }
        return Some(parsed);
    }

    let content = fs::read_to_string(file_path).await.ok()?;

    if file_path.to_lowercase().ends_with(".yaml") || file_path.to_lowercase().ends_with(".yml") {
        return serde_yaml::from_str(&content).ok();
    }

    if file_path.to_lowercase().ends_with(".json5") {
        return json5::from_str(&content).ok();
    }

    if file_path.to_lowercase().ends_with(".json") {
        return serde_json::from_str(&content).ok();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_to_xml_object_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");
        std::fs::write(&path, r#"{"root":{"a":1}}"#).unwrap();
        let out = parse_to_xml_object(path.to_str().unwrap()).await;
        assert!(out.is_some());
        let obj = out.unwrap();
        assert!(obj.get("root").is_some());
    }

    #[tokio::test]
    async fn parse_to_xml_object_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.yaml");
        std::fs::write(&path, "root:\n  a: 1\n").unwrap();
        let out = parse_to_xml_object(path.to_str().unwrap()).await;
        assert!(out.is_some());
    }

    #[tokio::test]
    async fn parse_to_xml_object_yml_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.yml");
        std::fs::write(&path, "root: {}").unwrap();
        let out = parse_to_xml_object(path.to_str().unwrap()).await;
        assert!(out.is_some());
    }

    #[tokio::test]
    async fn parse_to_xml_object_json5() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json5");
        std::fs::write(&path, "{ root: { a: 1 } }").unwrap();
        let out = parse_to_xml_object(path.to_str().unwrap()).await;
        assert!(out.is_some());
    }

    #[tokio::test]
    async fn parse_to_xml_object_unsupported_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "not xml").unwrap();
        let out = parse_to_xml_object(path.to_str().unwrap()).await;
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn parse_to_xml_object_xml_with_declaration_and_xmlns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.xml");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><root xmlns="http://example.com"><a>1</a></root>"#;
        std::fs::write(&path, xml).unwrap();
        let out = parse_to_xml_object(path.to_str().unwrap()).await;
        assert!(out.is_some());
        let obj = out.unwrap();
        assert!(obj.get("?xml").is_some());
        let root = obj.get("root").and_then(|r| r.as_object()).unwrap();
        assert_eq!(
            root.get("@xmlns").and_then(|v| v.as_str()),
            Some("http://example.com")
        );
    }
}
