//! Merge multiple XML elements into one.
//!
//! The work is done by the owned (`*_owned`) functions, which move every value
//! from the parsed shards into the merged tree. Reassemble calls those directly,
//! so it never holds a second full copy of the document while merging or
//! reordering. The borrowing [`merge_xml_elements`] / [`reorder_root_keys`] keep
//! the public API and clone their input once before delegating.
//!
//! Key order must match the original clone-based implementation byte for byte.
//! Each branch below makes the same `Map` calls in the same order -- including
//! `remove` + `insert` in the array branch, which with serde_json's
//! `preserve_order` is a *swap*-remove that moves keys. The test module keeps the
//! original implementation as a reference and checks the two agree.

use serde_json::{Map, Value};

use crate::xml::types::XmlElement;

fn merge_element_content(target: &mut Map<String, Value>, source: Map<String, Value>) {
    for (key, value) in source {
        match value {
            Value::Array(items) => merge_array_value(target, key, items),
            Value::Object(map) => merge_object_value(target, key, map),
            primitive => merge_primitive_value(target, key, primitive),
        }
    }
}

fn merge_array_value(target: &mut Map<String, Value>, key: String, items: Vec<Value>) {
    if !target.contains_key(&key) {
        target.insert(key, Value::Array(items));
    } else if let Some(Value::Array(arr)) = target.get_mut(&key) {
        arr.extend(items);
    } else {
        let existing = target.remove(&key).unwrap();
        let mut combined = Vec::with_capacity(items.len() + 1);
        combined.push(existing);
        combined.extend(items);
        target.insert(key, Value::Array(combined));
    }
}

fn merge_object_value(target: &mut Map<String, Value>, key: String, map: Map<String, Value>) {
    match target.get_mut(&key) {
        Some(Value::Array(arr)) => arr.push(Value::Object(map)),
        // Replacing the value of an existing key keeps the key's position, like
        // inserting over an existing key does.
        Some(existing) => {
            let previous = std::mem::take(existing);
            *existing = Value::Array(vec![previous, Value::Object(map)]);
        }
        None => {
            target.insert(key, Value::Object(map));
        }
    }
}

fn merge_primitive_value(target: &mut Map<String, Value>, key: String, value: Value) {
    if !target.contains_key(&key) {
        target.insert(key, value);
    }
}

fn default_xml_declaration() -> Value {
    let mut decl = Map::new();
    decl.insert("@version".to_string(), Value::String("1.0".to_string()));
    decl.insert("@encoding".to_string(), Value::String("UTF-8".to_string()));
    Value::Object(decl)
}

fn build_final_xml_element(
    declaration: Option<Value>,
    root_key: &str,
    content: Map<String, Value>,
) -> XmlElement {
    let mut result = Map::new();
    let decl = declaration.unwrap_or_else(default_xml_declaration);
    result.insert("?xml".to_string(), decl);
    result.insert(root_key.to_string(), Value::Object(content));
    Value::Object(result)
}

/// Reorder the root element's child keys to match the given order.
/// Keys not in `key_order` are appended at the end.
///
/// Returns `None` when `element` isn't an object with a non-`?xml` root key whose
/// value is an object. See [`reorder_root_keys_owned`] to avoid the copy.
pub fn reorder_root_keys(element: &XmlElement, key_order: &[String]) -> Option<XmlElement> {
    let obj = element.as_object()?;
    let root_key = obj.keys().find(|k| *k != "?xml")?;
    obj.get(root_key)?.as_object()?;
    Some(reorder_root_keys_owned(element.clone(), key_order))
}

/// Owned version of [`reorder_root_keys`]: moves the root's children into their
/// new order instead of cloning them. An `element` without the expected shape is
/// returned unchanged (the case where [`reorder_root_keys`] returns `None`).
pub fn reorder_root_keys_owned(element: XmlElement, key_order: &[String]) -> XmlElement {
    let mut obj = match element {
        Value::Object(obj) => obj,
        other => return other,
    };
    let Some(root_key) = obj.keys().find(|k| *k != "?xml").cloned() else {
        return Value::Object(obj);
    };
    let mut root_content = match obj.get_mut(&root_key) {
        Some(Value::Object(content)) => std::mem::take(content),
        _ => return Value::Object(obj),
    };

    let mut reordered = Map::new();
    for key in key_order {
        // A key listed twice keeps its first position (re-inserting an existing
        // key never moves it), so only the first occurrence needs taking.
        if !reordered.contains_key(key) {
            if let Some(value) = root_content.get_mut(key) {
                reordered.insert(key.clone(), std::mem::take(value));
            }
        }
    }
    for (key, value) in root_content {
        if !reordered.contains_key(&key) {
            reordered.insert(key, value);
        }
    }

    let mut result = Map::new();
    if let Some(decl) = obj.get_mut("?xml") {
        result.insert("?xml".to_string(), std::mem::take(decl));
    }
    result.insert(root_key, Value::Object(reordered));
    Value::Object(result)
}

/// Merge multiple XML elements into one.
///
/// Scans elements for the first non-`?xml` root key; tolerates leading entries
/// that are empty or declaration-only (e.g. a partially-written disassembled file),
/// which previously caused the whole merge to return `None` and the caller to
/// emit an empty `<root>` document. See [`merge_xml_elements_owned`] to avoid the copy.
pub fn merge_xml_elements(elements: &[XmlElement]) -> Option<XmlElement> {
    merge_xml_elements_owned(elements.to_vec())
}

/// Owned version of [`merge_xml_elements`]: consumes the parsed elements and
/// moves their content into the merged tree instead of cloning it.
pub fn merge_xml_elements_owned(elements: Vec<XmlElement>) -> Option<XmlElement> {
    if elements.is_empty() {
        log::error!("No elements to merge.");
        return None;
    }

    let root_key = elements
        .iter()
        .find_map(|el| el.as_object()?.keys().find(|k| *k != "?xml").cloned())?;

    let mut declaration: Option<Value> = None;
    let mut merged_content = Map::new();
    for element in elements {
        let Value::Object(mut obj) = element else {
            continue;
        };
        // First element carrying a declaration wins.
        if declaration.is_none() {
            if let Some(decl) = obj.get_mut("?xml") {
                declaration = Some(std::mem::take(decl));
            }
        }
        if let Some(Value::Object(content)) = obj.get_mut(&root_key) {
            merge_element_content(&mut merged_content, std::mem::take(content));
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

    // ---- reference implementation -------------------------------------------
    // The original clone-based merge / reorder, kept as the oracle for the owned
    // versions above.
    mod reference {
        use serde_json::{Map, Value};

        fn merge_element_content(target: &mut Map<String, Value>, source: &Map<String, Value>) {
            for (key, value) in source {
                if value.is_array() {
                    merge_array_value(target, key, value.as_array().unwrap());
                } else if value.is_object() {
                    merge_object_value(target, key, value.as_object().unwrap());
                } else if !target.contains_key(key) {
                    target.insert(key.to_string(), value.clone());
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

        fn merge_object_value(
            target: &mut Map<String, Value>,
            key: &str,
            value: &Map<String, Value>,
        ) {
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

        pub fn merge(elements: &[Value]) -> Option<Value> {
            if elements.is_empty() {
                return None;
            }
            let root_key = elements
                .iter()
                .find_map(|el| el.as_object()?.keys().find(|k| *k != "?xml").cloned())?;
            let declaration = elements.iter().find_map(|el| el.as_object()?.get("?xml"));
            let mut merged = Map::new();
            for element in elements {
                if let Some(content) = element
                    .as_object()
                    .and_then(|o| o.get(&root_key))
                    .and_then(Value::as_object)
                {
                    merge_element_content(&mut merged, content);
                }
            }
            let mut result = Map::new();
            let decl = declaration
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"@version": "1.0", "@encoding": "UTF-8"}));
            result.insert("?xml".to_string(), decl);
            result.insert(root_key, Value::Object(merged));
            Some(Value::Object(result))
        }

        pub fn reorder(element: &Value, key_order: &[String]) -> Option<Value> {
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
    }

    /// Serialised form: `Value`'s `==` ignores map key order, so compare strings.
    fn ordered(value: &Option<Value>) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn owned_merge_matches_reference_including_key_order() {
        let cases: Vec<Vec<Value>> = vec![
            // Scalar then array for the same key: the swap-remove branch, which
            // moves keys -- the order the owned version must reproduce.
            vec![
                json!({"R": {"a": "1", "b": "2", "c": "3"}}),
                json!({"R": {"a": ["x", "y"], "d": "4"}}),
            ],
            // Object then object: existing key keeps its position.
            vec![
                json!({"?xml": {"@version": "1.0"}, "R": {"o": {"k": 1}, "z": "z"}}),
                json!({"R": {"o": {"k": 2}, "p": "p"}}),
                json!({"R": {"o": {"k": 3}}}),
            ],
            // Array accumulation, primitive first-wins, mixed shapes.
            vec![
                json!({"R": {"items": [{"n": 1}], "label": "first"}}),
                json!({"R": {"items": [{"n": 2}, {"n": 3}], "label": "second"}}),
                json!({"R": {"items": {"n": 4}}}),
            ],
            // Leading declaration-only / empty / foreign-root / non-object entries.
            vec![
                json!({"?xml": {"@version": "1.0", "@encoding": "UTF-8"}}),
                json!({}),
                json!("not an object"),
                json!({"Other": {"x": 1}}),
                json!({"R": {"x": "kept"}}),
                json!({"?xml": {"@version": "ignored"}, "R": {"y": "also"}}),
            ],
            // Root content that isn't an object is skipped.
            vec![json!({"R": "text"}), json!({"R": {"a": 1}})],
        ];
        for (i, elements) in cases.iter().enumerate() {
            let expected = ordered(&reference::merge(elements));
            assert_eq!(
                ordered(&merge_xml_elements_owned(elements.clone())),
                expected,
                "case {i}"
            );
            assert_eq!(
                ordered(&merge_xml_elements(elements)),
                expected,
                "borrowing wrapper, case {i}"
            );
        }
    }

    #[test]
    fn owned_reorder_matches_reference_including_key_order() {
        let order = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        let doc = json!({
            "?xml": {"@version": "1.0"},
            "R": {"c": 3, "a": [1, 2], "b": {"x": 1}, "d": "4"}
        });
        let cases = [
            order(&["a", "b", "c", "d"]),
            order(&["d", "missing", "a"]),
            order(&["b", "b", "a"]), // duplicate key keeps its first position
            order(&[]),
        ];
        for key_order in &cases {
            let expected = ordered(&reference::reorder(&doc, key_order));
            assert_eq!(
                ordered(&reorder_root_keys(&doc, key_order)),
                expected,
                "{key_order:?}"
            );
            assert_eq!(
                ordered(&Some(reorder_root_keys_owned(doc.clone(), key_order))),
                expected,
                "owned {key_order:?}"
            );
        }
    }

    #[test]
    fn reorder_owned_returns_unexpected_shapes_unchanged() {
        let key_order = vec!["a".to_string()];
        for value in [
            json!("text"),
            json!({}),
            json!({"?xml": {"@version": "1.0"}}),
            json!({"R": "not an object"}),
        ] {
            assert!(reorder_root_keys(&value, &key_order).is_none(), "{value}");
            assert_eq!(reorder_root_keys_owned(value.clone(), &key_order), value);
        }
    }

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
