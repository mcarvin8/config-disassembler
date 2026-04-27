//! Parse unique ID from XML element for file naming.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::xml::types::XmlElement;

/// Cache for stringified elements - we use a simple approach in Rust.
/// For full equivalence we could use a type with interior mutability and weak refs.
fn create_short_hash(element: &XmlElement) -> String {
    let stringified = serde_json::to_string(element).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(stringified.as_bytes());
    let result = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(8);
    for b in result.iter().take(4) {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

fn is_object(value: &Value) -> bool {
    value.is_object() && !value.is_array()
}

/// Extract string from a value - handles both direct strings and objects with #text (XML leaf elements).
fn value_as_string(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    value
        .as_object()
        .and_then(|obj| obj.get("#text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn find_direct_field_match(element: &XmlElement, field_names: &[&str]) -> Option<String> {
    let obj = element.as_object()?;
    for name in field_names {
        if let Some(value) = obj.get(*name) {
            if let Some(s) = value_as_string(value) {
                return Some(s);
            }
        }
    }
    None
}

fn find_nested_field_match(element: &XmlElement, unique_id_elements: &str) -> Option<String> {
    let obj = element.as_object()?;
    for (_, child) in obj {
        if is_object(child) {
            // parse_unique_id_element always returns a non-empty string (falls back to a hash),
            // so the first nested object match is sufficient.
            return Some(parse_unique_id_element(child, Some(unique_id_elements)));
        }
    }
    None
}

/// Get a unique ID for an element, using configured fields or a hash.
pub fn parse_unique_id_element(element: &XmlElement, unique_id_elements: Option<&str>) -> String {
    if let Some(ids) = unique_id_elements {
        let field_names: Vec<&str> = ids.split(',').map(|s| s.trim()).collect();
        find_direct_field_match(element, &field_names)
            .or_else(|| find_nested_field_match(element, ids))
            .unwrap_or_else(|| create_short_hash(element))
    } else {
        create_short_hash(element)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_direct_field() {
        let el = json!({ "name": "Get_Info", "label": "Get Info" });
        assert_eq!(parse_unique_id_element(&el, Some("name")), "Get_Info");
    }

    #[test]
    fn finds_deeply_nested_field() {
        // value before connector so we find elementReference (matches TS iteration order)
        let el = json!({
            "value": { "elementReference": "accts.accounts" },
            "connector": { "targetReference": "X" }
        });
        assert_eq!(
            parse_unique_id_element(&el, Some("elementReference")),
            "accts.accounts"
        );
    }

    #[test]
    fn finds_id_in_grandchild() {
        let el = json!({
            "wrapper": {
                "inner": { "name": "NestedName" }
            }
        });
        assert_eq!(parse_unique_id_element(&el, Some("name")), "NestedName");
    }

    #[test]
    fn value_as_string_returns_none_for_non_string_non_text_objects() {
        // Directly named field exists but value is neither a string nor an object with #text.
        // Exercises the None-return path inside value_as_string plus the "no match, move on"
        // path inside find_direct_field_match.
        let el = json!({ "name": { "other": "xxx" } });
        let id = parse_unique_id_element(&el, Some("name"));
        // Falls through to the 8-char short-hash fallback.
        assert_eq!(id.len(), 8);
    }

    #[test]
    fn falls_back_to_hash_when_no_match_and_no_nested_object() {
        // No direct match and no nested object match → hash fallback.
        let el = json!({ "a": "string", "b": "another" });
        let id = parse_unique_id_element(&el, Some("name"));
        assert_eq!(id.len(), 8);
    }

    #[test]
    fn hash_fallback_when_unique_id_elements_is_none() {
        let el = json!({ "a": "b" });
        let id = parse_unique_id_element(&el, None);
        assert_eq!(id.len(), 8);
    }

    #[test]
    fn non_object_element_returns_hash() {
        let el = json!("just-a-string");
        let id = parse_unique_id_element(&el, Some("name"));
        assert_eq!(id.len(), 8);
    }

    #[test]
    fn finds_name_from_text_object() {
        // XML parser stores leaf elements as { "#text": "value" }
        let el = json!({
            "name": { "#text": "Get_Info" },
            "label": { "#text": "Get Info" },
            "actionName": { "#text": "GetFirstFromCollection" }
        });
        assert_eq!(parse_unique_id_element(&el, Some("name")), "Get_Info");
        assert_eq!(
            parse_unique_id_element(&el, Some("actionName")),
            "GetFirstFromCollection"
        );
    }
}
