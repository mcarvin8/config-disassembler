//! Build a single disassembled file.

use crate::xml::builders::build_xml_string;
use crate::xml::parsers::parse_unique_id_element;
use crate::xml::transformers::transform_format;
use crate::xml::types::BuildDisassembledFileOptions;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A disassembled file rendered in memory: where it goes and what it contains.
pub(crate) struct RenderedShard {
    pub(crate) directory: PathBuf,
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

/// Build and write a single disassembled file.
pub async fn build_disassembled_file(
    options: BuildDisassembledFileOptions<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    write_shards(vec![render_disassembled_file(options).await]).await
}

/// Write rendered shards with ONE blocking-pool task, creating each distinct
/// directory once.
///
/// `tokio::fs` runs every call as its own blocking task, and writing shards one
/// at a time also repeated `create_dir_all` (a full path walk) for every shard
/// even though they share a directory. Profiling sf-decomposer's workload after
/// #158 still showed kernel scheduling (task switches, futex wakes) and ext4
/// directory lookups as the largest remaining cost of disassemble. Callers
/// render a batch of siblings first and hand it here in one go.
///
/// Every shard is attempted even if an earlier one fails (callers write
/// best-effort, as they did one shard at a time); the first error is returned.
/// `std::fs::write` returns only after the file is written and closed, so a
/// reader that scans the tree right after disassemble never sees a partial shard.
pub(crate) async fn write_shards(
    shards: Vec<RenderedShard>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let directories: BTreeSet<&Path> = shards.iter().map(|s| s.directory.as_path()).collect();
        let mut first_error = None;
        for directory in directories {
            if let Err(e) = std::fs::create_dir_all(directory) {
                first_error.get_or_insert(e);
            }
        }
        for shard in &shards {
            match std::fs::write(&shard.path, &shard.contents) {
                Ok(()) => log::debug!("Created disassembled file: {}", shard.path.display()),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    })
    .await??;
    Ok(())
}

/// Render a disassembled file (target path + serialized contents) without
/// touching the file system. See [`write_shards`].
pub(crate) async fn render_disassembled_file(
    options: BuildDisassembledFileOptions<'_>,
) -> RenderedShard {
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

    let mut inner = match root_attributes {
        Value::Object(attrs) => attrs,
        _ => Map::new(),
    };

    // `content` is owned here, so move it into the shard instead of cloning it.
    if let Some(wk) = wrap_key {
        inner.insert(wk.to_string(), content);
    } else if let Value::Object(obj) = content {
        for (k, v) in obj {
            inner.insert(k, v);
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

    RenderedShard {
        directory: target_directory,
        path: output_path,
        contents: output_string,
    }
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
    async fn build_disassembled_file_derives_name_from_unique_id_when_no_output_name() {
        // wrap_key Some, is_grouped_array=false, content is object →
        // file_name = "{id}.{wrap_key}-meta.{format}" (the `format!` branch)
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_str().unwrap();
        let mut opts = opts_base(path);
        opts.output_file_name = None;
        opts.wrap_key = Some("action");
        opts.is_grouped_array = false;
        opts.content = serde_json::json!({ "name": { "#text": "MyAction" } });
        opts.unique_id_elements = Some("name");
        opts.precomputed_unique_id = None; // triggers parse_unique_id_element
        build_disassembled_file(opts).await.unwrap();
        // File name should contain "action-meta"
        let mut found = false;
        for entry in std::fs::read_dir(temp.path()).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().to_string();
            if name.contains("action-meta") {
                found = true;
                break;
            }
        }
        assert!(found, "expected a file with 'action-meta' in name");
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
        let out = tokio::fs::read_to_string(temp.path().join("single.xml"))
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

    #[tokio::test]
    async fn non_object_root_attributes_render_no_attributes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_str().unwrap();
        let mut opts = opts_base(path);
        opts.root_attributes = Value::Null;

        build_disassembled_file(opts).await.unwrap();

        let out = std::fs::read_to_string(temp.path().join("out.xml")).unwrap();
        assert!(out.contains("<Root>"), "expected a bare <Root>, got: {out}");
        assert!(out.contains("<a>b</a>"), "expected the content, got: {out}");
    }

    fn shard(dir: &std::path::Path, name: &str, contents: &str) -> RenderedShard {
        RenderedShard {
            directory: dir.to_path_buf(),
            path: dir.join(name),
            contents: contents.to_string(),
        }
    }

    #[tokio::test]
    async fn write_shards_creates_every_directory_and_writes_every_shard() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("fieldPermissions");
        let b = temp.path().join("objectPermissions").join("nested");
        let shards = vec![
            shard(&a, "one.xml", "1"),
            shard(&b, "two.xml", "2"),
            shard(&a, "three.xml", "3"),
        ];

        write_shards(shards).await.unwrap();

        assert_eq!(std::fs::read_to_string(a.join("one.xml")).unwrap(), "1");
        assert_eq!(std::fs::read_to_string(b.join("two.xml")).unwrap(), "2");
        assert_eq!(std::fs::read_to_string(a.join("three.xml")).unwrap(), "3");
    }

    #[tokio::test]
    async fn write_shards_keeps_going_after_a_failure_and_reports_it() {
        let temp = tempfile::tempdir().unwrap();
        // A regular file where a shard directory should be: creating that
        // directory, and writing into it, both fail.
        let blocked = temp.path().join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        let good = temp.path().join("good");
        let shards = vec![
            shard(&blocked, "lost.xml", "x"),
            shard(&good, "kept.xml", "kept"),
        ];

        let result = write_shards(shards).await;

        assert!(result.is_err(), "the failed shard must be reported");
        assert_eq!(
            std::fs::read_to_string(good.join("kept.xml")).unwrap(),
            "kept",
            "shards after a failure are still written"
        );
    }
}
