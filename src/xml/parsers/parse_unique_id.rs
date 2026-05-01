//! Parse unique ID from XML element for file naming.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::xml::types::XmlElement;

/// Hash the full canonicalized JSON form of an element to derive an 8-char
/// filename. SHA-256 over distinct content yields distinct prefixes with
/// vanishingly small collision probability for normal sibling counts.
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

/// True only for objects that have at least one element-name child. quick-xml
/// represents leaf scalars (and attribute-only nodes) as `{ "#text": "..." }` /
/// `{ "@attr": "...", "#text": "..." }`; those are *not* recursable - if we
/// recurse into them we end up hashing the same single text-leaf child for
/// every sibling that happens to start with the same scalar element, which
/// silently collapses distinct siblings into one filename.
fn is_recursable_object(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    obj.iter()
        .any(|(k, _)| !k.starts_with('#') && !k.starts_with('@'))
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

/// Search for a configured unique-id field anywhere in the subtree rooted at
/// `element`. Returns `Some(id)` only when a configured field is *actually*
/// matched; returns `None` when nothing matches so the caller can fall back to
/// hashing the *outer* element rather than a single inner child.
fn find_id_in_subtree(element: &XmlElement, unique_id_elements: &str) -> Option<String> {
    let field_names: Vec<&str> = unique_id_elements.split(',').map(|s| s.trim()).collect();
    if let Some(direct) = find_direct_field_match(element, &field_names) {
        return Some(direct);
    }
    let obj = element.as_object()?;
    for (_, child) in obj {
        if !is_recursable_object(child) {
            continue;
        }
        if let Some(found) = find_id_in_subtree(child, unique_id_elements) {
            return Some(found);
        }
    }
    None
}

/// Get a unique ID for an element, using configured fields or a hash of the
/// *outer* element when no configured field exists in the subtree.
///
/// Hashing must be performed on the outer element (not on whatever inner
/// child the search happened to visit first) so siblings whose first nested
/// child shares a value - e.g. a list of `<actionOverrides>` that all start
/// with `<actionName>View</actionName>` - still produce distinct filenames
/// reflecting their distinct content.
pub fn parse_unique_id_element(element: &XmlElement, unique_id_elements: Option<&str>) -> String {
    if let Some(ids) = unique_id_elements {
        find_id_in_subtree(element, ids).unwrap_or_else(|| create_short_hash(element))
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

    // ---- regression: text-leaf siblings must NOT collapse to one hash ------

    /// Models a `<CustomApplication>`'s `<actionOverrides>`: every block has
    /// the same `<actionName>View</actionName>` first child but distinct
    /// `<content>` and `<pageOrSobjectType>` payloads. With the old
    /// implementation the recursion landed on `{"#text":"View"}` for every
    /// sibling and they all hashed to the same 8-char prefix, silently
    /// collapsing 100s of overrides into a single shard that contained only
    /// the last one written.
    #[test]
    fn distinct_siblings_with_shared_first_text_leaf_get_distinct_hashes() {
        let make_action_override = |i: u32| -> XmlElement {
            json!({
                "actionName": { "#text": "View" },
                "comment": { "#text": format!("Action override {i}") },
                "content": { "#text": format!("Sample_Page_{i:05}") },
                "formFactor": { "#text": "Large" },
                "skipRecordTypeSelect": { "#text": "false" },
                "type": { "#text": "Flexipage" },
                "pageOrSobjectType": { "#text": format!("Sample_Object_{i:03}__c") }
            })
        };

        // Default unique-id elements ("fullName,name") - none of these are
        // present on actionOverride children.
        let ids = Some("fullName,name");

        let mut seen = std::collections::HashSet::new();
        for i in 1..=128 {
            let id = parse_unique_id_element(&make_action_override(i), ids);
            assert_eq!(id.len(), 8, "expected an 8-char short hash, got {id}");
            assert!(
                seen.insert(id.clone()),
                "duplicate hash {id} for actionOverride {i} - distinct siblings collapsed"
            );
        }
    }

    /// Same shape but with no unique-id config at all: must also produce
    /// distinct hashes per sibling.
    #[test]
    fn distinct_siblings_get_distinct_hashes_with_no_unique_id_config() {
        let mut seen = std::collections::HashSet::new();
        for i in 1..=64 {
            let el = json!({
                "actionName": { "#text": "View" },
                "content": { "#text": format!("Page_{i}") }
            });
            let id = parse_unique_id_element(&el, None);
            assert!(
                seen.insert(id.clone()),
                "duplicate hash {id} at index {i} with no unique-id config"
            );
        }
    }

    /// `find_id_in_subtree` must skip text-leaf wrappers like
    /// `{"#text": "..."}` rather than treat them as recursable objects.
    /// Otherwise the search returns a hash of the inner wrapper rather than
    /// hashing the outer element.
    #[test]
    fn text_leaf_wrappers_are_not_recursable() {
        let leaf = json!({ "#text": "View" });
        assert!(!is_recursable_object(&leaf));

        let attrs_only = json!({ "@attr": "x", "#text": "y" });
        assert!(!is_recursable_object(&attrs_only));

        let real = json!({ "name": "x" });
        assert!(is_recursable_object(&real));

        let mixed = json!({ "@attr": "x", "name": "y" });
        assert!(is_recursable_object(&mixed));
    }

    /// Recursion must only return when a configured unique-id field is
    /// *actually* found, not when a recursive call falls back to its own
    /// hash. The hash is computed exactly once, at the top level, on the
    /// outer element.
    #[test]
    fn nested_search_does_not_return_inner_hash() {
        // Two distinct outer elements whose first recursable child has the
        // same shape. With the old behavior the recursion would compute a
        // hash of that inner child for both - same hash for distinct outers.
        // With the fix, each outer is hashed in full and they differ.
        let a = json!({
            "wrapper": { "leafA": "shared", "extraA": "different-A" },
            "outerA": "A"
        });
        let b = json!({
            "wrapper": { "leafA": "shared", "extraA": "different-A" },
            "outerB": "B"
        });
        let id_a = parse_unique_id_element(&a, Some("name"));
        let id_b = parse_unique_id_element(&b, Some("name"));
        assert_ne!(id_a, id_b);
    }
}
