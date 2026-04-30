//! Reassemble XML from disassembled directory.

use crate::xml::builders::{build_xml_string, merge_xml_elements, reorder_root_keys};
use crate::xml::multi_level::{ensure_segment_files_structure, load_multi_level_config};
use crate::xml::parsers::parse_to_xml_object;
use crate::xml::types::XmlElement;
use crate::xml::utils::normalize_path_unix;
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use tokio::fs;

/// Read a `.key_order.json` file (if present) and parse it as a list of root key names.
async fn read_key_order(path: &Path) -> Option<Vec<String>> {
    let bytes = fs::read(path).await.ok()?;
    serde_json::from_slice::<Vec<String>>(&bytes).ok()
}

/// Remove @xmlns from an object so the reassembled segment wrapper (e.g. programProcesses) has no xmlns.
fn strip_xmlns_from_value(v: Value) -> Value {
    match v {
        Value::Object(obj) => {
            Value::Object(obj.into_iter().filter(|(k, _)| k != "@xmlns").collect())
        }
        other => other,
    }
}

type ProcessDirFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<XmlElement>, Box<dyn std::error::Error + Send + Sync>>>
            + Send
            + 'a,
    >,
>;

pub struct ReassembleXmlFileHandler;

impl ReassembleXmlFileHandler {
    pub fn new() -> Self {
        Self
    }

    pub async fn reassemble(
        &self,
        file_path: &str,
        file_extension: Option<&str>,
        post_purge: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file_path = normalize_path_unix(file_path);
        if !self.validate_directory(&file_path).await? {
            return Ok(());
        }

        let path = Path::new(&file_path);
        let config = load_multi_level_config(path).await;
        if let Some(ref config) = config {
            for rule in &config.rules {
                let segment_path = path.join(&rule.path_segment);
                self.reassemble_multi_level_segment(&segment_path, rule)
                    .await?;
            }
        }

        // Build one base-segment entry per multi-level rule so the recursive walker can
        // recognize each rule's path_segment under the disassembly root.
        let base_segments: Vec<(String, String, bool)> = config
            .as_ref()
            .map(|c| {
                c.rules
                    .iter()
                    .map(|r| (file_path.clone(), r.path_segment.clone(), true))
                    .collect()
            })
            .unwrap_or_default();
        // When multi-level reassembly is done, purge the entire disassembled directory
        let post_purge_final = post_purge || config.is_some();
        self.reassemble_plain(&file_path, file_extension, post_purge_final, &base_segments)
            .await
    }

    /// Reassemble a single multi-level segment directory: walk each process dir, reassemble
    /// nested segments, reassemble the process, then ensure the wrapper structure.
    async fn reassemble_multi_level_segment(
        &self,
        segment_path: &Path,
        rule: &crate::xml::types::MultiLevelRule,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !segment_path.is_dir() {
            return Ok(());
        }
        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(segment_path).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            entries.push(entry);
        }
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let process_path = entry.path();
            if !process_path.is_dir() {
                continue;
            }
            let process_path_str = normalize_path_unix(&process_path.to_string_lossy());
            let mut sub_entries = Vec::new();
            let mut sub_read = fs::read_dir(&process_path).await?;
            while let Some(e) = sub_read.next_entry().await? {
                sub_entries.push(e);
            }
            sub_entries.sort_by_key(|e| e.file_name());
            for sub_entry in sub_entries {
                let sub_path = sub_entry.path();
                if sub_path.is_dir() {
                    let sub_path_str = normalize_path_unix(&sub_path.to_string_lossy());
                    self.reassemble_plain(&sub_path_str, Some("xml"), true, &[])
                        .await?;
                }
            }
            self.reassemble_plain(&process_path_str, Some("xml"), true, &[])
                .await?;
        }
        ensure_segment_files_structure(
            segment_path,
            &rule.wrap_root_element,
            &rule.path_segment,
            &rule.wrap_xmlns,
        )
        .await?;
        Ok(())
    }

    /// Merge and write reassembled XML (no multi-level pre-step). Used internally.
    /// `base_segments` carries one tuple `(base_path, segment_name, extract_inner)` per
    /// multi-level rule. When the recursive walker reaches `base_path` and finds a subdir
    /// whose name matches one of the segment_names, that subdir's XML files are folded
    /// into a single array under the segment_name key. When extract_inner is true, each
    /// file's structure is `document_root > segment_name > content` and only the content
    /// is collected; otherwise the whole root is kept.
    async fn reassemble_plain(
        &self,
        file_path: &str,
        file_extension: Option<&str>,
        post_purge: bool,
        base_segments: &[(String, String, bool)],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file_path = normalize_path_unix(file_path);
        log::debug!("Parsing directory to reassemble: {}", file_path);
        let parsed_objects = self
            .process_files_in_directory(file_path.to_string(), base_segments.to_vec())
            .await?;

        if parsed_objects.is_empty() {
            log::error!(
                "No files under {} were parsed successfully. A reassembled XML file was not created.",
                file_path
            );
            return Ok(());
        }

        // merge_xml_elements only returns None when every parsed element is empty or
        // declaration-only (no usable root). Treat that the same as "nothing parsed"
        // rather than emitting an `<root></root>` stub.
        let Some(mut merged) = merge_xml_elements(&parsed_objects) else {
            log::error!(
                "No usable root element found while merging files under {}. A reassembled XML file was not created.",
                file_path
            );
            return Ok(());
        };

        // Apply stored key order so reassembled XML matches original document order.
        let key_order_path = Path::new(&file_path).join(".key_order.json");
        if let Some(reordered) = read_key_order(&key_order_path)
            .await
            .and_then(|order| reorder_root_keys(&merged, &order))
        {
            merged = reordered;
        }

        let final_xml = build_xml_string(&merged);
        let output_path = self.get_output_path(&file_path, file_extension);

        fs::write(&output_path, final_xml).await?;

        if post_purge {
            fs::remove_dir_all(file_path).await.ok();
        }

        Ok(())
    }

    fn process_files_in_directory<'a>(
        &'a self,
        dir_path: String,
        base_segments: Vec<(String, String, bool)>,
    ) -> ProcessDirFuture<'a> {
        Box::pin(async move {
            let mut parsed = Vec::new();
            let mut entries = Vec::new();
            let mut read_dir = fs::read_dir(&dir_path).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                entries.push(entry);
            }
            // Sort by full filename for deterministic cross-platform ordering
            entries.sort_by(|a, b| {
                let a_name = a.file_name().to_string_lossy().to_string();
                let b_name = b.file_name().to_string_lossy().to_string();
                a_name.cmp(&b_name)
            });

            // We are at the disassembly root for a given rule when our dir_path matches
            // the base_path stored on that rule. Each rule shares the same base_path in
            // the current implementation, but tracking them per-entry keeps the door open
            // for future per-rule base_paths without another signature change.
            let is_base = base_segments.iter().any(|(base, _, _)| dir_path == *base);

            for entry in entries {
                let path = entry.path();
                let file_path = normalize_path_unix(&path.to_string_lossy()).to_string();

                if path.is_file() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') && self.is_parsable_file(name) {
                        if let Some(parsed_obj) = parse_to_xml_object(&file_path).await {
                            parsed.push(parsed_obj);
                        }
                    }
                } else {
                    // Anything not a regular file is treated as a directory; symlinks and
                    // other exotic entries simply recurse via read_dir below.
                    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let matched_segment = if is_base {
                        base_segments
                            .iter()
                            .find(|(_, seg_name, _)| seg_name == dir_name)
                            .cloned()
                    } else {
                        None
                    };
                    if let Some((_, segment_name, extract_inner)) = matched_segment {
                        let segment_element = self
                            .collect_segment_as_array(&file_path, &segment_name, extract_inner)
                            .await?;
                        if let Some(el) = segment_element {
                            parsed.push(el);
                        }
                    } else {
                        let sub_parsed = self
                            .process_files_in_directory(file_path, base_segments.clone())
                            .await?;
                        parsed.extend(sub_parsed);
                    }
                }
            }

            Ok(parsed)
        })
    }

    /// Collect all .xml files in a directory, parse each, and build one element with
    /// root_key and single key segment_name whose value is array of each file's content.
    /// When extract_inner is true, each file has root > segment_name > content; we push that content.
    async fn collect_segment_as_array(
        &self,
        segment_dir: &str,
        segment_name: &str,
        extract_inner: bool,
    ) -> Result<Option<XmlElement>, Box<dyn std::error::Error + Send + Sync>> {
        let mut xml_files = Vec::new();
        let mut read_dir = fs::read_dir(segment_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_file() && !name.starts_with('.') && self.is_parsable_file(name) {
                xml_files.push(normalize_path_unix(&path.to_string_lossy()));
            }
        }
        xml_files.sort();

        let mut root_contents = Vec::new();
        let mut first_xml: Option<(String, Option<Value>)> = None;
        for file_path in &xml_files {
            // parse_to_xml_object always yields a JSON object on success; treat any other
            // shape (including parse failure) as a skip without branching explicitly.
            let Some(parsed) = parse_to_xml_object(file_path).await else {
                continue;
            };
            let obj_owned = parsed.as_object().cloned().unwrap_or_default();
            let obj = &obj_owned;
            let Some(root_key) = obj.keys().find(|k| *k != "?xml").cloned() else {
                continue;
            };
            let root_val = obj
                .get(&root_key)
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let mut content = if extract_inner {
                root_val
                    .get(segment_name)
                    .cloned()
                    .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
            } else {
                root_val
            };
            // Inner segment element (e.g. programProcesses) should not have xmlns in output
            if extract_inner {
                content = strip_xmlns_from_value(content);
            }
            root_contents.push(content);
            if first_xml.is_none() {
                first_xml = Some((root_key, obj.get("?xml").cloned()));
            }
        }
        if root_contents.is_empty() {
            return Ok(None);
        }
        let (root_key, decl_opt) = first_xml.unwrap();
        let mut content = serde_json::Map::new();
        content.insert(segment_name.to_string(), Value::Array(root_contents));
        let mut top = serde_json::Map::new();
        if let Some(decl) = decl_opt {
            top.insert("?xml".to_string(), decl);
        } else {
            let mut d = serde_json::Map::new();
            d.insert("@version".to_string(), Value::String("1.0".to_string()));
            d.insert("@encoding".to_string(), Value::String("UTF-8".to_string()));
            top.insert("?xml".to_string(), Value::Object(d));
        }
        top.insert(root_key, Value::Object(content));
        Ok(Some(Value::Object(top)))
    }

    fn is_parsable_file(&self, file_name: &str) -> bool {
        let lower = file_name.to_lowercase();
        lower.ends_with(".xml")
            || lower.ends_with(".json")
            || lower.ends_with(".json5")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
    }

    async fn validate_directory(
        &self,
        path: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let meta = fs::metadata(path).await?;
        if !meta.is_dir() {
            log::error!(
                "The provided path to reassemble is not a directory: {}",
                path
            );
            return Ok(false);
        }
        Ok(true)
    }

    fn get_output_path(&self, dir_path: &str, extension: Option<&str>) -> String {
        let path = Path::new(dir_path);
        let parent = path.parent().unwrap_or(Path::new("."));
        let base_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output");
        let ext = extension.unwrap_or("xml");
        parent
            .join(format!("{}.{}", base_name, ext))
            .to_string_lossy()
            .to_string()
    }
}

impl Default for ReassembleXmlFileHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn reassemble_handler_default_equals_new() {
        let _ = ReassembleXmlFileHandler::default();
    }

    #[test]
    fn strip_xmlns_from_value_passes_non_object_through() {
        let s = Value::String("hello".to_string());
        assert_eq!(
            strip_xmlns_from_value(s),
            Value::String("hello".to_string())
        );
        let arr = json!([1, 2]);
        assert_eq!(strip_xmlns_from_value(arr.clone()), arr);
    }

    #[test]
    fn strip_xmlns_from_value_removes_xmlns_key() {
        let obj = json!({ "@xmlns": "ns", "child": 1 });
        let stripped = strip_xmlns_from_value(obj);
        let map = stripped.as_object().unwrap();
        assert!(map.get("@xmlns").is_none());
        assert_eq!(map.get("child").and_then(|v| v.as_i64()), Some(1));
    }

    #[test]
    fn is_parsable_file_recognises_supported_extensions() {
        let h = ReassembleXmlFileHandler::new();
        assert!(h.is_parsable_file("a.xml"));
        assert!(h.is_parsable_file("a.json"));
        assert!(h.is_parsable_file("a.json5"));
        assert!(h.is_parsable_file("a.yaml"));
        assert!(h.is_parsable_file("a.yml"));
        assert!(h.is_parsable_file("A.XML"));
        assert!(!h.is_parsable_file("a.txt"));
    }

    #[test]
    fn get_output_path_appends_extension_and_uses_parent_dir() {
        let h = ReassembleXmlFileHandler::new();
        let out = h.get_output_path("/tmp/foo", Some("xml"));
        assert!(out.ends_with("foo.xml"));
        let out_default = h.get_output_path("/tmp/bar", None);
        assert!(out_default.ends_with("bar.xml"));
        // No parent - uses "." fallback
        assert_eq!(h.get_output_path("only", Some("json")), "only.json");
    }

    #[tokio::test]
    async fn reassemble_multi_level_segment_noop_when_not_dir() {
        let h = ReassembleXmlFileHandler::new();
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not_a_dir.txt");
        tokio::fs::write(&file, "hi").await.unwrap();
        let rule = crate::xml::types::MultiLevelRule {
            file_pattern: String::new(),
            root_to_strip: String::new(),
            unique_id_elements: String::new(),
            path_segment: String::new(),
            wrap_root_element: "Root".to_string(),
            wrap_xmlns: String::new(),
        };
        h.reassemble_multi_level_segment(&file, &rule)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn reassemble_multi_level_segment_skips_files_in_segment_root() {
        let h = ReassembleXmlFileHandler::new();
        let tmp = tempfile::tempdir().unwrap();
        let segment = tmp.path().join("segment");
        tokio::fs::create_dir(&segment).await.unwrap();
        // A bare file inside the segment dir should be skipped (not a subdir).
        tokio::fs::write(segment.join("stray.txt"), "x")
            .await
            .unwrap();
        let rule = crate::xml::types::MultiLevelRule {
            file_pattern: String::new(),
            root_to_strip: String::new(),
            unique_id_elements: String::new(),
            path_segment: "segment".to_string(),
            wrap_root_element: "Root".to_string(),
            wrap_xmlns: "http://example.com".to_string(),
        };
        h.reassemble_multi_level_segment(&segment, &rule)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn collect_segment_as_array_returns_none_for_empty_dir() {
        let h = ReassembleXmlFileHandler::new();
        let tmp = tempfile::tempdir().unwrap();
        let out = h
            .collect_segment_as_array(tmp.path().to_str().unwrap(), "seg", true)
            .await
            .unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn collect_segment_as_array_skips_unparseable_and_empty_roots() {
        let h = ReassembleXmlFileHandler::new();
        let tmp = tempfile::tempdir().unwrap();
        // Unparseable XML
        tokio::fs::write(tmp.path().join("bad.xml"), "<<")
            .await
            .unwrap();
        // Valid XML but only declaration and no root after parse
        tokio::fs::write(tmp.path().join("only-decl.xml"), "")
            .await
            .unwrap();
        // Hidden file is skipped
        tokio::fs::write(tmp.path().join(".hidden.xml"), "<r/>")
            .await
            .unwrap();
        let out = h
            .collect_segment_as_array(tmp.path().to_str().unwrap(), "seg", false)
            .await
            .unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn collect_segment_as_array_without_extract_inner_wraps_root() {
        let h = ReassembleXmlFileHandler::new();
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.xml"), r#"<Root><child>1</child></Root>"#)
            .await
            .unwrap();
        let out = h
            .collect_segment_as_array(tmp.path().to_str().unwrap(), "seg", false)
            .await
            .unwrap()
            .unwrap();
        let obj = out.as_object().unwrap();
        assert!(obj.contains_key("?xml"));
        let root = obj.get("Root").and_then(|r| r.as_object()).unwrap();
        assert!(root.get("seg").and_then(|v| v.as_array()).is_some());
    }
}
