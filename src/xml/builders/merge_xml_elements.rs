//! Merge multiple XML elements into one.

use serde_json::{Map, Value};

use crate::xml::types::XmlElement;

fn is_mergeable_object(value: &Value) -> bool {
    value.is_object() && !value.is_array()
}

fn merge_element_content(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    for (key, value) in source {
        if value.is_array() {
            merge_array_value(target, key, value.as_array().unwrap());
        } else if is_mergeable_object(value) {
            merge_object_value(target, key, value.as_object().unwrap());
        } else {
            merge_primitive_value(target, key, value);
        }
    }
}

fn merge_array_value(target: &mut Map<String, Value>, key: &str, value: &[Value]) {
    if !target.contains_key(key) {
        target.insert(key.to_string(), Value::Array(value.to_vec()));
    } else if let Some(Value::Array(arr)) = target.get_mut(key) {
        arr.extend(value.iter().cloned());
    } else {
        let existing = target.remove(key).unwrap();
        target.insert(
            key.to_string(),
            Value::Array(
                [vec![existing], value.to_vec()]
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
        );
    }
}

fn merge_object_value(target: &mut Map<String, Value>, key: &str, value: &Map<String, Value>) {
    if let Some(Value::Array(arr)) = target.get_mut(key) {
        arr.push(Value::Object(value.clone()));
    } else if let Some(existing) = target.get(key) {
        let existing = existing.clone();
        target.insert(
            key.to_string(),
            Value::Array(vec![existing, Value::Object(value.clone())]),
        );
    } else {
        target.insert(key.to_string(), Value::Object(value.clone()));
    }
}

fn merge_primitive_value(target: &mut Map<String, Value>, key: &str, value: &Value) {
    if !target.contains_key(key) {
        target.insert(key.to_string(), value.clone());
    }
}

fn default_xml_declaration() -> Value {
    let mut decl = Map::new();
    decl.insert("@version".to_string(), Value::String("1.0".to_string()));
    decl.insert("@encoding".to_string(), Value::String("UTF-8".to_string()));
    Value::Object(decl)
}

fn build_final_xml_element(
    declaration: Option<&Value>,
    root_key: &str,
    content: Map<String, Value>,
) -> XmlElement {
    let mut result = Map::new();
    let decl = declaration.cloned().unwrap_or_else(default_xml_declaration);
    result.insert("?xml".to_string(), decl);
    result.insert(root_key.to_string(), Value::Object(content));
    Value::Object(result)
}

/// Reorder the root element's child keys to match the given order.
/// Keys not in `key_order` are appended at the end.
pub fn reorder_root_keys(element: &XmlElement, key_order: &[String]) -> Option<XmlElement> {
    let obj = element.as_object()?;
    let root_key = obj.keys().find(|k| *k != "?xml")?.clone();
    let root_content = obj.get(&root_key)?.as_object()?;
    let mut reordered = Map::new();
    for key in key_order {
        if let Some(v) = root_content.get(key) {
            reordered.insert(key.clone(), v.clone());
        }
    }
    for (key, value) in root_content {
        if !reordered.contains_key(key) {
            reordered.insert(key.clone(), value.clone());
        }
    }
    let mut result = Map::new();
    if let Some(decl) = obj.get("?xml") {
        result.insert("?xml".to_string(), decl.clone());
    }
    result.insert(root_key, Value::Object(reordered));
    Some(Value::Object(result))
}

/// Merge multiple XML elements into one.
///
/// Scans elements for the first non-`?xml` root key; tolerates leading entries
/// that are empty or declaration-only (e.g. a partially-written disassembled file),
/// which previously caused the whole merge to return `None` and the caller to
/// emit an empty `<root>` document.
pub fn merge_xml_elements(elements: &[XmlElement]) -> Option<XmlElement> {
    if elements.is_empty() {
        log::error!("No elements to merge.");
        return None;
    }

    let root_key = elements
        .iter()
        .find_map(|el| el.as_object()?.keys().find(|k| *k != "?xml").cloned())?;

    let declaration = elements.iter().find_map(|el| el.as_object()?.get("?xml"));

    let mut merged_content = Map::new();
    for element in elements {
        if let Some(obj) = element.as_object() {
            if let Some(root_content) = obj.get(&root_key) {
                if let Some(content_obj) = root_content.as_object() {
                    merge_element_content(&mut merged_content, content_obj);
                }
            }
        }
    }

    Some(build_final_xml_element(
        declaration,
        &root_key,
        merged_content,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_empty_returns_none() {
        assert!(merge_xml_elements(&[]).is_none());
    }

    #[test]
    fn merge_single_element_preserves_structure() {
        let el = json!({
            "?xml": { "@version": "1.0", "@encoding": "UTF-8" },
            "Root": { "@xmlns": "http://example.com", "child": "a" }
        });
        let merged = merge_xml_elements(std::slice::from_ref(&el)).unwrap();
        assert!(merged.get("?xml").is_some());
        let root = merged.get("Root").and_then(|v| v.as_object()).unwrap();
        assert_eq!(root.get("child").and_then(|v| v.as_str()), Some("a"));
        assert_eq!(
            root.get("@xmlns").and_then(|v| v.as_str()),
            Some("http://example.com")
        );
    }

    #[test]
    fn merge_two_elements_combines_nested_objects_into_array() {
        let a = json!({ "Root": { "section": { "name": "first" } } });
        let b = json!({ "Root": { "section": { "name": "second" } } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let root = merged.get("Root").and_then(|v| v.as_object()).unwrap();
        let sections = root.get("section").and_then(|v| v.as_array()).unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(
            sections[0].get("name").and_then(|v| v.as_str()),
            Some("first")
        );
        assert_eq!(
            sections[1].get("name").and_then(|v| v.as_str()),
            Some("second")
        );
    }

    #[test]
    fn reorder_root_keys_reorders_and_appends_extra() {
        let el = json!({
            "?xml": { "@version": "1.0" },
            "Root": { "z": "last", "a": "first", "m": "mid" }
        });
        let reordered = reorder_root_keys(&el, &["a".into(), "m".into()]).unwrap();
        let root = reordered.get("Root").and_then(|v| v.as_object()).unwrap();
        let keys: Vec<_> = root
            .keys()
            .filter(|k| !k.starts_with('@'))
            .cloned()
            .collect();
        assert_eq!(keys, ["a", "m", "z"]);
    }

    #[test]
    fn merge_elements_without_declaration_uses_default() {
        let a = json!({ "Root": { "a": "1" } });
        let b = json!({ "Root": { "b": "2" } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        // Pin the exact shape of `default_xml_declaration`: a `Default::default()`
        // substitution would replace it with `Value::Null` and the assertions on
        // `is_some()` alone would still pass.
        let decl = merged
            .get("?xml")
            .and_then(|v| v.as_object())
            .expect("default ?xml declaration must be an object, not Null");
        assert_eq!(decl.get("@version").and_then(|v| v.as_str()), Some("1.0"));
        assert_eq!(
            decl.get("@encoding").and_then(|v| v.as_str()),
            Some("UTF-8")
        );
        let root = merged.get("Root").and_then(|v| v.as_object()).unwrap();
        assert_eq!(root.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(root.get("b").and_then(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn merge_array_value_coalesces_into_array() {
        let a = json!({ "Root": { "item": { "x": "1" } } });
        let b = json!({ "Root": { "item": { "x": "2" } } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let items = merged
            .get("Root")
            .and_then(|r| r.get("item"))
            .and_then(|i| i.as_array())
            .unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn merge_elements_with_non_object_or_missing_root_skipped() {
        // First element supplies the root key. Second element is a primitive (skipped).
        // Third element is an object but lacks the root key (skipped). Fourth element
        // has the root key but the root value is a primitive (skipped).
        let a = json!({ "Root": { "item": "a" } });
        let b = json!("not-an-object");
        let c = json!({ "Other": { "item": "c" } });
        let d = json!({ "Root": "primitive" });
        let merged = merge_xml_elements(&[a, b, c, d]).unwrap();
        let root = merged.get("Root").and_then(|v| v.as_object()).unwrap();
        assert_eq!(root.get("item").and_then(|v| v.as_str()), Some("a"));
    }

    #[test]
    fn merge_primitive_value_does_not_overwrite_existing() {
        // First has primitive "item"; second also has primitive "item" - kept first
        let a = json!({ "Root": { "item": "first" } });
        let b = json!({ "Root": { "item": "second" } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let item = merged
            .get("Root")
            .and_then(|r| r.get("item"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(item, "first");
    }

    #[test]
    fn merge_array_value_when_target_key_absent_sets_array() {
        // Target lacks "item"; second element provides item as array
        let a = json!({ "Root": { "other": "x" } });
        let b = json!({ "Root": { "item": [ { "x": "1" } ] } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let items = merged
            .get("Root")
            .and_then(|r| r.get("item"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn merge_array_value_extends_existing_array() {
        // Both elements have item as array → extend
        let a = json!({ "Root": { "item": [ { "x": "1" } ] } });
        let b = json!({ "Root": { "item": [ { "x": "2" }, { "x": "3" } ] } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let items = merged
            .get("Root")
            .and_then(|r| r.get("item"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn reorder_root_keys_returns_none_for_invalid_inputs() {
        // Not an object
        assert!(reorder_root_keys(&json!("string"), &["a".into()]).is_none());
        // No root key (only declaration)
        assert!(reorder_root_keys(&json!({ "?xml": {} }), &["a".into()]).is_none());
        // Root value not object
        let el = json!({ "Root": "primitive" });
        assert!(reorder_root_keys(&el, &["a".into()]).is_none());
    }

    #[test]
    fn reorder_root_keys_without_declaration_omits_it() {
        let el = json!({ "Root": { "b": "2", "a": "1" } });
        let out = reorder_root_keys(&el, &["a".into(), "b".into()]).unwrap();
        assert!(out.get("?xml").is_none());
        let root = out.get("Root").and_then(|v| v.as_object()).unwrap();
        let keys: Vec<_> = root.keys().cloned().collect();
        assert_eq!(keys, ["a", "b"]);
    }

    #[test]
    fn merge_skips_leading_declaration_only_element() {
        // Simulates a partially-written disassembled file that only contains the XML
        // declaration: the merge should still produce a valid root from later elements.
        let a = json!({ "?xml": { "@version": "1.0" } });
        let b = json!({ "?xml": { "@version": "1.0" }, "Root": { "item": "x" } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let root = merged.get("Root").and_then(|v| v.as_object()).unwrap();
        assert_eq!(root.get("item").and_then(|v| v.as_str()), Some("x"));
    }

    #[test]
    fn merge_skips_leading_empty_object_element() {
        // A fully empty object leading the slice (e.g. an empty file) used to poison
        // the merge and emit `<root></root>`. Now it's skipped and the next element wins.
        let a = json!({});
        let b = json!({ "Root": { "item": "ok" } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let root = merged.get("Root").and_then(|v| v.as_object()).unwrap();
        assert_eq!(root.get("item").and_then(|v| v.as_str()), Some("ok"));
    }

    #[test]
    fn merge_returns_none_when_every_element_has_no_root() {
        // All elements are empty or declaration-only: merge must return None so callers
        // can surface an error rather than writing a stub document.
        let a = json!({});
        let b = json!({ "?xml": { "@version": "1.0" } });
        assert!(merge_xml_elements(&[a, b]).is_none());
    }

    #[test]
    fn merge_array_value_when_target_has_key_non_array_coalesces_into_array() {
        // First element has "item" as object; second has "item" as array → coalesce to [existing, ...new]
        let a = json!({ "Root": { "item": { "x": "1" } } });
        let b = json!({ "Root": { "item": [ { "x": "2" } ] } });
        let merged = merge_xml_elements(&[a, b]).unwrap();
        let items = merged
            .get("Root")
            .and_then(|r| r.get("item"))
            .and_then(|i| i.as_array())
            .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("x").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(items[1].get("x").and_then(|v| v.as_str()), Some("2"));
    }
}
