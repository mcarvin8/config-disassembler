//! Extract XML attributes from root element.

use serde_json::{Map, Value};

use crate::xml::types::XmlElement;

fn attr_value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// Extracts XML attributes from a root element and returns them as a flat map.
/// Handles @-prefixed keys (e.g. @xmlns, @version) and xmlns (default namespace).
/// Converts values to strings for consistent XML output.
pub fn extract_root_attributes(element: &XmlElement) -> XmlElement {
    let mut attributes = Map::new();
    if let Some(obj) = element.as_object() {
        for (key, value) in obj {
            let is_attr = key.starts_with('@') || key == "xmlns";
            if is_attr {
                let attr_key = if key == "xmlns" {
                    "@xmlns".to_string()
                } else {
                    key.clone()
                };
                attributes.insert(attr_key, Value::String(attr_value_to_string(value)));
            }
        }
    }
    Value::Object(attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_at_prefixed_attributes() {
        let element = json!({
            "@xmlns": "http://example.com",
            "@version": "1.0",
            "child": "ignored"
        });
        let attrs = extract_root_attributes(&element);
        let obj = attrs.as_object().unwrap();
        assert_eq!(
            obj.get("@xmlns").and_then(|v| v.as_str()),
            Some("http://example.com")
        );
        assert_eq!(obj.get("@version").and_then(|v| v.as_str()), Some("1.0"));
        assert!(obj.get("child").is_none());
    }

    #[test]
    fn returns_empty_for_non_object() {
        let element = json!("string");
        let attrs = extract_root_attributes(&element);
        assert!(attrs.as_object().unwrap().is_empty());
    }

    #[test]
    fn converts_xmlns_to_at_xmlns() {
        let element = json!({ "xmlns": "http://ns.example.com", "child": {} });
        let attrs = extract_root_attributes(&element);
        assert_eq!(
            attrs
                .as_object()
                .unwrap()
                .get("@xmlns")
                .and_then(|v| v.as_str()),
            Some("http://ns.example.com")
        );
    }

    #[test]
    fn attr_value_number_and_bool_converted_to_string() {
        let element = json!({ "@num": 42, "@flag": true, "child": {} });
        let attrs = extract_root_attributes(&element);
        let obj = attrs.as_object().unwrap();
        assert_eq!(obj.get("@num").and_then(|v| v.as_str()), Some("42"));
        assert_eq!(obj.get("@flag").and_then(|v| v.as_str()), Some("true"));
    }

    #[test]
    fn attr_value_null_converted_to_empty_string() {
        let element = json!({ "@empty": null, "child": {} });
        let attrs = extract_root_attributes(&element);
        assert_eq!(attrs.get("@empty").and_then(|v| v.as_str()), Some(""));
    }

    #[test]
    fn attr_value_array_uses_serde_json_fallback() {
        let element = json!({ "@list": [1, 2, 3] });
        let attrs = extract_root_attributes(&element);
        assert_eq!(attrs.get("@list").and_then(|v| v.as_str()), Some("[1,2,3]"));
    }

    #[test]
    fn attr_value_object_uses_serde_json_fallback() {
        let element = json!({ "@obj": { "k": "v" } });
        let attrs = extract_root_attributes(&element);
        assert_eq!(
            attrs.get("@obj").and_then(|v| v.as_str()),
            Some(r#"{"k":"v"}"#)
        );
    }
}
