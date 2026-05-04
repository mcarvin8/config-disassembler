//! Parse unique ID from XML element for file naming.
//!
//! ## Configuration syntax
//!
//! `unique_id_elements` is a comma-separated list of *candidates*; the first
//! candidate that fully resolves against an element wins. Each candidate is
//! either:
//!
//! * a single field name (e.g. `fullName`) - matches when that field is
//!   present anywhere in the element's subtree, or
//! * a `+`-joined **compound** of two or more field names (e.g.
//!   `actionName+pageOrSobjectType+formFactor`) - matches only when *every*
//!   sub-field resolves at the same level, in which case the resolved
//!   values are joined with [`COMPOUND_VALUE_SEPARATOR`] (`__`).
//!
//! Compounds let metadata types like `<profileActionOverrides>` - whose
//! natural unique key is `actionName + pageOrSobjectType + formFactor +
//! profile [+ recordType]` - produce stable, readable filenames instead of
//! collapsing every sibling into a SHA-256 fallback. Listing both the wide
//! and narrow forms (`A+B+C+D, A+B+C, A`) gives a graceful fallback chain
//! when an item only carries some of the keys.
//!
//! Backwards compatibility: any spec that contains no `+` is parsed as a
//! list of single-field candidates and behaves identically to releases
//! prior to compound-key support.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::xml::types::XmlElement;

/// Separator inserted between resolved values when a compound candidate
/// matches. Picked because filenames are filesystem-safe everywhere and
/// because individual Salesforce identifier names rarely contain the
/// double-underscore (single `_` is common - e.g. `Account_Name__c` - so
/// a single underscore would round-trip ambiguously when values themselves
/// already contain `_`).
const COMPOUND_VALUE_SEPARATOR: &str = "__";

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

/// Parse the user-supplied spec into a list of candidates, where each
/// candidate is itself a list of field names. A candidate of length 1 is a
/// plain single-field match (legacy behaviour); length >= 2 is a compound.
///
/// Empty entries (from leading/trailing commas, double commas, or stray `+`
/// separators) are filtered so a copy-pasted spec like `, name ,, +foo+ ,`
/// degrades to `[["name"], ["foo"]]` rather than panicking on empty lookups.
fn parse_candidates(spec: &str) -> Vec<Vec<&str>> {
    spec.split(',')
        .map(|candidate| {
            candidate
                .split('+')
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .collect::<Vec<&str>>()
        })
        .filter(|fields| !fields.is_empty())
        .collect()
}

/// Match a single candidate against the element's *direct* fields. A
/// single-field candidate succeeds when the field is present and resolves
/// to a non-empty string; a compound candidate succeeds only when every
/// sub-field is present and non-empty, in which case the resolved values
/// are joined with [`COMPOUND_VALUE_SEPARATOR`].
///
/// Restricting compounds to the same level keeps the semantics intuitive:
/// `actionName+profile+recordType` describes a single record's shape, not
/// a search for those tokens scattered across the subtree.
fn match_candidate_at_direct(element: &XmlElement, fields: &[&str]) -> Option<String> {
    let obj = element.as_object()?;
    let mut parts: Vec<String> = Vec::with_capacity(fields.len());
    for field in fields {
        let value = obj.get(*field).and_then(value_as_string)?;
        if value.is_empty() {
            return None;
        }
        parts.push(value);
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(COMPOUND_VALUE_SEPARATOR))
}

/// Search for a configured unique-id candidate anywhere in the subtree
/// rooted at `element`. Returns `Some(id)` only when a candidate fully
/// resolves; returns `None` so the caller can fall back to hashing the
/// *outer* element rather than a single inner child.
///
/// Order of evaluation:
/// 1. Try every candidate against the direct fields of `element` (so a
///    direct match always beats a deeper one - preserves the priority that
///    callers configuring `fullName,name` historically relied on).
/// 2. If nothing matched, recurse into recursable children and repeat.
fn find_id_in_subtree(element: &XmlElement, unique_id_elements: &str) -> Option<String> {
    let candidates = parse_candidates(unique_id_elements);
    if candidates.is_empty() {
        return None;
    }
    for candidate in &candidates {
        if let Some(id) = match_candidate_at_direct(element, candidate) {
            return Some(id);
        }
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

    // ---- compound-key support ----------------------------------------------

    /// A `<profileActionOverrides>` element with the full key set. The
    /// compound `actionName+pageOrSobjectType+formFactor+profile` must
    /// resolve to all four values joined with `__`.
    #[test]
    fn compound_resolves_when_all_fields_present() {
        let el = json!({
            "actionName": { "#text": "Tab" },
            "content": { "#text": "Home_Page_Default" },
            "formFactor": { "#text": "Large" },
            "pageOrSobjectType": { "#text": "standard-home" },
            "type": { "#text": "Flexipage" },
            "profile": { "#text": "Implementation_Lightning" }
        });
        let id =
            parse_unique_id_element(&el, Some("actionName+pageOrSobjectType+formFactor+profile"));
        assert_eq!(id, "Tab__standard-home__Large__Implementation_Lightning");
    }

    /// A compound that names a field the element doesn't have must NOT
    /// match - the next candidate (a narrower compound, then a single
    /// field) takes over.
    #[test]
    fn compound_falls_through_when_one_field_missing() {
        // `<actionOverrides>` (no profile, no recordType) - the wide compound
        // must fail, the narrow compound must succeed.
        let el = json!({
            "actionName": { "#text": "View" },
            "content": { "#text": "LUX_Case_Release_Candidate_Copy" },
            "formFactor": { "#text": "Large" },
            "pageOrSobjectType": { "#text": "Case" },
            "type": { "#text": "Flexipage" }
        });
        let spec = "actionName+pageOrSobjectType+formFactor+profile,actionName+pageOrSobjectType+formFactor,actionName";
        assert_eq!(
            parse_unique_id_element(&el, Some(spec)),
            "View__Case__Large"
        );
    }

    /// All compound candidates miss → the loop must fall back to the
    /// single-field candidate at the tail of the spec, and ultimately to
    /// the outer-element hash if even that misses.
    #[test]
    fn compound_then_single_then_hash_fallback() {
        let el = json!({
            "actionName": { "#text": "View" }
        });
        let spec_all_compound =
            "actionName+pageOrSobjectType+formFactor+profile,actionName+pageOrSobjectType";
        let id = parse_unique_id_element(&el, Some(spec_all_compound));
        assert_eq!(
            id.len(),
            8,
            "no candidate should match → hash fallback, got {id}"
        );

        let spec_with_single_tail = "actionName+pageOrSobjectType+formFactor,actionName";
        assert_eq!(
            parse_unique_id_element(&el, Some(spec_with_single_tail)),
            "View"
        );
    }

    /// Empty values (`<recordType></recordType>`) must be treated as
    /// missing for the purpose of compound matching - otherwise we would
    /// emit filenames like `View__Account__Large__` with a trailing
    /// separator and silently collide with siblings that genuinely lack
    /// the field.
    #[test]
    fn compound_treats_empty_values_as_missing() {
        let el = json!({
            "actionName": { "#text": "View" },
            "pageOrSobjectType": { "#text": "Account" },
            "recordType": { "#text": "" }  // explicitly empty
        });
        let spec = "actionName+pageOrSobjectType+recordType,actionName+pageOrSobjectType";
        assert_eq!(
            parse_unique_id_element(&el, Some(spec)),
            "View__Account",
            "empty <recordType> must be treated as missing"
        );
    }

    /// Distinct profileActionOverrides siblings sharing actionName +
    /// pageOrSobjectType + formFactor but differing in `profile` must
    /// produce distinct compound IDs (not collide).
    #[test]
    fn compound_disambiguates_siblings_that_share_outer_fields() {
        let make = |profile: &str| {
            json!({
                "actionName": { "#text": "Tab" },
                "content": { "#text": "Home_Page_Default" },
                "formFactor": { "#text": "Large" },
                "pageOrSobjectType": { "#text": "standard-home" },
                "type": { "#text": "Flexipage" },
                "profile": { "#text": profile }
            })
        };
        let spec = "actionName+pageOrSobjectType+formFactor+profile";
        let a = parse_unique_id_element(&make("Implementation_Lightning"), Some(spec));
        let b = parse_unique_id_element(&make("Sales_Lightning"), Some(spec));
        assert_ne!(a, b);
        assert!(a.ends_with("Implementation_Lightning"));
        assert!(b.ends_with("Sales_Lightning"));
    }

    /// A single-field spec must behave identically to releases prior to
    /// compound-key support: same priority (direct first, then nested),
    /// same hash fallback, no spurious `__` separators.
    #[test]
    fn single_field_behaviour_is_unchanged() {
        let el = json!({ "name": "Get_Info", "label": "Get Info" });
        assert_eq!(parse_unique_id_element(&el, Some("name")), "Get_Info");

        // Direct vs nested priority preserved.
        let nested = json!({
            "wrapper": { "name": "NestedName" }
        });
        assert_eq!(parse_unique_id_element(&nested, Some("name")), "NestedName");
    }

    /// Pathological/malformed specs - leading commas, stray `+`, all
    /// whitespace - must not panic and must degrade to hash fallback.
    #[test]
    fn malformed_spec_degrades_to_hash() {
        let el = json!({ "foo": "bar" });
        let id = parse_unique_id_element(&el, Some(",,+,, "));
        assert_eq!(id.len(), 8, "all-empty candidates → hash fallback");
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
