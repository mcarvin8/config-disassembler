//! Build a single disassembled file.

use crate::xml::builders::build_xml_string;
use crate::xml::parsers::parse_unique_id_element;
use crate::xml::transformers::transform_format;
use crate::xml::types::BuildDisassembledFileOptions;
use serde_json::{Map, Value};
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub async fn build_disassembled_file(
    options: BuildDisassembledFileOptions<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let BuildDisassembledFileOptions {
        content,
        disassembled_path,
        output_file_name,
        subdirectory,
        wrap_key,
        is_grouped_array,
        root_element_name,
        root_attributes,
        xml_declaration,
        format,
        unique_id_elements,
        precomputed_unique_id,
    } = options;

    let target_directory = if let Some(subdir) = subdirectory {
        Path::new(disassembled_path).join(subdir)
    } else {
        Path::new(disassembled_path).to_path_buf()
    };

    let file_name = if let Some(name) = output_file_name {
        name.to_string()
    } else if let Some(wk) = wrap_key {
        if !is_grouped_array && content.is_object() {
            // Caller-supplied id wins. The collision detector in
            // `disassemble_element_keys` injects a content hash here when
            // two siblings of the same parent would otherwise resolve to
            // the same filename - without that override we'd silently
            // overwrite the first-written sibling on the second write.
            let id = precomputed_unique_id
                .map(str::to_string)
                .unwrap_or_else(|| parse_unique_id_element(&content, unique_id_elements));
            format!("{}.{}-meta.{}", id, wk, format)
        } else {
            "output".to_string()
        }
    } else {
        "output".to_string()
    };

    let output_path = target_directory.join(&file_name);

    fs::create_dir_all(&target_directory).await?;

    let root_attrs_obj = root_attributes.as_object().cloned().unwrap_or_default();
    let mut inner = root_attrs_obj.clone();

    if let Some(wk) = wrap_key {
        inner.insert(wk.to_string(), content.clone());
    } else if let Some(obj) = content.as_object() {
        for (k, v) in obj {
            inner.insert(k.clone(), v.clone());
        }
    }

    let mut wrapped_inner = Map::new();
    wrapped_inner.insert(root_element_name.to_string(), Value::Object(inner));

    if let Some(decl) = xml_declaration.filter(|d| d.is_object()) {
        let mut root = Map::new();
        root.insert("?xml".to_string(), decl);
        for (k, v) in wrapped_inner {
            root.insert(k, v);
        }
        wrapped_inner = root;
    }

    let wrapped_xml = Value::Object(wrapped_inner);

    let output_string = if let Some(s) = transform_format(format, &wrapped_xml).await {
        s
    } else {
        build_xml_string(&wrapped_xml)
    };

    // Tokio's `write_all` only guarantees the bytes are queued in the
    // runtime's userspace buffer; it does NOT guarantee they have reached
    // the OS or that the file is visible to subsequent readers. Without
    // an explicit `flush` + `shutdown`, callers that immediately read the
    // disassembled tree (test harnesses, multi-step pipelines) can race
    // and observe a partially-written directory - the failure mode is a
    // "missing" shard whose write was queued but not yet flushed when
    // the directory was scanned. `shutdown` fully closes the handle and
    // waits for the underlying file to be flushed, eliminating the race.
    let mut file = fs::File::create(&output_path).await?;
    file.write_all(output_string.as_bytes()).await?;
    file.flush().await?;
    file.shutdown().await?;
    log::debug!("Created disassembled file: {}", output_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn opts_base(disassembled_path: &str) -> BuildDisassembledFileOptions<'_> {
        BuildDisassembledFileOptions {
            content: json!({ "a": "b" }),
            disassembled_path,
            output_file_name: Some("out.xml"),
            subdirectory: None,
            wrap_key: None,
            is_grouped_array: false,
            root_element_name: "Root",
            root_attributes: Value::Object(Map::new()),
            xml_declaration: None,
            format: "xml",
            unique_id_elements: None,
            precomputed_unique_id: None,
        }
    }

    #[tokio::test]
    async fn build_disassembled_file_file_name_output_when_wrap_key_no_output_name_grouped_array() {
        // wrap_key Some, is_grouped_array true → file_name = "output"
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_str().unwrap();
        let mut opts = opts_base(path);
        opts.output_file_name = None;
        opts.wrap_key = Some("wrap");
        opts.is_grouped_array = true;
        opts.content = json!([{ "x": "1" }]);
        build_disassembled_file(opts).await.unwrap();
        assert!(temp.path().join("output").exists());
    }

    #[tokio::test]
    async fn build_disassembled_file_file_name_output_when_wrap_key_content_not_object() {
        // wrap_key Some, content not object (e.g. Array) → file_name = "output"
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_str().unwrap();
        let mut opts = opts_base(path);
        opts.output_file_name = None;
        opts.wrap_key = Some("wrap");
        opts.is_grouped_array = false;
        opts.content = json!([{ "id": "a" }]);
        build_disassembled_file(opts).await.unwrap();
        assert!(temp.path().join("output").exists());
    }

    #[tokio::test]
    async fn build_disassembled_file_file_name_output_when_no_wrap_key_no_output_name() {
        // No output_file_name, no wrap_key → file_name = "output"
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_str().unwrap();
        let mut opts = opts_base(path);
        opts.output_file_name = None;
        opts.wrap_key = None;
        build_disassembled_file(opts).await.unwrap();
        assert!(temp.path().join("output").exists());
    }

    #[tokio::test]
    async fn build_disassembled_file_content_not_object_no_spread() {
        // No wrap_key, content not object -> inner is only root metadata.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_str().unwrap();
        let mut opts = opts_base(path);
        opts.output_file_name = Some("single.xml");
        opts.wrap_key = None;
        opts.content = json!(42);
        opts.root_attributes = json!({ "marker": "kept" });
        build_disassembled_file(opts).await.unwrap();
        let out = fs::read_to_string(temp.path().join("single.xml"))
            .await
            .unwrap();
        assert!(out.contains("<Root"), "expected Root element, got: {out}");
        assert!(out.contains("marker"), "expected root metadata, got: {out}");
        assert!(
            out.contains("kept"),
            "expected root metadata value, got: {out}"
        );
        assert!(!out.contains("42"));
    }
}
