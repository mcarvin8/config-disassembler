//! Parse element during disassembly - unified strategy handling.

use crate::xml::builders::build_disassembled_file;
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

pub async fn parse_element_unified(params: XmlElementParams<'_>) -> UnifiedParseResult {
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
            nested.insert(key.to_string(), vec![element.clone()]);
            return UnifiedParseResult {
                leaf_content: Value::Object(Map::new()),
                leaf_count,
                has_nested_elements: true,
                nested_groups: Some(nested),
            };
        } else {
            let _ = build_disassembled_file(crate::xml::types::BuildDisassembledFileOptions {
                content: element.clone(),
                disassembled_path,
                output_file_name: None,
                subdirectory: Some(key),
                wrap_key: Some(key),
                is_grouped_array: false,
                root_element_name,
                root_attributes: root_attributes.clone(),
                format,
                xml_declaration: xml_declaration.clone(),
                unique_id_elements,
                precomputed_unique_id,
            })
            .await;
            return UnifiedParseResult {
                leaf_content: Value::Object(Map::new()),
                leaf_count,
                has_nested_elements: true,
                nested_groups: None,
            };
        }
    }

    let mut leaf_content = Map::new();
    leaf_content.insert(key.to_string(), Value::Array(vec![element]));
    UnifiedParseResult {
        leaf_content: Value::Object(leaf_content),
        leaf_count: leaf_count + 1,
        has_nested_elements,
        nested_groups: None,
    }
}
