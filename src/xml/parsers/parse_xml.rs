//! Parse XML file from path into XmlElement structure.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use tokio::fs;

use crate::xml::parsers::parse_xml_cdata;
use crate::xml::parsers::{mark_compact_elements, strip_whitespace_text_nodes_in_place};
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
    let mut parsed: Value = match parse_xml_cdata::parse_xml_with_cdata(content) {
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

    // Mark single-element "compact" wrappers (no whitespace around their sole child --
    // e.g. Flow's `<connector><targetReference>X</targetReference></connector>`) before
    // stripping whitespace-only text nodes below, which would otherwise erase the
    // distinction between "never had whitespace" and "had it, now removed". Walk the
    // document root's own children rather than `parsed` itself: the root wrapper always
    // has exactly one key (the root element), which would otherwise always qualify.
    //
    // `parse_xml_with_cdata` always returns `Value::Object` (a populated root or an
    // empty map -- see its two return sites), so `as_object_mut` is infallible here.
    let root_children = parsed
        .as_object_mut()
        .expect("parse_xml_with_cdata always returns an object");
    for value in root_children.values_mut() {
        mark_compact_elements(value);
    }

    strip_whitespace_text_nodes_in_place(&mut parsed);
    Some(parsed)
}

// Compiled once per process. These used to be built with `Regex::new` on
// every call; `extract_xml_declaration_from_raw` runs once per shard on
// reassemble (via `parse_to_xml_object`), so a large component compiled
// thousands of regexes -- profiling sf-decomposer's workload showed regex
// compilation as the single largest share of reassemble CPU.
static XMLNS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"xmlns="([^"]*)""#).expect("valid regex"));
static DECLARATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<\?xml\s+([^?]+)\?>"#).expect("valid regex"));
// Pseudo-attribute values may use either quote style (XML 1.0 §2.8).
static VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"version\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("valid regex"));
static ENCODING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"encoding\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("valid regex"));
static STANDALONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"standalone\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("valid regex")
});

/// Key under the `?xml` object holding the declaration's original text, set only when that
/// text differs from what `BytesDecl::new` would write (e.g. Prettier's `"UTF-8" ?>` or
/// single quotes). `build_xml_string` writes it back verbatim so the round trip is
/// byte-identical; declarations already in canonical form don't carry it, so shard output
/// for them is unchanged.
pub const XML_DECLARATION_RAW_KEY: &str = "#raw";

fn pseudo_attr(re: &Regex, decl_content: &str) -> Option<String> {
    let cap = re.captures(decl_content)?;
    cap.get(1)
        .or_else(|| cap.get(2))
        .map(|m| m.as_str().to_string())
}

/// The declaration exactly as `quick_xml`'s `BytesDecl::new` serializes it.
fn canonical_declaration(
    version: &str,
    encoding: Option<&str>,
    standalone: Option<&str>,
) -> String {
    let mut out = format!(r#"<?xml version="{version}""#);
    if let Some(e) = encoding {
        out.push_str(&format!(r#" encoding="{e}""#));
    }
    if let Some(s) = standalone {
        out.push_str(&format!(r#" standalone="{s}""#));
    }
    out.push_str("?>");
    out
}

/// Extract xmlns attribute from raw XML (quickxml_to_serde drops it).
/// Returns Some(value) if found, None otherwise.
pub fn extract_xmlns_from_raw(xml_content: &str) -> Option<String> {
    XMLNS_RE.captures(xml_content).map(|c| c[1].to_string())
}

/// Extract XML declaration from raw XML (quickxml_to_serde drops it).
/// Returns a Value object like {"@version": "1.0", "@encoding": "UTF-8", "@standalone": "yes"}
/// for use in build_xml_string. None if no declaration found.
pub fn extract_xml_declaration_from_raw(xml_content: &str) -> Option<XmlElement> {
    let caps = DECLARATION_RE.captures(xml_content)?;
    let raw = caps.get(0)?.as_str();
    let decl_content = caps.get(1)?.as_str();
    let version = pseudo_attr(&VERSION_RE, decl_content)?;
    let encoding = pseudo_attr(&ENCODING_RE, decl_content);
    let standalone = pseudo_attr(&STANDALONE_RE, decl_content);

    let mut decl = serde_json::Map::new();
    if raw != canonical_declaration(&version, encoding.as_deref(), standalone.as_deref()) {
        decl.insert(
            XML_DECLARATION_RAW_KEY.to_string(),
            Value::String(raw.to_string()),
        );
    }
    decl.insert("@version".to_string(), Value::String(version));
    if let Some(e) = encoding {
        decl.insert("@encoding".to_string(), Value::String(e));
    }
    if let Some(s) = standalone {
        decl.insert("@standalone".to_string(), Value::String(s));
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
    fn extract_xml_declaration_from_raw_version_only_no_encoding_no_standalone() {
        // Exercises the false branches of both the encoding and standalone
        // `if let Some(cap)` checks inside extract_xml_declaration_from_raw.
        let xml = r#"<?xml version="1.0"?><root/>"#;
        let decl = extract_xml_declaration_from_raw(xml).unwrap();
        let obj = decl.as_object().unwrap();
        assert_eq!(obj.get("@version").and_then(|v| v.as_str()), Some("1.0"));
        assert!(
            obj.get("@encoding").is_none(),
            "encoding must be absent when not in source"
        );
        assert!(
            obj.get("@standalone").is_none(),
            "standalone must be absent when not in source"
        );
    }

    #[test]
    fn extract_xml_declaration_from_raw_omits_raw_for_canonical_form() {
        for xml in [
            r#"<?xml version="1.0"?><root/>"#,
            r#"<?xml version="1.0" encoding="UTF-8"?><root/>"#,
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><root/>"#,
        ] {
            let decl = extract_xml_declaration_from_raw(xml).unwrap();
            assert!(
                decl.get(XML_DECLARATION_RAW_KEY).is_none(),
                "canonical declaration must not carry raw text: {xml}"
            );
        }
    }

    #[test]
    fn extract_xml_declaration_from_raw_keeps_raw_for_non_canonical_form() {
        for (xml, raw) in [
            (
                r#"<?xml version="1.0" encoding="UTF-8" ?><root/>"#,
                r#"<?xml version="1.0" encoding="UTF-8" ?>"#,
            ),
            (
                r#"<?xml version='1.0' encoding='UTF-8'?><root/>"#,
                r#"<?xml version='1.0' encoding='UTF-8'?>"#,
            ),
            (
                r#"<?xml  version = "1.0"  encoding="UTF-8"?><root/>"#,
                r#"<?xml  version = "1.0"  encoding="UTF-8"?>"#,
            ),
        ] {
            let decl = extract_xml_declaration_from_raw(xml).unwrap();
            assert_eq!(
                decl.get(XML_DECLARATION_RAW_KEY).and_then(|v| v.as_str()),
                Some(raw)
            );
            assert_eq!(decl.get("@version").and_then(|v| v.as_str()), Some("1.0"));
            assert_eq!(
                decl.get("@encoding").and_then(|v| v.as_str()),
                Some("UTF-8")
            );
        }
    }

    #[test]
    fn extract_xml_declaration_from_raw_parses_single_quoted_values() {
        let xml = r#"<?xml version='1.0' standalone='no'?><root/>"#;
        let decl = extract_xml_declaration_from_raw(xml).unwrap();
        assert_eq!(decl.get("@version").and_then(|v| v.as_str()), Some("1.0"));
        assert_eq!(decl.get("@standalone").and_then(|v| v.as_str()), Some("no"));
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
