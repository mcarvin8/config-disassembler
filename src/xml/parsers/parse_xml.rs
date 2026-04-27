//! Parse XML file from path into XmlElement structure.

use serde_json::Value;
use tokio::fs;

use crate::xml::parsers::parse_xml_cdata;
use crate::xml::parsers::strip_whitespace_text_nodes;
use crate::xml::types::XmlElement;

/// Parses an XML file from a path.
pub async fn parse_xml(file_path: &str) -> Option<XmlElement> {
    let content = match fs::read_to_string(file_path).await {
        Ok(c) => c,
        Err(e) => {
            log::error!(
                "{} was unable to be parsed and will not be processed. Confirm formatting and try again.",
                file_path
            );
            log::debug!("Parse error: {}", e);
            return None;
        }
    };
    parse_xml_from_str(&content, file_path)
}

/// Parses XML from a string. The file_path is used for error logging only.
/// Uses custom parser that preserves CDATA sections (output as #cdata key).
pub fn parse_xml_from_str(content: &str, file_path: &str) -> Option<XmlElement> {
    let parsed: Value = match parse_xml_cdata::parse_xml_with_cdata(content) {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "{} was unable to be parsed and will not be processed. Confirm formatting and try again.",
                file_path
            );
            log::debug!("Parse error: {}", e);
            return None;
        }
    };

    let cleaned = strip_whitespace_text_nodes(&parsed);
    Some(cleaned)
}

/// Extract xmlns attribute from raw XML (quickxml_to_serde drops it).
/// Returns Some(value) if found, None otherwise.
pub fn extract_xmlns_from_raw(xml_content: &str) -> Option<String> {
    let re = regex::Regex::new(r#"xmlns="([^"]*)""#).ok()?;
    re.captures(xml_content).map(|c| c[1].to_string())
}

/// Extract XML declaration from raw XML (quickxml_to_serde drops it).
/// Returns a Value object like {"@version": "1.0", "@encoding": "UTF-8", "@standalone": "yes"}
/// for use in build_xml_string. None if no declaration found.
pub fn extract_xml_declaration_from_raw(xml_content: &str) -> Option<XmlElement> {
    let decl_re = regex::Regex::new(r#"<\?xml\s+([^?]+)\?>"#).ok()?;
    let decl_content = decl_re.captures(xml_content)?.get(1)?.as_str();
    let mut decl = serde_json::Map::new();
    let version_re = regex::Regex::new(r#"version="([^"]*)""#).ok()?;
    if let Some(cap) = version_re.captures(decl_content) {
        decl.insert("@version".to_string(), Value::String(cap[1].to_string()));
    } else {
        return None;
    }
    let encoding_re = regex::Regex::new(r#"encoding="([^"]*)""#).ok()?;
    if let Some(cap) = encoding_re.captures(decl_content) {
        decl.insert("@encoding".to_string(), Value::String(cap[1].to_string()));
    }
    let standalone_re = regex::Regex::new(r#"standalone="([^"]*)""#).ok()?;
    if let Some(cap) = standalone_re.captures(decl_content) {
        decl.insert("@standalone".to_string(), Value::String(cap[1].to_string()));
    }
    Some(Value::Object(decl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_xmlns_from_raw_finds_namespace() {
        let xml = r#"<root xmlns="http://soap.sforce.com/2006/04/metadata"><a/></root>"#;
        assert_eq!(
            extract_xmlns_from_raw(xml),
            Some("http://soap.sforce.com/2006/04/metadata".to_string())
        );
    }

    #[test]
    fn extract_xmlns_from_raw_returns_none_when_absent() {
        let xml = r#"<root><a/></root>"#;
        assert_eq!(extract_xmlns_from_raw(xml), None);
    }

    #[test]
    fn extract_xml_declaration_from_raw_parses_version_and_encoding() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><root/>"#;
        let decl = extract_xml_declaration_from_raw(xml).unwrap();
        let obj = decl.as_object().unwrap();
        assert_eq!(obj.get("@version").and_then(|v| v.as_str()), Some("1.0"));
        assert_eq!(obj.get("@encoding").and_then(|v| v.as_str()), Some("UTF-8"));
    }

    #[test]
    fn extract_xml_declaration_from_raw_parses_standalone() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><root/>"#;
        let decl = extract_xml_declaration_from_raw(xml).unwrap();
        let obj = decl.as_object().unwrap();
        assert_eq!(obj.get("@standalone").and_then(|v| v.as_str()), Some("yes"));
    }

    #[test]
    fn extract_xml_declaration_from_raw_returns_none_without_declaration() {
        let xml = r#"<root/>"#;
        assert!(extract_xml_declaration_from_raw(xml).is_none());
    }

    #[test]
    fn extract_xml_declaration_from_raw_returns_none_when_version_missing() {
        let xml = r#"<?xml encoding="UTF-8"?><root/>"#;
        assert!(extract_xml_declaration_from_raw(xml).is_none());
    }

    #[test]
    fn parse_xml_from_str_invalid_xml_returns_none() {
        let result = parse_xml_from_str("<<", "test.xml");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn parse_xml_missing_file_returns_none() {
        let result = parse_xml("/nonexistent/path/file.xml").await;
        assert!(result.is_none());
    }
}
