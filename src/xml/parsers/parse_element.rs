//! Parse element during disassembly - unified strategy handling.

use crate::xml::builders::{render_disassembled_file, write_shards, RenderedShard};
use crate::xml::types::{UnifiedParseResult, XmlElementArrayMap, XmlElementParams};
use serde_json::{Map, Value};

fn is_nested_object(element: &Value) -> bool {
    element
        .as_object()
        .map(|obj| {
            obj.keys()
                .any(|k| !k.starts_with('#') && !k.starts_with('@') && k != "?xml")
        })
        .unwrap_or(false)
}

/// Parse one element during disassembly, writing its shard immediately when it
/// takes the unique-id nested path. Internally, disassembly uses
/// `parse_element_collect` (crate-internal) to batch those writes instead.
pub async fn parse_element_unified(params: XmlElementParams<'_>) -> UnifiedParseResult {
    let (result, shard) = parse_element_collect(params).await;
    if let Some(shard) = shard {
        let _ = write_shards(vec![shard]).await;
    }
    result
}

/// Same dispatch as [`parse_element_unified`], but a unique-id nested element's
/// shard is rendered and RETURNED rather than written, so the caller can write a
/// whole batch of siblings with one blocking-pool task.
pub(crate) async fn parse_element_collect(
    params: XmlElementParams<'_>,
) -> (UnifiedParseResult, Option<RenderedShard>) {
    let XmlElementParams {
        element,
        disassembled_path,
        unique_id_elements,
        root_element_name,
        root_attributes,
        key,
        leaf_count,
        has_nested_elements,
        format,
        xml_declaration,
        strategy,
        leaf_content: _,
        precomputed_unique_id,
    } = params;

    let is_array = element.is_array();
    let is_nested_obj = is_nested_object(&element);
    let is_nested = is_array || is_nested_obj;

    if is_nested {
        if strategy == "grouped-by-tag" {
            let mut nested = XmlElementArrayMap::new();
            nested.insert(key.to_string(), vec![element]);
            return (
                UnifiedParseResult {
                    leaf_content: Value::Object(Map::new()),
                    leaf_count,
                    has_nested_elements: true,
                    nested_groups: Some(nested),
                },
                None,
            );
        } else {
            let shard = render_disassembled_file(crate::xml::types::BuildDisassembledFileOptions {
                content: element,
                disassembled_path,
                output_file_name: None,
                subdirectory: Some(key),
                wrap_key: Some(key),
                is_grouped_array: false,
                root_element_name,
                root_attributes,
                format,
                xml_declaration,
                unique_id_elements,
                precomputed_unique_id,
            })
            .await;
            return (
                UnifiedParseResult {
                    leaf_content: Value::Object(Map::new()),
                    leaf_count,
                    has_nested_elements: true,
                    nested_groups: None,
                },
                Some(shard),
            );
        }
    }

    let mut leaf_content = Map::new();
    leaf_content.insert(key.to_string(), Value::Array(vec![element]));
    (
        UnifiedParseResult {
            leaf_content: Value::Object(leaf_content),
            leaf_count: leaf_count + 1,
            has_nested_elements,
            nested_groups: None,
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params<'a>(element: Value, dir: &'a str) -> XmlElementParams<'a> {
        XmlElementParams {
            element,
            disassembled_path: dir,
            unique_id_elements: Some("name"),
            root_element_name: "Root",
            root_attributes: json!({}),
            key: "items",
            leaf_content: Value::Object(Map::new()),
            leaf_count: 0,
            has_nested_elements: false,
            format: "xml",
            xml_declaration: None,
            strategy: "unique-id",
            precomputed_unique_id: None,
        }
    }

    // The public entry point keeps its write-as-you-go behaviour even though
    // disassembly itself now batches through `parse_element_collect`.
    #[tokio::test]
    async fn parse_element_unified_writes_the_shard_of_a_nested_element() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().to_str().unwrap();

        let result =
            parse_element_unified(params(json!({ "name": "Alpha", "value": "1" }), dir)).await;

        assert!(result.has_nested_elements);
        let shard = temp.path().join("items").join("Alpha.items-meta.xml");
        let written = std::fs::read_to_string(&shard).expect("shard written");
        assert!(written.contains("<name>Alpha</name>"), "{written}");
    }

    #[tokio::test]
    async fn parse_element_unified_writes_nothing_for_a_leaf_element() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().to_str().unwrap();

        let result = parse_element_unified(params(json!("just text"), dir)).await;

        assert_eq!(result.leaf_count, 1);
        assert!(!result.has_nested_elements);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
