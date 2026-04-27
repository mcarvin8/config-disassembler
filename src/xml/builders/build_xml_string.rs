//! Build XML string from XmlElement structure.

use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use serde_json::{Map, Value};

use crate::xml::types::XmlElement;

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => serde_json::to_string(v).unwrap_or_default(),
    }
}

fn write_element<W: std::io::Write>(
    writer: &mut Writer<W>,
    name: &str,
    content: &Value,
    indent_level: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let indent = "    ".repeat(indent_level);
    let child_indent = "    ".repeat(indent_level + 1);

    match content {
        Value::Object(obj) => {
            let (attrs, children): (Vec<_>, Vec<_>) =
                obj.iter().partition(|(k, _)| k.starts_with('@'));

            let attr_name = |k: &str| k.trim_start_matches('@').to_string();

            let mut text_content = String::new();
            let mut comment_content = String::new();
            let mut text_tail_content = String::new();
            let mut cdata_content = String::new();
            let child_elements: Vec<(&String, &Value)> = children
                .iter()
                .filter_map(|(k, v)| {
                    if *k == "#text" {
                        text_content = value_to_string(v);
                        None
                    } else if *k == "#comment" {
                        comment_content = value_to_string(v);
                        None
                    } else if *k == "#text-tail" {
                        text_tail_content = value_to_string(v);
                        None
                    } else if *k == "#cdata" {
                        cdata_content = value_to_string(v);
                        None
                    } else {
                        Some((*k, *v))
                    }
                })
                .collect();

            let has_children = child_elements.iter().any(|(_, v)| {
                v.is_object()
                    || (v.is_array() && v.as_array().map(|a| !a.is_empty()).unwrap_or(false))
            });

            let attrs: Vec<(String, String)> = attrs
                .iter()
                .map(|(k, v)| (attr_name(k), value_to_string(v)))
                .collect();

            let mut start = BytesStart::new(name);
            for (k, v) in &attrs {
                start.push_attribute((k.as_str(), v.as_str()));
            }
            writer.write_event(Event::Start(start))?;

            if has_children || !child_elements.is_empty() {
                writer.write_event(Event::Text(BytesText::new(
                    format!("\n{}", child_indent).as_str(),
                )))?;

                let child_count = child_elements.len();
                for (idx, (child_name, child_value)) in child_elements.iter().enumerate() {
                    let is_last = idx == child_count - 1;
                    match child_value {
                        Value::Array(arr) => {
                            let arr_len = arr.len();
                            for (i, item) in arr.iter().enumerate() {
                                let arr_last = i == arr_len - 1;
                                write_element(writer, child_name, item, indent_level + 1)?;
                                if !arr_last {
                                    writer.write_event(Event::Text(BytesText::new(
                                        format!("\n{}", child_indent).as_str(),
                                    )))?;
                                }
                            }
                            if !is_last {
                                writer.write_event(Event::Text(BytesText::new(
                                    format!("\n{}", child_indent).as_str(),
                                )))?;
                            }
                        }
                        Value::Object(_) => {
                            write_element(writer, child_name, child_value, indent_level + 1)?;
                            if !is_last {
                                writer.write_event(Event::Text(BytesText::new(
                                    format!("\n{}", child_indent).as_str(),
                                )))?;
                            }
                        }
                        _ => {
                            writer
                                .write_event(Event::Start(BytesStart::new(child_name.as_str())))?;
                            // BytesText::new() expects unescaped content; the writer escapes when writing
                            writer.write_event(Event::Text(BytesText::new(
                                value_to_string(child_value).as_str(),
                            )))?;
                            writer.write_event(Event::End(BytesEnd::new(child_name.as_str())))?;
                            if !is_last {
                                writer.write_event(Event::Text(BytesText::new(
                                    format!("\n{}", child_indent).as_str(),
                                )))?;
                            }
                        }
                    }
                }

                writer.write_event(Event::Text(BytesText::new(
                    format!("\n{}", indent).as_str(),
                )))?;
            } else if !cdata_content.is_empty()
                || !text_content.is_empty()
                || !comment_content.is_empty()
                || !text_tail_content.is_empty()
            {
                // Add newline+indent before content when no leading text (keeps CDATA/comment on separate line)
                if text_content.is_empty() && comment_content.is_empty() {
                    writer.write_event(Event::Text(BytesText::new(
                        format!("\n{}", child_indent).as_str(),
                    )))?;
                }
                // Output in order: #text, #comment, #text-tail, #cdata
                if !text_content.is_empty() {
                    writer.write_event(Event::Text(BytesText::new(text_content.as_str())))?;
                }
                if !comment_content.is_empty() {
                    writer.write_event(Event::Comment(BytesText::new(comment_content.as_str())))?;
                }
                if !text_tail_content.is_empty() {
                    writer.write_event(Event::Text(BytesText::new(text_tail_content.as_str())))?;
                }
                if !cdata_content.is_empty() {
                    writer.write_event(Event::CData(BytesCData::new(cdata_content.as_str())))?;
                }
                // Add newline+indent before closing tag only for CDATA (keeps compact for text-only)
                if !cdata_content.is_empty() {
                    writer.write_event(Event::Text(BytesText::new(
                        format!("\n{}", indent).as_str(),
                    )))?;
                }
            }

            writer.write_event(Event::End(BytesEnd::new(name)))?;
        }
        Value::Array(arr) => {
            for item in arr {
                write_element(writer, name, item, indent_level)?;
            }
        }
        _ => {
            writer.write_event(Event::Start(BytesStart::new(name)))?;
            // BytesText::new() expects unescaped content; the writer escapes when writing
            writer.write_event(Event::Text(BytesText::new(
                value_to_string(content).as_str(),
            )))?;
            writer.write_event(Event::End(BytesEnd::new(name)))?;
        }
    }

    Ok(())
}

fn build_xml_from_object(
    element: &Map<String, Value>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Use Writer::new (no indent) so leaf elements stay compact and match fixture format
    let mut writer = Writer::new(Vec::new());

    let (declaration, root_key, root_value) = if let Some(decl) = element.get("?xml") {
        let root_key = element
            .keys()
            .find(|k| *k != "?xml")
            .cloned()
            .unwrap_or_else(|| "root".to_string());
        let root_value = element
            .get(&root_key)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        (Some(decl), root_key, root_value)
    } else {
        let root_key = element
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "root".to_string());
        let root_value = element
            .get(&root_key)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        (None, root_key, root_value)
    };

    if let Some(obj) = declaration.and_then(|d| d.as_object()) {
        let version = obj
            .get("@version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0");
        let encoding = obj.get("@encoding").and_then(|v| v.as_str());
        let standalone = obj.get("@standalone").and_then(|v| v.as_str());
        writer.write_event(Event::Decl(BytesDecl::new(version, encoding, standalone)))?;
        writer.write_event(Event::Text(BytesText::new("\n")))?;
    }

    write_element(&mut writer, &root_key, &root_value, 0)?;

    let result = String::from_utf8(writer.into_inner())?;
    Ok(result.trim_end().to_string())
}

/// Build XML string from XmlElement.
pub fn build_xml_string(element: &XmlElement) -> String {
    match element {
        Value::Object(obj) => build_xml_from_object(obj).unwrap_or_default(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_xml_string_non_object_returns_empty() {
        assert!(build_xml_string(&Value::Array(vec![])).is_empty());
        assert!(build_xml_string(&Value::Null).is_empty());
    }

    #[test]
    fn build_xml_string_simple_root() {
        let el = json!({
            "?xml": { "@version": "1.0", "@encoding": "UTF-8" },
            "root": { "child": "value" }
        });
        let out = build_xml_string(&el);
        assert!(out.contains("<?xml"));
        assert!(out.contains("<root>"));
        assert!(out.contains("<child>value</child>"));
        assert!(out.contains("</root>"));
    }

    #[test]
    fn build_xml_string_with_attributes() {
        let el = json!({
            "root": { "@xmlns": "http://example.com", "a": "b" }
        });
        let out = build_xml_string(&el);
        assert!(out.contains("xmlns"));
        assert!(out.contains("http://example.com"));
        assert!(out.contains("<a>b</a>"));
    }

    #[test]
    fn build_xml_string_with_array() {
        let el = json!({
            "root": { "item": [ { "x": "1" }, { "x": "2" } ] }
        });
        let out = build_xml_string(&el);
        assert!(out.contains("<item>"));
        assert!(out.contains("<x>1</x>"));
        assert!(out.contains("<x>2</x>"));
    }

    #[test]
    fn build_xml_string_without_declaration() {
        let el = json!({ "root": { "a": "b" } });
        let out = build_xml_string(&el);
        assert!(!out.contains("<?xml"));
        assert!(out.contains("<root>"));
    }

    #[test]
    fn build_xml_string_with_text_comment_cdata() {
        let root = json!({
            "#text": "text",
            "#comment": " a comment ",
            "#cdata": "<cdata>"
        });
        let el = json!({
            "?xml": { "@version": "1.0" },
            "root": root
        });
        let out = build_xml_string(&el);
        assert!(out.contains("text"));
        assert!(out.contains("<!--"));
        assert!(out.contains(" a comment "));
        assert!(out.contains("<![CDATA["));
        assert!(out.contains("<cdata>"));
    }

    #[test]
    fn build_xml_string_with_declaration_encoding_standalone() {
        let el = json!({
            "?xml": { "@version": "1.0", "@encoding": "UTF-8", "@standalone": "yes" },
            "root": { "a": "b" }
        });
        let out = build_xml_string(&el);
        assert!(out.contains("<?xml"));
        assert!(out.contains("UTF-8"));
        assert!(out.contains("standalone"));
        assert!(out.contains("<root>"));
    }

    #[test]
    fn build_xml_string_primitive_sibling_children() {
        // Root with multiple children: one object, one primitive (hits _ => branch)
        let el = json!({
            "root": { "obj": { "x": "1" }, "num": 42, "flag": true }
        });
        let out = build_xml_string(&el);
        assert!(out.contains("<obj>"));
        assert!(out.contains("<num>42</num>"));
        assert!(out.contains("<flag>true</flag>"));
    }

    #[test]
    fn build_xml_string_null_child_value() {
        let el = json!({
            "root": { "empty": null }
        });
        let out = build_xml_string(&el);
        assert!(out.contains("<empty>"));
        assert!(out.contains("</empty>"));
    }

    #[test]
    fn build_xml_string_cdata_only_no_text_or_comment() {
        let root = json!({ "#cdata": "only cdata content" });
        let el = json!({ "?xml": { "@version": "1.0" }, "root": root });
        let out = build_xml_string(&el);
        assert!(out.contains("<![CDATA["));
        assert!(out.contains("only cdata content"));
    }

    #[test]
    fn build_xml_string_declaration_only_defaults_root_key() {
        let el = json!({ "?xml": { "@version": "1.0", "@encoding": "UTF-8" } });
        let out = build_xml_string(&el);
        assert!(out.contains("<?xml"));
        assert!(out.contains("<root>"));
    }

    #[test]
    fn build_xml_string_declaration_non_object_skips_decl_write() {
        let el = json!({ "?xml": "not-an-object", "root": { "a": "b" } });
        let out = build_xml_string(&el);
        assert!(!out.contains("<?xml"));
        assert!(out.contains("<root>"));
    }

    #[test]
    fn build_xml_string_root_value_array_sibling_elements() {
        // Root value is Array (write_element Value::Array branch)
        let el = json!({
            "root": [ { "a": "1" }, { "b": "2" } ]
        });
        let out = build_xml_string(&el);
        assert!(out.contains("<root>"));
        assert!(out.contains("<a>1</a>"));
        assert!(out.contains("<b>2</b>"));
        assert!(out.contains("</root>"));
    }

    #[test]
    fn build_xml_string_root_value_primitive() {
        // Root value is primitive (write_element _ branch for top-level content)
        let el = json!({ "root": 42 });
        let out = build_xml_string(&el);
        assert!(out.contains("<root>42</root>"));
    }

    #[test]
    fn build_xml_string_attribute_value_object_uses_serde_fallback() {
        // Attribute value that is Object hits value_to_string _ branch (serde_json::to_string)
        let el = json!({
            "root": { "@complex": { "nested": true }, "child": "v" }
        });
        let out = build_xml_string(&el);
        assert!(out.contains("child"));
        assert!(out.contains("v"));
    }
}
