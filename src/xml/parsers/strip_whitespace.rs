//! Strip meaningless whitespace-only #text nodes from parsed XML structure.

use serde_json::{Map, Value};

/// Keys whose mere presence means an element had real text/comment/CDATA content
/// between its tags -- disqualifying it from `mark_compact_elements`'s "wrapper with
/// zero surrounding whitespace" check regardless of the key's value.
const CONTENT_KEYS: [&str; 4] = ["#text", "#comment", "#text-tail", "#cdata"];

fn is_meta_key(key: &str) -> bool {
    key.starts_with('#') || key.starts_with('@') || key == "?xml"
}

/// Mark elements written as a single-line "compact" wrapper around exactly one nested
/// child element, with zero whitespace between the wrapper's start tag, the child, and
/// the wrapper's end tag -- e.g. Salesforce Flow's
/// `<connector><targetReference>X</targetReference></connector>` idiom. Marks by
/// inserting `"#compact": true` on the wrapper element itself; `build_xml_string`
/// consumes and strips it to render the wrapper and its single child on one line.
///
/// Must run on the freshly parsed tree *before* [`strip_whitespace_text_nodes`], which
/// removes whitespace-only `#text`/`#cdata`/`#text-tail` this depends on: once removed,
/// "never had whitespace" (compact) and "had whitespace, now stripped" (block) are
/// indistinguishable from the object alone.
///
/// Does not recurse into `Value::String`/`Number`/etc; call on each of the document
/// root's own child values (not the root wrapper itself, which always has exactly one
/// key -- the root element -- and would otherwise always be marked compact).
pub fn mark_compact_elements(node: &mut Value) {
    match node {
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                mark_compact_elements(item);
            }
        }
        Value::Object(obj) => {
            for value in obj.values_mut() {
                mark_compact_elements(value);
            }

            let has_content_key = obj.keys().any(|k| CONTENT_KEYS.contains(&k.as_str()));
            let mut element_key_count = 0;
            let mut sole_child_is_single_element = false;
            for (key, value) in obj.iter() {
                if !is_meta_key(key) {
                    element_key_count += 1;
                    sole_child_is_single_element = !matches!(value, Value::Array(_));
                }
            }

            if !has_content_key && element_key_count == 1 && sole_child_is_single_element {
                obj.insert("#compact".to_string(), Value::Bool(true));
            }
        }
        _ => {}
    }
}

fn is_empty_text_node(key: &str, value: &Value) -> bool {
    (key == "#text" || key == "#cdata" || key == "#text-tail")
        && value.as_str().map(|s| s.trim().is_empty()).unwrap_or(false)
}

fn clean_array(arr: &[Value]) -> Vec<Value> {
    arr.iter()
        .filter_map(|entry| {
            let cleaned = strip_whitespace_text_nodes(entry);
            match &cleaned {
                Value::Object(m) if m.is_empty() => None,
                _ => Some(cleaned),
            }
        })
        .collect()
}

fn clean_object(obj: &Map<String, Value>) -> Map<String, Value> {
    let mut result = Map::new();
    let has_cdata = obj.contains_key("#cdata");
    let has_comment = obj.contains_key("#comment");
    for (key, value) in obj {
        // Preserve whitespace-only #text when element has #cdata (needed for round-trip)
        // Preserve whitespace-only #text and #text-tail when element has #comment
        if is_empty_text_node(key, value)
            && !(key == "#text" && has_cdata)
            && !(key == "#text" && has_comment)
            && !(key == "#text-tail" && has_comment)
        {
            continue;
        }
        let cleaned = strip_whitespace_text_nodes(value);
        if !cleaned.is_null()
            || key == "#text"
            || key == "#cdata"
            || key == "#comment"
            || key == "#text-tail"
        {
            result.insert(key.clone(), cleaned);
        }
    }
    result
}

/// Remove meaningless whitespace-only #text nodes from the XML structure.
pub fn strip_whitespace_text_nodes(node: &Value) -> Value {
    match node {
        Value::Array(arr) => Value::Array(clean_array(arr)),
        Value::Object(obj) => Value::Object(clean_object(obj)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_empty_text_nodes_from_array() {
        let input = json!([{ "#text": "   " }, { "#text": "keep me" }]);
        let result = strip_whitespace_text_nodes(&input);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0].get("#text").and_then(|v| v.as_str()),
            Some("keep me")
        );
    }

    #[test]
    fn preserves_non_empty_text() {
        let input = json!({ "#text": "  content  " });
        let result = strip_whitespace_text_nodes(&input);
        assert_eq!(
            result.get("#text").and_then(|v| v.as_str()),
            Some("  content  ")
        );
    }

    #[test]
    fn leaves_primitive_unchanged() {
        let input = json!("hello");
        let result = strip_whitespace_text_nodes(&input);
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn preserves_empty_text_when_element_has_cdata() {
        let input = json!({ "#cdata": "content", "#text": "   " });
        let result = strip_whitespace_text_nodes(&input);
        let obj = result.as_object().unwrap();
        assert_eq!(obj.get("#cdata").and_then(|v| v.as_str()), Some("content"));
        assert_eq!(obj.get("#text").and_then(|v| v.as_str()), Some("   "));
    }

    #[test]
    fn preserves_null_special_keys() {
        let input = json!({ "#text": null });
        let result = strip_whitespace_text_nodes(&input);
        assert!(result.get("#text").map(|v| v.is_null()) == Some(true));
    }

    #[test]
    fn strips_whitespace_only_cdata_node() {
        // Whitespace-only #cdata (without a sibling preservation trigger) should be dropped,
        // mirroring the existing #text behavior. Guards is_empty_text_node's #cdata branch.
        let input = json!([{ "#cdata": "   " }, { "#text": "keep me" }]);
        let result = strip_whitespace_text_nodes(&input);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0].get("#text").and_then(|v| v.as_str()),
            Some("keep me")
        );
    }

    #[test]
    fn strips_whitespace_only_text_tail_node() {
        // Whitespace-only #text-tail without a #comment sibling should be stripped.
        // Guards both is_empty_text_node's #text-tail branch and the line-32 guard in clean_object.
        let input = json!([{ "#text-tail": "   " }, { "#text": "keep me" }]);
        let result = strip_whitespace_text_nodes(&input);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0].get("#text").and_then(|v| v.as_str()),
            Some("keep me")
        );
    }

    #[test]
    fn preserves_whitespace_text_tail_when_element_has_comment() {
        // Mirror of preserves_empty_text_when_element_has_cdata for #text-tail + #comment.
        // Documents the line-32 guard that keeps whitespace #text-tail alive next to comments.
        let input = json!({ "#comment": "note", "#text-tail": "   " });
        let result = strip_whitespace_text_nodes(&input);
        let obj = result.as_object().unwrap();
        assert_eq!(obj.get("#comment").and_then(|v| v.as_str()), Some("note"));
        assert_eq!(obj.get("#text-tail").and_then(|v| v.as_str()), Some("   "));
    }

    #[test]
    fn preserves_null_cdata_comment_and_text_tail_keys() {
        // Special keys #cdata, #comment, #text-tail are kept even when value is null (insert branch)
        let input = json!({
            "#cdata": null,
            "#comment": null,
            "#text-tail": null,
            "a": "b"
        });
        let result = strip_whitespace_text_nodes(&input);
        let obj = result.as_object().unwrap();
        assert!(obj.get("#cdata").map(|v| v.is_null()) == Some(true));
        assert!(obj.get("#comment").map(|v| v.is_null()) == Some(true));
        assert!(obj.get("#text-tail").map(|v| v.is_null()) == Some(true));
        assert_eq!(obj.get("a").and_then(|v| v.as_str()), Some("b"));
    }

    #[test]
    fn mark_compact_elements_marks_sole_element_child_with_no_whitespace() {
        // `<connector><targetReference>X</targetReference></connector>` -- parsed with
        // no #text on `connector` at all (no whitespace ever occurred between tags).
        let mut input = json!({
            "connector": { "targetReference": { "#text": "X" } }
        });
        mark_compact_elements(&mut input);
        let connector = input.get("connector").and_then(|v| v.as_object()).unwrap();
        assert_eq!(connector.get("#compact"), Some(&Value::Bool(true)));
    }

    #[test]
    fn mark_compact_elements_recurses_into_array_items() {
        // `Value::Array` must recurse into each item, not just skip past the array --
        // otherwise a compact wrapper nested inside a repeated sibling element (very
        // common: each shard/array item is its own subtree) would never get marked.
        let mut input = json!({
            "items": [
                { "wrapper": { "child": { "#text": "1" } } },
                { "unrelated": { "#text": "2" } }
            ]
        });
        mark_compact_elements(&mut input);
        let items = input.get("items").and_then(|v| v.as_array()).unwrap();
        let wrapper = items[0].get("wrapper").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            wrapper.get("#compact"),
            Some(&Value::Bool(true)),
            "wrapper nested inside an array item must still be marked compact"
        );
    }

    #[test]
    fn mark_compact_elements_treats_hash_prefixed_marker_as_meta_not_element() {
        // Isolates is_meta_key's `key.starts_with('#')` clause: a `#`-prefixed key that
        // is NOT one of CONTENT_KEYS (so has_content_key stays false) and is NOT the
        // literal "#compact" marker itself (which would make the assertion vacuously
        // true regardless of is_meta_key's behavior) must still be excluded from the
        // element count, or a wrapper carrying it alongside its one real child would be
        // miscounted as having two children and never marked.
        let mut input = json!({
            "wrapper": { "#some-other-marker": "ignored", "child": { "#text": "1" } }
        });
        mark_compact_elements(&mut input);
        let wrapper = input.get("wrapper").and_then(|v| v.as_object()).unwrap();
        assert_eq!(wrapper.get("#compact"), Some(&Value::Bool(true)));
    }

    #[test]
    fn mark_compact_elements_treats_xml_declaration_key_as_meta_not_element() {
        // Isolates is_meta_key's `key == "?xml"` clause: even though mark_compact_elements
        // is only ever invoked one level below the document root in practice (never on an
        // object that itself carries "?xml"), is_meta_key's own contract must still exclude
        // it from the element count wherever it appears.
        let mut input = json!({
            "wrapper": { "?xml": { "@version": "1.0" }, "child": { "#text": "1" } }
        });
        mark_compact_elements(&mut input);
        let wrapper = input.get("wrapper").and_then(|v| v.as_object()).unwrap();
        assert_eq!(wrapper.get("#compact"), Some(&Value::Bool(true)));
    }

    #[test]
    fn mark_compact_elements_does_not_mark_block_formatted_wrapper() {
        // Same shape, but whitespace was present (block-formatted in source) --
        // `connector` carries a whitespace-only #text from the surrounding newlines.
        let mut input = json!({
            "connector": {
                "#text": "\n        ",
                "targetReference": { "#text": "X" }
            }
        });
        mark_compact_elements(&mut input);
        let connector = input.get("connector").and_then(|v| v.as_object()).unwrap();
        assert!(connector.get("#compact").is_none());
    }

    #[test]
    fn mark_compact_elements_recurses_into_nested_children() {
        // The nested `value` wrapper should be marked even though `decisions` itself
        // (with its many real fields) never qualifies as a compact wrapper.
        let mut input = json!({
            "decisions": {
                "name": { "#text": "Decision_0001" },
                "value": { "stringValue": { "#text": "Match" } }
            }
        });
        mark_compact_elements(&mut input);
        let decisions = input.get("decisions").and_then(|v| v.as_object()).unwrap();
        assert!(
            decisions.get("#compact").is_none(),
            "element with multiple children must never be marked compact"
        );
        let value = decisions.get("value").and_then(|v| v.as_object()).unwrap();
        assert_eq!(value.get("#compact"), Some(&Value::Bool(true)));
    }

    #[test]
    fn mark_compact_elements_ignores_array_valued_sole_key() {
        // A repeated sibling tag collapses to an Array even when it is the only key
        // present; that must never be treated as a single-element wrapper.
        let mut input = json!({
            "parent": { "item": [{ "#text": "1" }, { "#text": "2" }] }
        });
        mark_compact_elements(&mut input);
        let parent = input.get("parent").and_then(|v| v.as_object()).unwrap();
        assert!(parent.get("#compact").is_none());
    }

    #[test]
    fn mark_compact_elements_ignores_element_with_attributes_and_text() {
        // An element with both an attribute and real text content is a leaf, not a
        // wrapper -- must never be marked regardless of key count.
        let mut input = json!({
            "field": { "@type": "string", "#text": "value" }
        });
        mark_compact_elements(&mut input);
        let field = input.get("field").and_then(|v| v.as_object()).unwrap();
        assert!(field.get("#compact").is_none());
    }

    #[test]
    fn mark_compact_elements_leaves_primitives_and_empty_containers_unchanged() {
        let mut s = json!("hello");
        mark_compact_elements(&mut s);
        assert_eq!(s, json!("hello"));

        let mut empty_obj = json!({});
        mark_compact_elements(&mut empty_obj);
        assert_eq!(empty_obj, json!({}));

        let mut empty_arr = json!([]);
        mark_compact_elements(&mut empty_arr);
        assert_eq!(empty_arr, json!([]));
    }
}
