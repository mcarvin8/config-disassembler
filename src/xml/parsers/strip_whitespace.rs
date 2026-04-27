//! Strip meaningless whitespace-only #text nodes from parsed XML structure.

use serde_json::{Map, Value};

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
}
