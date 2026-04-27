//! Build disassembled files from source XML file.

use crate::xml::builders::{build_disassembled_file, extract_root_attributes};
use crate::xml::parsers::{extract_xml_declaration_from_raw, parse_element_unified};
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

        for chunk in elements.chunks(BATCH_SIZE) {
            for element in chunk {
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
