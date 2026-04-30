//! Disassemble XML file handler.

use crate::xml::builders::build_disassembled_files_unified;
use crate::xml::multi_level::{
    capture_xmlns_from_root, path_segment_from_file_pattern, save_multi_level_config,
    strip_root_and_build_xml,
};
use crate::xml::parsers::parse_xml;
use crate::xml::types::{BuildDisassembledFilesOptions, DecomposeRule, MultiLevelRule};
use crate::xml::utils::normalize_path_unix;
use ignore::gitignore::GitignoreBuilder;
use std::path::Path;
use tokio::fs;

pub struct DisassembleXmlFileHandler {
    ign: Option<ignore::gitignore::Gitignore>,
}

impl DisassembleXmlFileHandler {
    pub fn new() -> Self {
        Self { ign: None }
    }

    async fn load_ignore_rules(&mut self, ignore_path: &str) {
        let path = Path::new(ignore_path);
        let content = match fs::read_to_string(path).await {
            Ok(c) => c,
            Err(_) => return,
        };
        let root = path.parent().unwrap_or(Path::new("."));
        let mut builder = GitignoreBuilder::new(root);
        for line in content.lines() {
            let _ = builder.add_line(None, line);
        }
        // `GitignoreBuilder::build` only fails on unlikely I/O errors; treat as absent rules.
        self.ign = builder.build().ok();
    }

    fn posix_path(path: &str) -> String {
        path.replace('\\', "/")
    }

    fn is_xml_file(file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".xml")
    }

    fn is_ignored(&self, path: &str) -> bool {
        self.ign
            .as_ref()
            .map(|ign| ign.matched(path, false).is_ignore())
            .unwrap_or(false)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn disassemble(
        &mut self,
        file_path: &str,
        unique_id_elements: Option<&str>,
        strategy: Option<&str>,
        pre_purge: bool,
        post_purge: bool,
        ignore_path: &str,
        format: &str,
        multi_level_rules: Option<&[MultiLevelRule]>,
        decompose_rules: Option<&[DecomposeRule]>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let strategy = strategy.unwrap_or("unique-id");
        let strategy = if ["unique-id", "grouped-by-tag"].contains(&strategy) {
            strategy
        } else {
            log::warn!(
                "Unsupported strategy \"{}\", defaulting to \"unique-id\".",
                strategy
            );
            "unique-id"
        };

        self.load_ignore_rules(ignore_path).await;

        let path = Path::new(file_path);
        let meta = fs::metadata(path).await?;
        let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        let relative_path = path.strip_prefix(&cwd).unwrap_or(path).to_string_lossy();
        let relative_path = Self::posix_path(&relative_path);

        // Treat an empty rules slice as "no multi-level".
        let multi_level_rules = multi_level_rules.filter(|rules| !rules.is_empty());

        if meta.is_file() {
            self.handle_file(
                file_path,
                &relative_path,
                unique_id_elements,
                strategy,
                pre_purge,
                post_purge,
                format,
                multi_level_rules,
                decompose_rules,
            )
            .await?;
        } else {
            // Anything that isn't a regular file is treated as a directory; fs::metadata on
            // the caller already errored out if the path didn't exist.
            self.handle_directory(
                file_path,
                unique_id_elements,
                strategy,
                pre_purge,
                post_purge,
                format,
                multi_level_rules,
                decompose_rules,
            )
            .await?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_file(
        &self,
        file_path: &str,
        relative_path: &str,
        unique_id_elements: Option<&str>,
        strategy: &str,
        pre_purge: bool,
        post_purge: bool,
        format: &str,
        multi_level_rules: Option<&[MultiLevelRule]>,
        decompose_rules: Option<&[DecomposeRule]>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let resolved = Path::new(file_path)
            .canonicalize()
            .unwrap_or_else(|_| Path::new(file_path).to_path_buf());
        let resolved_str = normalize_path_unix(&resolved.to_string_lossy());

        if !Self::is_xml_file(&resolved_str) {
            log::error!(
                "The file path provided is not an XML file: {}",
                resolved_str
            );
            return Ok(());
        }

        if self.is_ignored(relative_path) {
            log::warn!("File ignored by ignore rules: {}", resolved_str);
            return Ok(());
        }

        let dir_path = resolved.parent().unwrap_or(Path::new("."));
        let dir_path_str = normalize_path_unix(&dir_path.to_string_lossy());
        self.process_file(
            &dir_path_str,
            strategy,
            &resolved_str,
            unique_id_elements,
            pre_purge,
            post_purge,
            format,
            multi_level_rules,
            decompose_rules,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_directory(
        &self,
        dir_path: &str,
        unique_id_elements: Option<&str>,
        strategy: &str,
        pre_purge: bool,
        post_purge: bool,
        format: &str,
        multi_level_rules: Option<&[MultiLevelRule]>,
        decompose_rules: Option<&[DecomposeRule]>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let dir_path = normalize_path_unix(dir_path);
        let mut entries = fs::read_dir(&dir_path).await?;
        let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

        while let Some(entry) = entries.next_entry().await? {
            let sub_path = entry.path();
            let sub_file_path = sub_path.to_string_lossy();
            let relative_sub = sub_path
                .strip_prefix(&cwd)
                .unwrap_or(&sub_path)
                .to_string_lossy();
            let relative_sub = Self::posix_path(&relative_sub);

            if !(sub_path.is_file() && Self::is_xml_file(&sub_file_path)) {
                continue;
            }
            if self.is_ignored(&relative_sub) {
                log::warn!("File ignored by ignore rules: {}", sub_file_path);
                continue;
            }
            let sub_file_path_norm = normalize_path_unix(&sub_file_path);
            self.process_file(
                &dir_path,
                strategy,
                &sub_file_path_norm,
                unique_id_elements,
                pre_purge,
                post_purge,
                format,
                multi_level_rules,
                decompose_rules,
            )
            .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_file(
        &self,
        dir_path: &str,
        strategy: &str,
        file_path: &str,
        unique_id_elements: Option<&str>,
        pre_purge: bool,
        post_purge: bool,
        format: &str,
        multi_level_rules: Option<&[MultiLevelRule]>,
        decompose_rules: Option<&[DecomposeRule]>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::debug!("Parsing file to disassemble: {}", file_path);

        let file_name = Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let base_name = file_name.split('.').next().unwrap_or(file_name);
        let output_path = Path::new(dir_path).join(base_name);

        if pre_purge && output_path.exists() {
            fs::remove_dir_all(&output_path).await.ok();
        }

        build_disassembled_files_unified(BuildDisassembledFilesOptions {
            file_path,
            disassembled_path: output_path.to_str().unwrap_or("."),
            base_name: file_name,
            post_purge,
            format,
            unique_id_elements,
            strategy,
            decompose_rules,
        })
        .await?;

        // Apply each multi-level rule in order. Each rule walks the same disassembly tree
        // independently; rules are merged into the shared `.multi_level.json` so reassembly
        // can replay them in order.
        if let Some(rules) = multi_level_rules {
            for rule in rules {
                self.recursively_disassemble_multi_level(&output_path, rule, format)
                    .await?;
            }
        }

        Ok(())
    }

    /// Recursively walk the disassembly output; for XML files matching the rule's file_pattern,
    /// strip the root and re-disassemble with the rule's unique_id_elements.
    async fn recursively_disassemble_multi_level(
        &self,
        dir_path: &Path,
        rule: &MultiLevelRule,
        format: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut config = crate::xml::multi_level::load_multi_level_config(dir_path)
            .await
            .unwrap_or_default();

        let mut stack = vec![dir_path.to_path_buf()];
        while let Some(current) = stack.pop() {
            let mut entries = Vec::new();
            let mut read_dir = fs::read_dir(&current).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                entries.push(entry);
            }

            for entry in entries {
                let path = entry.path();
                let path_str = path.to_string_lossy().to_string();

                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                // Anything not a directory is processed as a regular file below.
                {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let path_str_check = path.to_string_lossy();
                    if !name.ends_with(".xml")
                        || (!name.contains(&rule.file_pattern)
                            && !path_str_check.contains(&rule.file_pattern))
                    {
                        continue;
                    }

                    let parsed = match parse_xml(&path_str).await {
                        Some(p) => p,
                        None => continue,
                    };
                    let has_element_to_strip = parsed
                        .as_object()
                        .and_then(|o| {
                            let root_key = o.keys().find(|k| *k != "?xml")?;
                            let root_val = o.get(root_key)?.as_object()?;
                            Some(
                                root_key == &rule.root_to_strip
                                    || root_val.contains_key(&rule.root_to_strip),
                            )
                        })
                        .unwrap_or(false);
                    if !has_element_to_strip {
                        continue;
                    }

                    let wrap_xmlns = capture_xmlns_from_root(&parsed).unwrap_or_default();

                    let stripped_xml = match strip_root_and_build_xml(&parsed, &rule.root_to_strip)
                    {
                        Some(xml) => xml,
                        None => continue,
                    };

                    fs::write(&path, stripped_xml).await?;

                    let file_stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("output");
                    let output_dir_name = file_stem.split('.').next().unwrap_or(file_stem);
                    let parent = path.parent().unwrap_or(dir_path);
                    let second_level_output = parent.join(output_dir_name);

                    build_disassembled_files_unified(BuildDisassembledFilesOptions {
                        file_path: &path_str,
                        disassembled_path: second_level_output.to_str().unwrap_or("."),
                        base_name: output_dir_name,
                        post_purge: true,
                        format,
                        unique_id_elements: Some(&rule.unique_id_elements),
                        strategy: "unique-id",
                        decompose_rules: None,
                    })
                    .await?;

                    // Find an existing entry for this rule by (file_pattern, root_to_strip).
                    // Multiple rules may co-exist in `.multi_level.json` (one per logical
                    // segment); per-rule deduplication keeps each one a singleton.
                    let existing_idx = config.rules.iter().position(|r| {
                        r.file_pattern == rule.file_pattern && r.root_to_strip == rule.root_to_strip
                    });
                    match existing_idx {
                        None => {
                            let wrap_root = parsed
                                .as_object()
                                .and_then(|o| o.keys().find(|k| *k != "?xml").cloned())
                                .unwrap_or_else(|| rule.wrap_root_element.clone());
                            let path_segment = if rule.path_segment.is_empty() {
                                path_segment_from_file_pattern(&rule.file_pattern)
                            } else {
                                rule.path_segment.clone()
                            };
                            let stored_xmlns = if rule.wrap_xmlns.is_empty() {
                                wrap_xmlns
                            } else {
                                rule.wrap_xmlns.clone()
                            };
                            config.rules.push(MultiLevelRule {
                                file_pattern: rule.file_pattern.clone(),
                                root_to_strip: rule.root_to_strip.clone(),
                                unique_id_elements: rule.unique_id_elements.clone(),
                                path_segment,
                                // Persist document root (e.g. LoyaltyProgramSetup) so reassembly uses it
                                // as root with xmlns; path_segment is the inner wrapper in each file.
                                wrap_root_element: wrap_root,
                                wrap_xmlns: stored_xmlns,
                            });
                        }
                        Some(idx) => {
                            // Backfill xmlns from the source if we didn't have one yet; otherwise
                            // leave the existing entry alone (the first observed file wins).
                            if config.rules[idx].wrap_xmlns.is_empty() {
                                config.rules[idx].wrap_xmlns = wrap_xmlns;
                            }
                        }
                    }
                }
            }
        }

        if !config.rules.is_empty() {
            save_multi_level_config(dir_path, &config).await?;
        }

        Ok(())
    }
}

impl Default for DisassembleXmlFileHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn disassemble_handler_default_equals_new() {
        let _ = DisassembleXmlFileHandler::default();
    }

    #[test]
    fn is_xml_file_matches_case_insensitively() {
        assert!(DisassembleXmlFileHandler::is_xml_file("foo.xml"));
        assert!(DisassembleXmlFileHandler::is_xml_file("BAR.XML"));
        assert!(!DisassembleXmlFileHandler::is_xml_file("foo.txt"));
    }

    #[test]
    fn posix_path_converts_backslashes() {
        assert_eq!(
            DisassembleXmlFileHandler::posix_path(r"C:\Users\name\file.xml"),
            "C:/Users/name/file.xml"
        );
    }

    #[tokio::test]
    async fn load_ignore_rules_noop_when_path_missing() {
        let mut handler = DisassembleXmlFileHandler::new();
        handler
            .load_ignore_rules("/definitely/does/not/exist/.ignore")
            .await;
        assert!(handler.ign.is_none());
    }

    #[tokio::test]
    async fn load_ignore_rules_builds_matcher() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".ignore");
        tokio::fs::write(&path, "*.xml\n").await.unwrap();
        let mut handler = DisassembleXmlFileHandler::new();
        handler.load_ignore_rules(path.to_str().unwrap()).await;
        assert!(handler.ign.is_some());
        assert!(handler.is_ignored("file.xml"));
        assert!(!handler.is_ignored("file.txt"));
    }

    #[test]
    fn is_ignored_default_false_without_rules() {
        let handler = DisassembleXmlFileHandler::new();
        assert!(!handler.is_ignored("some/path.xml"));
    }
}
