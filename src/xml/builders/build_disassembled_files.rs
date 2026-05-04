//! Build disassembled files from source XML file.

use crate::xml::builders::{build_disassembled_file, extract_root_attributes};
use crate::xml::parsers::{
    extract_xml_declaration_from_raw, parse_element_unified, parse_unique_id_element,
    short_hash_for_element,
};
use crate::xml::types::{
    BuildDisassembledFilesOptions, DecomposeRule, XmlElementArrayMap, XmlElementParams,
};
use crate::xml::utils::normalize_path_unix;
use serde_json::{Map, Value};
use std::collections::HashMap;
use tokio::fs;

const BATCH_SIZE: usize = 20;

fn get_root_info(parsed_xml: &Value) -> Option<(String, Value)> {
    let obj = parsed_xml.as_object()?;
    let root_element_name = obj.keys().find(|k| *k != "?xml")?.clone();
    let root_element = obj.get(&root_element_name)?.clone();
    Some((root_element_name, root_element))
}

fn order_xml_element_keys(content: &Map<String, Value>, key_order: &[String]) -> Value {
    let mut ordered = Map::new();
    for key in key_order {
        if let Some(v) = content.get(key) {
            ordered.insert(key.clone(), v.clone());
        }
    }
    Value::Object(ordered)
}

/// Resolve a collision-safe unique-ID for every nested sibling in a group.
///
/// For each entry in `elements`, derive its unique-ID via the configured
/// `unique_id_elements` (sanitized at the boundary of
/// `parse_unique_id_element`), then check whether that id is unique within
/// the group. When two or more siblings would resolve to the same filename,
/// *all* of them fall back to a per-element SHA-256 short hash so no
/// sibling is silently overwritten on disk.
///
/// Returns a `Vec<Option<String>>` parallel to `elements`. `None` means the
/// caller should let `build_disassembled_file` derive the id itself (used
/// for non-nested elements where the override is irrelevant). `Some(id)`
/// means the caller MUST pass that id verbatim as `precomputed_unique_id`
/// so the file write uses the collision-safe filename.
///
/// A single WARN log per colliding group names the parent key, the
/// collided id, and the sibling count so users can investigate their
/// `unique_id_elements` configuration. The default (silent-but-safe)
/// fallback is intentional: pre-0.4.6 the collision caused outright data
/// loss with no log at all, so any visibility is an upgrade.
fn resolve_collision_safe_ids(
    elements: &[Value],
    unique_id_elements: Option<&str>,
    parent_key: &str,
    strategy: &str,
) -> Vec<Option<String>> {
    if strategy != "unique-id" {
        // grouped-by-tag has its own write path with deterministic
        // filename derivation; collision detection is meaningless there.
        return vec![None; elements.len()];
    }

    // First pass: derive each sibling's unique-id (or `None` for elements
    // that won't trigger the unique-id write path - leaves and arrays).
    let derived: Vec<Option<String>> = elements
        .iter()
        .map(|el| {
            if !is_nested_for_unique_id(el) {
                return None;
            }
            Some(parse_unique_id_element(el, unique_id_elements))
        })
        .collect();

    // Tally cardinality of each id across the group, then build the set
    // of colliding ids. We own the strings so the rest of this function
    // can consume `derived` without juggling borrows.
    let mut counts: HashMap<String, usize> = HashMap::new();
    for id in derived.iter().flatten() {
        *counts.entry(id.clone()).or_insert(0) += 1;
    }
    let collided: std::collections::HashSet<String> = counts
        .iter()
        .filter_map(|(k, &v)| if v > 1 { Some(k.clone()) } else { None })
        .collect();

    if collided.is_empty() {
        return derived;
    }

    // One WARN per colliding id describing the impact, using the same
    // `log::warn!` channel already used elsewhere in the crate. The order
    // is non-deterministic across HashMap rehash boundaries but the
    // *content* is stable.
    for id in &collided {
        let count = counts.get(id).copied().unwrap_or(0);
        log::warn!(
            "uniqueIdElements collision: <{parent_key}> id \"{id}\" matched {count} sibling elements; \
             falling back to SHA-256 content hashes for the colliding group. \
             Consider adding more discriminating fields to uniqueIdElements for this metadata type."
        );
    }

    // Second pass: replace any id that landed in a colliding group with a
    // per-element content hash. The hash is computed on the same outer
    // element used for the derivation so distinct content -> distinct hash
    // even when the derived id collided.
    derived
        .into_iter()
        .zip(elements.iter())
        .map(|(maybe_id, element)| match maybe_id {
            Some(id) if collided.contains(&id) => Some(short_hash_for_element(element)),
            other => other,
        })
        .collect()
}

/// Mirrors the nesting predicate inside `parse_element_unified` so we can
/// pre-compute ids only for elements that will actually take the
/// unique-id write path. Leaves and arrays are dispatched differently and
/// don't need a precomputed override.
fn is_nested_for_unique_id(element: &Value) -> bool {
    if element.is_array() {
        return false;
    }
    element
        .as_object()
        .map(|obj| {
            obj.keys()
                .any(|k| !k.starts_with('#') && !k.starts_with('@') && k != "?xml")
        })
        .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
async fn disassemble_element_keys(
    root_element: &Value,
    key_order: &[String],
    disassembled_path: &str,
    root_element_name: &str,
    root_attributes: &Value,
    xml_declaration: Option<&Value>,
    unique_id_elements: Option<&str>,
    strategy: &str,
    format: &str,
) -> (Map<String, Value>, XmlElementArrayMap, usize, bool) {
    let mut leaf_content = Map::new();
    let mut nested_groups = XmlElementArrayMap::new();
    let mut leaf_count = 0usize;
    let mut has_nested_elements = false;

    let empty_map = Map::new();
    let root_obj = root_element.as_object().unwrap_or(&empty_map);

    // Iterate root_obj in key_order's ordering: we consume only keys that are present,
    // which matches the caller's invariant and keeps the loop body branch-free.
    let ordered: Vec<(&String, &Value)> = key_order
        .iter()
        .filter_map(|k| root_obj.get_key_value(k))
        .collect();
    for (key, val) in ordered {
        let elements: Vec<Value> = match val.as_array() {
            Some(arr) => arr.clone(),
            None => vec![val.clone()],
        };

        // Pre-resolve a collision-safe unique-id for every sibling so the
        // nested write path can never silently overwrite a peer. Computed
        // once per (key, sibling-group) and reused across the chunk loop.
        let resolved_ids = resolve_collision_safe_ids(&elements, unique_id_elements, key, strategy);

        for (chunk_offset, chunk) in elements.chunks(BATCH_SIZE).enumerate() {
            for (within_chunk, element) in chunk.iter().enumerate() {
                let global_idx = chunk_offset * BATCH_SIZE + within_chunk;
                let precomputed = resolved_ids.get(global_idx).and_then(Option::as_deref);
                let result = parse_element_unified(XmlElementParams {
                    element: element.clone(),
                    disassembled_path,
                    unique_id_elements,
                    root_element_name,
                    root_attributes: root_attributes.clone(),
                    key,
                    leaf_content: Value::Object(Map::new()),
                    leaf_count,
                    has_nested_elements,
                    format,
                    xml_declaration: xml_declaration.cloned(),
                    strategy,
                    precomputed_unique_id: precomputed,
                })
                .await;

                if let Some(arr) = result.leaf_content.as_object().and_then(|o| o.get(key)) {
                    match leaf_content.get_mut(key).and_then(|v| v.as_array_mut()) {
                        Some(existing_arr) => {
                            if let Some(new_arr) = arr.as_array() {
                                existing_arr.extend(new_arr.iter().cloned());
                            }
                        }
                        None => {
                            leaf_content.insert(key.clone(), arr.clone());
                        }
                    }
                }

                if strategy == "grouped-by-tag" {
                    if let Some(groups) = result.nested_groups {
                        for (tag, arr) in groups {
                            nested_groups.entry(tag).or_default().extend(arr);
                        }
                    }
                }

                leaf_count = result.leaf_count;
                has_nested_elements = result.has_nested_elements;
            }
        }
    }

    (leaf_content, nested_groups, leaf_count, has_nested_elements)
}

/// Extract string from an element's field - handles direct strings and objects with #text (XML leaf elements).
fn get_field_value(element: &Value, field: &str) -> Option<String> {
    let v = element.as_object()?.get(field)?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    v.as_object()
        .and_then(|child| child.get("#text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

/// For group mode: use the segment before the first '.' as key when present (e.g. "Account.Name" -> "Account").
fn group_key_from_field_value(s: &str) -> &str {
    s.find('.').map(|i| &s[..i]).unwrap_or(s)
}

/// Sanitize a string for use as a filename (no path separators or invalid chars).
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

async fn write_nested_groups(
    nested_groups: &XmlElementArrayMap,
    strategy: &str,
    options: &WriteNestedOptions<'_>,
) {
    if strategy != "grouped-by-tag" {
        return;
    }
    let decompose_by_tag: HashMap<&str, &DecomposeRule> = options
        .decompose_rules
        .map(|rules| rules.iter().map(|r| (r.tag.as_str(), r)).collect())
        .unwrap_or_default();

    for (tag, arr) in nested_groups {
        let rule = decompose_by_tag.get(tag.as_str());
        let path_segment = rule
            .map(|r| {
                if r.path_segment.is_empty() {
                    &r.tag
                } else {
                    &r.path_segment
                }
            })
            .unwrap_or(tag);

        if let Some(r) = rule {
            if r.mode == "split" {
                for (idx, item) in arr.iter().enumerate() {
                    let name = get_field_value(item, &r.field)
                        .as_deref()
                        .map(sanitize_filename)
                        .filter(|s: &String| !s.is_empty())
                        .unwrap_or_else(|| idx.to_string());
                    let file_name = format!("{}.{}-meta.{}", name, tag, options.format);
                    let _ =
                        build_disassembled_file(crate::xml::types::BuildDisassembledFileOptions {
                            content: item.clone(),
                            disassembled_path: options.disassembled_path,
                            output_file_name: Some(&file_name),
                            subdirectory: Some(path_segment),
                            wrap_key: Some(tag),
                            is_grouped_array: false,
                            root_element_name: options.root_element_name,
                            root_attributes: options.root_attributes.clone(),
                            format: options.format,
                            xml_declaration: options.xml_declaration.clone(),
                            unique_id_elements: None,
                            precomputed_unique_id: None,
                        })
                        .await;
                }
            } else if r.mode == "group" {
                let mut by_key: HashMap<String, Vec<Value>> = HashMap::new();
                for item in arr {
                    let key = get_field_value(item, &r.field)
                        .as_deref()
                        .map(group_key_from_field_value)
                        .map(sanitize_filename)
                        .filter(|s: &String| !s.is_empty())
                        .unwrap_or_else(|| "unknown".to_string());
                    by_key.entry(key).or_default().push(item.clone());
                }
                // Sort keys for deterministic cross-platform output order
                let mut sorted_keys: Vec<_> = by_key.keys().cloned().collect();
                sorted_keys.sort();
                for key in sorted_keys {
                    let group = by_key.remove(&key).unwrap();
                    let file_name = format!("{}.{}-meta.{}", key, tag, options.format);
                    let _ =
                        build_disassembled_file(crate::xml::types::BuildDisassembledFileOptions {
                            content: Value::Array(group),
                            disassembled_path: options.disassembled_path,
                            output_file_name: Some(&file_name),
                            subdirectory: Some(path_segment),
                            wrap_key: Some(tag),
                            is_grouped_array: true,
                            root_element_name: options.root_element_name,
                            root_attributes: options.root_attributes.clone(),
                            format: options.format,
                            xml_declaration: options.xml_declaration.clone(),
                            unique_id_elements: None,
                            precomputed_unique_id: None,
                        })
                        .await;
                }
            } else {
                fallback_write_one_file(tag, arr, path_segment, options).await;
            }
        } else {
            fallback_write_one_file(tag, arr, path_segment, options).await;
        }
    }
}

async fn fallback_write_one_file(
    tag: &str,
    arr: &[Value],
    _path_segment: &str,
    options: &WriteNestedOptions<'_>,
) {
    let _ = build_disassembled_file(crate::xml::types::BuildDisassembledFileOptions {
        content: Value::Array(arr.to_vec()),
        disassembled_path: options.disassembled_path,
        output_file_name: Some(&format!("{}.{}", tag, options.format)),
        subdirectory: None,
        wrap_key: Some(tag),
        is_grouped_array: true,
        root_element_name: options.root_element_name,
        root_attributes: options.root_attributes.clone(),
        format: options.format,
        xml_declaration: options.xml_declaration.clone(),
        unique_id_elements: None,
        precomputed_unique_id: None,
    })
    .await;
}

struct WriteNestedOptions<'a> {
    disassembled_path: &'a str,
    root_element_name: &'a str,
    root_attributes: Value,
    xml_declaration: Option<Value>,
    format: &'a str,
    decompose_rules: Option<&'a [DecomposeRule]>,
}

pub async fn build_disassembled_files_unified(
    options: BuildDisassembledFilesOptions<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let BuildDisassembledFilesOptions {
        file_path,
        disassembled_path,
        base_name,
        post_purge,
        format,
        unique_id_elements,
        strategy,
        decompose_rules,
    } = options;

    let file_path = normalize_path_unix(file_path);

    let xml_content = match fs::read_to_string(&file_path).await {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let parsed_xml = match crate::xml::parsers::parse_xml_from_str(&xml_content, &file_path) {
        Some(p) => p,
        None => return Ok(()),
    };

    let (root_element_name, root_element) = match get_root_info(&parsed_xml) {
        Some(info) => info,
        None => return Ok(()),
    };
    // The custom parser ignores <?xml ?>; always recover it from raw XML.
    let xml_declaration = extract_xml_declaration_from_raw(&xml_content);

    let root_attributes = extract_root_attributes(&root_element);
    let key_order: Vec<String> = root_element
        .as_object()
        .map(|o| o.keys().filter(|k| !k.starts_with('@')).cloned().collect())
        .unwrap_or_default();

    let (leaf_content, nested_groups, leaf_count, has_nested_elements) = disassemble_element_keys(
        &root_element,
        &key_order,
        disassembled_path,
        &root_element_name,
        &root_attributes,
        xml_declaration.as_ref(),
        unique_id_elements,
        strategy,
        format,
    )
    .await;

    if !has_nested_elements && leaf_count > 0 {
        log::error!(
            "The XML file {} only has leaf elements. This file will not be disassembled.",
            &file_path
        );
        return Ok(());
    }

    let write_opts = WriteNestedOptions {
        disassembled_path,
        root_element_name: &root_element_name,
        root_attributes: root_attributes.clone(),
        xml_declaration: xml_declaration.clone(),
        format,
        decompose_rules,
    };
    write_nested_groups(&nested_groups, strategy, &write_opts).await;

    // Persist root key order so reassembly can match original document order.
    // serde_json::to_string never fails for Vec<String>; writes are best-effort.
    let key_order_path = std::path::Path::new(disassembled_path).join(".key_order.json");
    let json = serde_json::to_string(&key_order).unwrap_or_else(|_| "[]".to_string());
    let _ = fs::write(key_order_path, json).await;

    if leaf_count > 0 {
        let final_leaf_content = if strategy == "grouped-by-tag" {
            order_xml_element_keys(&leaf_content, &key_order)
        } else {
            Value::Object(leaf_content.clone())
        };

        let _ = build_disassembled_file(crate::xml::types::BuildDisassembledFileOptions {
            content: final_leaf_content,
            disassembled_path,
            output_file_name: Some(&format!("{}.{}", base_name, format)),
            subdirectory: None,
            wrap_key: None,
            is_grouped_array: false,
            root_element_name: &root_element_name,
            root_attributes: root_attributes.clone(),
            format,
            xml_declaration: xml_declaration.clone(),
            unique_id_elements: None,
            precomputed_unique_id: None,
        })
        .await;
    }

    if post_purge {
        // Best-effort purge; a failure here is benign (file may have been removed concurrently).
        let _ = fs::remove_file(&file_path).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_field_value_returns_direct_string() {
        let el = json!({ "field": "value" });
        assert_eq!(get_field_value(&el, "field"), Some("value".to_string()));
    }

    #[test]
    fn get_field_value_returns_nested_text() {
        let el = json!({ "field": { "#text": "value" } });
        assert_eq!(get_field_value(&el, "field"), Some("value".to_string()));
    }

    #[test]
    fn get_field_value_returns_none_when_missing_or_non_string() {
        let el = json!({ "field": { "nested": { "#text": "x" } } });
        assert!(get_field_value(&el, "field").is_none());
        assert!(get_field_value(&el, "missing").is_none());
        let el = json!("not-an-object");
        assert!(get_field_value(&el, "field").is_none());
    }

    #[test]
    fn group_key_from_field_value_takes_prefix_before_dot() {
        assert_eq!(group_key_from_field_value("Account.Name"), "Account");
        assert_eq!(group_key_from_field_value("NoDot"), "NoDot");
    }

    #[test]
    fn sanitize_filename_replaces_disallowed_chars_with_underscore() {
        assert_eq!(sanitize_filename("a/b c:d"), "a_b_c_d");
        assert_eq!(sanitize_filename("ok-name_1.xml"), "ok-name_1.xml");
    }

    #[test]
    fn order_xml_element_keys_preserves_order_and_drops_absent() {
        let mut m = Map::new();
        m.insert("b".to_string(), json!(2));
        m.insert("a".to_string(), json!(1));
        let ordered =
            order_xml_element_keys(&m, &["a".to_string(), "c".to_string(), "b".to_string()]);
        let obj = ordered.as_object().unwrap();
        let keys: Vec<&String> = obj.keys().collect();
        assert_eq!(keys, vec![&"a".to_string(), &"b".to_string()]);
    }

    #[test]
    fn get_root_info_returns_name_and_element() {
        let parsed = json!({ "?xml": {"@version": "1.0"}, "Root": { "child": 1 } });
        let (name, element) = get_root_info(&parsed).unwrap();
        assert_eq!(name, "Root");
        assert!(element.as_object().unwrap().contains_key("child"));
    }

    #[test]
    fn get_root_info_returns_none_for_non_object_or_decl_only() {
        assert!(get_root_info(&json!("s")).is_none());
        assert!(get_root_info(&json!({ "?xml": {} })).is_none());
    }

    // ---- collision detection (issue #24) -----------------------------------

    /// Helper for collision-detection tests: build an `<actionOverrides>`
    /// element where the `actionName` field drives the unique-id derivation
    /// and the `content` field drives the per-element hash. Decoupling the
    /// two lets the tests construct distinct-content siblings whose
    /// derived ids deliberately collide - the exact shape that triggered
    /// the silent data loss tracked in #24.
    fn make_action_override(name: &str, content_seed: &str) -> Value {
        json!({
            "actionName": { "#text": name },
            "content": { "#text": format!("Page_{content_seed}") },
            "formFactor": { "#text": "Large" }
        })
    }

    #[test]
    fn resolve_no_collision_returns_derived_ids_unchanged() {
        // Three siblings with distinct actionNames must each keep their
        // derived id - no fallback to hashes when there's no collision.
        let elements = vec![
            make_action_override("View", "v1"),
            make_action_override("Edit", "e1"),
            make_action_override("New", "n1"),
        ];
        let resolved = resolve_collision_safe_ids(
            &elements,
            Some("actionName"),
            "actionOverrides",
            "unique-id",
        );
        assert_eq!(
            resolved,
            vec![
                Some("View".to_string()),
                Some("Edit".to_string()),
                Some("New".to_string())
            ]
        );
    }

    #[test]
    fn resolve_collision_falls_back_to_hash_for_entire_group() {
        // Three siblings sharing actionName=View but differing in content
        // must ALL fall back to a per-element hash. Hashing only the
        // duplicates beyond the first would still let one row "win" the
        // readable id, breaking idempotence: the same input would produce
        // a different first-winner across runs depending on iteration order.
        let elements = vec![
            make_action_override("View", "v1"),
            make_action_override("View", "v2"),
            make_action_override("View", "v3"),
        ];
        let resolved = resolve_collision_safe_ids(
            &elements,
            Some("actionName"),
            "actionOverrides",
            "unique-id",
        );
        let ids: Vec<&str> = resolved
            .iter()
            .map(|o| {
                o.as_deref()
                    .expect("nested element must have a resolved id")
            })
            .collect();
        // Every id is an 8-char hex hash; all three must be distinct
        // because the underlying `content` field differs per row.
        for id in &ids {
            assert_eq!(id.len(), 8, "expected hash fallback, got {id:?}");
            assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        }
        let unique: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique.len(), 3, "siblings must produce distinct hashes");
    }

    #[test]
    fn resolve_partial_collision_only_collapses_the_colliding_subgroup() {
        // Only the two `View` rows collide; `Edit` and `New` keep their
        // derived ids. The hash fallback is targeted - it doesn't punish
        // siblings that already had unique derived ids.
        let elements = vec![
            make_action_override("View", "v1"),
            make_action_override("Edit", "e1"),
            make_action_override("View", "v2"),
            make_action_override("New", "n1"),
        ];
        let resolved = resolve_collision_safe_ids(
            &elements,
            Some("actionName"),
            "actionOverrides",
            "unique-id",
        );
        // Edit and New keep their names verbatim.
        assert_eq!(resolved[1], Some("Edit".to_string()));
        assert_eq!(resolved[3], Some("New".to_string()));
        // The two Views fall back to distinct hashes.
        let v0 = resolved[0].as_deref().unwrap();
        let v2 = resolved[2].as_deref().unwrap();
        assert_eq!(v0.len(), 8);
        assert_eq!(v2.len(), 8);
        assert_ne!(v0, v2, "colliding siblings must hash distinctly");
        assert_ne!(v0, "View");
        assert_ne!(v2, "View");
    }

    #[test]
    fn resolve_skips_grouped_by_tag_strategy() {
        // The grouped-by-tag write path has its own filename derivation;
        // this helper must be a no-op for it so we don't double-pay.
        let elements = vec![
            make_action_override("View", "v1"),
            make_action_override("View", "v2"),
        ];
        let resolved = resolve_collision_safe_ids(
            &elements,
            Some("actionName"),
            "actionOverrides",
            "grouped-by-tag",
        );
        assert_eq!(resolved, vec![None, None]);
    }

    #[test]
    fn resolve_returns_none_for_leaves_and_arrays() {
        // Only nested-object elements take the unique-id write path. Leaves
        // (`{ "#text": "..." }`) and arrays go through different code, so
        // their slot in the parallel resolved-id vec must be `None`.
        let elements = vec![
            json!({ "#text": "leaf" }),
            json!([{ "x": "y" }]),
            make_action_override("Tab", "t1"),
        ];
        let resolved =
            resolve_collision_safe_ids(&elements, Some("actionName"), "tabs", "unique-id");
        assert_eq!(resolved[0], None);
        assert_eq!(resolved[1], None);
        assert_eq!(resolved[2], Some("Tab".to_string()));
    }

    #[test]
    fn resolve_collision_after_sanitization_falls_back_to_hash() {
        // Two distinct un-sanitized values `Foo/Bar` and `Foo_Bar` collapse
        // to the same sanitized form `Foo_Bar`. The collision detector
        // must fire on the sanitized value (which is what reaches disk),
        // not on the raw original. Otherwise we'd silently overwrite.
        let elements = vec![
            json!({ "milestoneName": { "#text": "Foo/Bar" } }),
            json!({ "milestoneName": { "#text": "Foo_Bar" } }),
        ];
        let resolved =
            resolve_collision_safe_ids(&elements, Some("milestoneName"), "milestones", "unique-id");
        // Both must fall back to hashes because their sanitized ids would
        // collide on the same shard path.
        let a = resolved[0].as_deref().unwrap();
        let b = resolved[1].as_deref().unwrap();
        assert_eq!(
            a.len(),
            8,
            "expected hash fallback for sanitization-induced collision"
        );
        assert_eq!(b.len(), 8);
        assert_ne!(a, b, "distinct content must hash distinctly");
    }

    #[tokio::test]
    async fn unified_build_returns_ok_when_source_unreadable() {
        // Missing source file: unified build should short-circuit with Ok(()).
        let dir = tempfile::tempdir().unwrap();
        let disassembled = dir.path().join("out");
        let missing = dir.path().join("does_not_exist.xml");
        build_disassembled_files_unified(BuildDisassembledFilesOptions {
            file_path: missing.to_str().unwrap(),
            disassembled_path: disassembled.to_str().unwrap(),
            base_name: "does_not_exist",
            post_purge: false,
            format: "xml",
            unique_id_elements: None,
            strategy: "unique-id",
            decompose_rules: None,
        })
        .await
        .unwrap();
        assert!(!disassembled.exists());
    }
}
