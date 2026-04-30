//! Command-line interface for the xml-disassembler binary.
//!
//! Kept in the library crate so it can be exercised by unit tests and
//! the binary stays a thin shim.

use crate::xml::{
    DecomposeRule, DisassembleXmlFileHandler, MultiLevelRule, ReassembleXmlFileHandler,
};

/// Options parsed from disassemble CLI args.
pub struct DisassembleOpts<'a> {
    pub path: Option<&'a str>,
    pub unique_id_elements: Option<&'a str>,
    pub pre_purge: bool,
    pub post_purge: bool,
    /// Explicit `--ignore-path` value. `None` means the caller did not
    /// pass the flag and the runner should resolve a default (see
    /// [`crate::xml::ignore_file::resolve_xml_ignore_path`]).
    ///
    /// [`crate::xml::ignore_file::resolve_xml_ignore_path`]: crate::ignore_file::resolve_xml_ignore_path
    pub ignore_path: Option<&'a str>,
    pub format: &'a str,
    pub strategy: Option<&'a str>,
    pub multi_level: Option<String>,
    pub split_tags: Option<String>,
}

/// Parse --split-tags spec for grouped-by-tag. Comma-separated rules; each rule:
/// `tag:mode:field` (path_segment defaults to tag) or `tag:path:mode:field`.
/// mode = "split" (one file per item) or "group" (group by field).
pub fn parse_decompose_spec(spec: &str) -> Vec<DecomposeRule> {
    let mut rules = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        let segments: Vec<&str> = part.splitn(4, ':').collect();
        if segments.len() >= 3 {
            let tag = segments[0].to_string();
            let (path_segment, mode, field) = if segments.len() == 3 {
                (
                    tag.clone(),
                    segments[1].to_string(),
                    segments[2].to_string(),
                )
            } else {
                (
                    segments[1].to_string(),
                    segments[2].to_string(),
                    segments[3].to_string(),
                )
            };
            if !tag.is_empty() && !mode.is_empty() && !field.is_empty() {
                rules.push(DecomposeRule {
                    tag,
                    path_segment,
                    mode,
                    field,
                });
            }
        }
    }
    rules
}

/// Parse a single --multi-level spec: `file_pattern:root_to_strip:unique_id_elements`.
pub fn parse_multi_level_spec(spec: &str) -> Option<MultiLevelRule> {
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    if parts.len() != 3 {
        return None;
    }
    let (file_pattern, root_to_strip, unique_id_elements) = (parts[0], parts[1], parts[2]);
    if file_pattern.is_empty() || root_to_strip.is_empty() || unique_id_elements.is_empty() {
        return None;
    }
    let path_segment = crate::xml::path_segment_from_file_pattern(file_pattern);
    Some(MultiLevelRule {
        file_pattern: file_pattern.to_string(),
        root_to_strip: root_to_strip.to_string(),
        unique_id_elements: unique_id_elements.to_string(),
        path_segment: path_segment.clone(),
        wrap_root_element: root_to_strip.to_string(),
        wrap_xmlns: String::new(),
    })
}

/// Parse one or more --multi-level specs separated by `;`.
///
/// Each rule is `file_pattern:root_to_strip:unique_id_elements`; rules are joined by `;`
/// because the third part is itself a comma-separated list. Empty rules (e.g. trailing `;`)
/// are skipped silently. Malformed rules are dropped (the caller may warn separately if it
/// needs to surface that to the user).
pub fn parse_multi_level_specs(spec: &str) -> Vec<MultiLevelRule> {
    spec.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(parse_multi_level_spec)
        .collect()
}

/// Parse disassemble args: `<path> [options]`.
pub fn parse_disassemble_args(args: &[String]) -> DisassembleOpts<'_> {
    let mut path = None;
    let mut unique_id_elements = None;
    let mut pre_purge = false;
    let mut post_purge = false;
    let mut ignore_path: Option<&str> = None;
    let mut format = "xml";
    let mut strategy = None;
    let mut multi_level = None;
    let mut split_tags = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--postpurge" {
            post_purge = true;
            i += 1;
        } else if arg == "--prepurge" {
            pre_purge = true;
            i += 1;
        } else if let Some(rest) = arg.strip_prefix("--unique-id-elements=") {
            unique_id_elements = Some(rest);
            i += 1;
        } else if arg == "--unique-id-elements" {
            i += 1;
            if i < args.len() {
                unique_id_elements = Some(args[i].as_str());
                i += 1;
            }
        } else if let Some(rest) = arg.strip_prefix("--ignore-path=") {
            ignore_path = Some(rest);
            i += 1;
        } else if arg == "--ignore-path" {
            i += 1;
            if i < args.len() {
                ignore_path = Some(args[i].as_str());
                i += 1;
            }
        } else if let Some(rest) = arg.strip_prefix("--format=") {
            format = rest;
            i += 1;
        } else if arg == "--format" {
            i += 1;
            if i < args.len() {
                format = args[i].as_str();
                i += 1;
            }
        } else if let Some(rest) = arg.strip_prefix("--strategy=") {
            strategy = Some(rest);
            i += 1;
        } else if arg == "--strategy" {
            i += 1;
            if i < args.len() {
                strategy = Some(args[i].as_str());
                i += 1;
            }
        } else if let Some(rest) = arg.strip_prefix("--multi-level=") {
            multi_level = Some(rest.to_string());
            i += 1;
        } else if arg == "--multi-level" {
            i += 1;
            if i < args.len() {
                multi_level = Some(args[i].clone());
                i += 1;
            }
        } else if let Some(rest) = arg.strip_prefix("--split-tags=") {
            split_tags = Some(rest.to_string());
            i += 1;
        } else if arg == "--split-tags" || arg == "-p" {
            i += 1;
            if i < args.len() {
                split_tags = Some(args[i].clone());
                i += 1;
            }
        } else if arg.starts_with("--") {
            i += 1;
        } else if path.is_none() {
            path = Some(arg.as_str());
            i += 1;
        } else {
            i += 1;
        }
    }

    DisassembleOpts {
        path,
        unique_id_elements,
        pre_purge,
        post_purge,
        ignore_path,
        format,
        strategy,
        multi_level,
        split_tags,
    }
}

/// Parse reassemble args: `<path> [extension] [--postpurge]`.
pub fn parse_reassemble_args(args: &[String]) -> (Option<&str>, Option<&str>, bool) {
    let mut path = None;
    let mut extension = None;
    let mut post_purge = false;
    for arg in args {
        if arg == "--postpurge" {
            post_purge = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else if extension.is_none() {
            extension = Some(arg.as_str());
        }
    }
    (path, extension, post_purge)
}

/// Print CLI usage to stderr.
pub fn print_usage() {
    eprintln!("Usage: xml-disassembler <command> [options]");
    eprintln!("  disassemble <path> [options]     - Disassemble XML file or directory");
    eprintln!("    --postpurge                    - Delete original file/dir after disassembling (default: false)");
    eprintln!("    --prepurge                     - Remove existing disassembly output before running (default: false)");
    eprintln!(
        "    --unique-id-elements <list>    - Comma-separated element names for nested filenames"
    );
    eprintln!("    --ignore-path <path>           - Path to ignore file (default: .cdignore; falls back to .xmldisassemblerignore for backward compatibility)");
    eprintln!(
        "    --format <fmt>                 - Output format: xml, json, json5, yaml (default: xml)"
    );
    eprintln!(
        "    --strategy <name>              - unique-id or grouped-by-tag (default: unique-id)"
    );
    eprintln!("    --multi-level <spec>          - Further disassemble matching files: file_pattern:root_to_strip:unique_id_elements (multiple rules separated by ';')");
    eprintln!("    -p, --split-tags <spec>       - With grouped-by-tag: split/group nested tags (e.g. objectPermissions:split:object,fieldPermissions:group:field)");
    eprintln!("  reassemble <path> [extension] [--postpurge]  - Reassemble directory (default extension: xml)");
}

/// Run the CLI with the given args. `args[0]` is expected to be the program name.
pub async fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let command = &args[1];
    match command.as_str() {
        "disassemble" => run_disassemble(&args[2..]).await?,
        "reassemble" => run_reassemble(&args[2..]).await?,
        _ => {
            eprintln!("Unknown command: {}", command);
        }
    }

    Ok(())
}

async fn run_disassemble(args: &[String]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opts = parse_disassemble_args(args);
    let path = opts.path.unwrap_or(".");
    let strategy = opts.strategy.unwrap_or("unique-id");
    let multi_level_rules: Vec<MultiLevelRule> = opts
        .multi_level
        .as_deref()
        .map(parse_multi_level_specs)
        .unwrap_or_default();
    if opts.multi_level.is_some() && multi_level_rules.is_empty() {
        eprintln!("Invalid --multi-level spec; use file_pattern:root_to_strip:unique_id_elements (multiple rules separated by ';')");
    }
    let decompose_rules: Vec<DecomposeRule> = if strategy == "grouped-by-tag" {
        opts.split_tags
            .as_ref()
            .map(|s| parse_decompose_spec(s))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let decompose_rules_ref = if decompose_rules.is_empty() {
        None
    } else {
        Some(decompose_rules.as_slice())
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::Path::new(".").to_path_buf());
    let resolved_ignore = crate::ignore_file::resolve_xml_ignore_path(opts.ignore_path, &cwd);
    let multi_level_rules_ref = if multi_level_rules.is_empty() {
        None
    } else {
        Some(multi_level_rules.as_slice())
    };
    let mut handler = DisassembleXmlFileHandler::new();
    handler
        .disassemble(
            path,
            opts.unique_id_elements,
            Some(strategy),
            opts.pre_purge,
            opts.post_purge,
            &resolved_ignore,
            opts.format,
            multi_level_rules_ref,
            decompose_rules_ref,
        )
        .await?;
    Ok(())
}

async fn run_reassemble(args: &[String]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (path, extension, post_purge) = parse_reassemble_args(args);
    let path = path.unwrap_or(".");
    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(path, extension.or(Some("xml")), post_purge)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sv(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn parse_decompose_spec_three_segments_defaults_path_segment_to_tag() {
        let rules = parse_decompose_spec("objectPermissions:split:object");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert_eq!(r.tag, "objectPermissions");
        assert_eq!(r.path_segment, "objectPermissions");
        assert_eq!(r.mode, "split");
        assert_eq!(r.field, "object");
    }

    #[test]
    fn parse_decompose_spec_four_segments_uses_explicit_path_segment() {
        let rules = parse_decompose_spec("fieldPermissions:fieldPerms:group:field");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert_eq!(r.tag, "fieldPermissions");
        assert_eq!(r.path_segment, "fieldPerms");
        assert_eq!(r.mode, "group");
        assert_eq!(r.field, "field");
    }

    #[test]
    fn parse_decompose_spec_comma_separated_rules_trims_whitespace() {
        let rules = parse_decompose_spec("a:split:f, b:group:g , c:x:split:y");
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].tag, "a");
        assert_eq!(rules[1].tag, "b");
        assert_eq!(rules[2].tag, "c");
        assert_eq!(rules[2].path_segment, "x");
    }

    #[test]
    fn parse_decompose_spec_rejects_empty_segments() {
        // Too few segments
        assert!(parse_decompose_spec("only:two").is_empty());
        // Empty tag, mode, or field are filtered
        assert!(parse_decompose_spec(":split:field").is_empty());
        assert!(parse_decompose_spec("tag::field").is_empty());
        assert!(parse_decompose_spec("tag:split:").is_empty());
    }

    #[test]
    fn parse_multi_level_spec_valid_returns_rule() {
        let rule = parse_multi_level_spec(
            "programProcesses-meta:LoyaltyProgramSetup:parameterName,ruleName",
        )
        .unwrap();
        assert_eq!(rule.file_pattern, "programProcesses-meta");
        assert_eq!(rule.root_to_strip, "LoyaltyProgramSetup");
        assert_eq!(rule.unique_id_elements, "parameterName,ruleName");
        assert_eq!(rule.path_segment, "programProcesses");
        assert_eq!(rule.wrap_root_element, "LoyaltyProgramSetup");
        assert!(rule.wrap_xmlns.is_empty());
    }

    #[test]
    fn parse_multi_level_spec_rejects_wrong_parts() {
        assert!(parse_multi_level_spec("only:two").is_none());
        assert!(parse_multi_level_spec(":Root:ids").is_none());
        assert!(parse_multi_level_spec("file::ids").is_none());
        assert!(parse_multi_level_spec("file:Root:").is_none());
    }

    #[test]
    fn parse_multi_level_specs_single_rule_returns_one() {
        let rules = parse_multi_level_specs("a-meta:Root:id");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].file_pattern, "a-meta");
    }

    #[test]
    fn parse_multi_level_specs_semicolon_separates_rules() {
        let rules =
            parse_multi_level_specs("a-meta:RootA:id1,id2; b-meta:RootB:other ; c-meta:RootC:k");
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].file_pattern, "a-meta");
        assert_eq!(rules[0].unique_id_elements, "id1,id2");
        assert_eq!(rules[1].file_pattern, "b-meta");
        assert_eq!(rules[1].root_to_strip, "RootB");
        assert_eq!(rules[2].file_pattern, "c-meta");
    }

    #[test]
    fn parse_multi_level_specs_skips_empty_and_malformed() {
        // Trailing semicolons and malformed rules are dropped without aborting the rest.
        let rules = parse_multi_level_specs("a:R:id; ; bad ; b:R:id;");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].file_pattern, "a");
        assert_eq!(rules[1].file_pattern, "b");
    }

    #[test]
    fn parse_multi_level_specs_empty_string_returns_empty() {
        assert!(parse_multi_level_specs("").is_empty());
        assert!(parse_multi_level_specs(" ; ;").is_empty());
    }

    #[test]
    fn parse_disassemble_args_handles_flags_and_eq_forms() {
        let args = [
            "path/to/file.xml",
            "--postpurge",
            "--prepurge",
            "--unique-id-elements=name,id",
            "--ignore-path=.foo",
            "--format=json",
            "--strategy=grouped-by-tag",
            "--multi-level=pattern:Root:ids",
            "--split-tags=a:split:b",
        ]
        .iter()
        .map(|s| sv(s))
        .collect::<Vec<_>>();
        let opts = parse_disassemble_args(&args);
        assert_eq!(opts.path, Some("path/to/file.xml"));
        assert!(opts.pre_purge);
        assert!(opts.post_purge);
        assert_eq!(opts.unique_id_elements, Some("name,id"));
        assert_eq!(opts.ignore_path, Some(".foo"));
        assert_eq!(opts.format, "json");
        assert_eq!(opts.strategy, Some("grouped-by-tag"));
        assert_eq!(opts.multi_level.as_deref(), Some("pattern:Root:ids"));
        assert_eq!(opts.split_tags.as_deref(), Some("a:split:b"));
    }

    #[test]
    fn parse_disassemble_args_handles_space_separated_forms() {
        let args = [
            "file.xml",
            "--unique-id-elements",
            "name",
            "--ignore-path",
            ".gitignore",
            "--format",
            "yaml",
            "--strategy",
            "unique-id",
            "--multi-level",
            "p:R:ids",
            "--split-tags",
            "t:split:f",
        ]
        .iter()
        .map(|s| sv(s))
        .collect::<Vec<_>>();
        let opts = parse_disassemble_args(&args);
        assert_eq!(opts.path, Some("file.xml"));
        assert_eq!(opts.unique_id_elements, Some("name"));
        assert_eq!(opts.ignore_path, Some(".gitignore"));
        assert_eq!(opts.format, "yaml");
        assert_eq!(opts.strategy, Some("unique-id"));
        assert_eq!(opts.multi_level.as_deref(), Some("p:R:ids"));
        assert_eq!(opts.split_tags.as_deref(), Some("t:split:f"));
    }

    #[test]
    fn parse_disassemble_args_p_alias_for_split_tags() {
        let args = ["file.xml", "-p", "a:split:b"]
            .iter()
            .map(|s| sv(s))
            .collect::<Vec<_>>();
        let opts = parse_disassemble_args(&args);
        assert_eq!(opts.split_tags.as_deref(), Some("a:split:b"));
    }

    #[test]
    fn parse_disassemble_args_unknown_long_flag_is_skipped() {
        let args = ["file.xml", "--unknown"]
            .iter()
            .map(|s| sv(s))
            .collect::<Vec<_>>();
        let opts = parse_disassemble_args(&args);
        assert_eq!(opts.path, Some("file.xml"));
    }

    #[test]
    fn parse_disassemble_args_defaults_when_empty() {
        let opts = parse_disassemble_args(&[]);
        assert!(opts.path.is_none());
        assert!(opts.strategy.is_none());
        assert!(opts.unique_id_elements.is_none());
        assert!(!opts.pre_purge);
        assert!(!opts.post_purge);
        assert!(
            opts.ignore_path.is_none(),
            "default is `None` so the runner can pick the right filename"
        );
        assert_eq!(opts.format, "xml");
    }

    #[test]
    fn parse_disassemble_args_space_forms_without_value_leave_default() {
        let args = ["--unique-id-elements"]
            .iter()
            .map(|s| sv(s))
            .collect::<Vec<_>>();
        let opts = parse_disassemble_args(&args);
        assert!(opts.unique_id_elements.is_none());
    }

    #[test]
    fn parse_disassemble_args_trailing_extra_positional_ignored() {
        let args = ["first.xml", "second.xml"]
            .iter()
            .map(|s| sv(s))
            .collect::<Vec<_>>();
        let opts = parse_disassemble_args(&args);
        assert_eq!(opts.path, Some("first.xml"));
    }

    #[test]
    fn parse_reassemble_args_picks_path_extension_and_flag() {
        let args = ["some/dir", "json", "--postpurge"]
            .iter()
            .map(|s| sv(s))
            .collect::<Vec<_>>();
        let (path, ext, purge) = parse_reassemble_args(&args);
        assert_eq!(path, Some("some/dir"));
        assert_eq!(ext, Some("json"));
        assert!(purge);
    }

    #[test]
    fn parse_reassemble_args_defaults_and_extra_args_ignored() {
        let (p, e, purge) = parse_reassemble_args(&[]);
        assert!(p.is_none());
        assert!(e.is_none());
        assert!(!purge);

        let args = ["dir", "xml", "extra"]
            .iter()
            .map(|s| sv(s))
            .collect::<Vec<_>>();
        let (p, e, _) = parse_reassemble_args(&args);
        assert_eq!(p, Some("dir"));
        assert_eq!(e, Some("xml"));
    }

    #[tokio::test]
    async fn run_no_args_prints_usage_and_succeeds() {
        run(vec![sv("xml-disassembler")]).await.unwrap();
    }

    #[tokio::test]
    async fn run_unknown_command_is_not_an_error() {
        run(vec![sv("xml-disassembler"), sv("unknown")])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_reassemble_missing_path_returns_err() {
        // Missing directory path propagates an error from fs::metadata.
        let err = run(vec![
            sv("xml-disassembler"),
            sv("reassemble"),
            sv("/definitely/not/here/xyz"),
        ])
        .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn run_disassemble_writes_expected_output() {
        let dir = tempfile::tempdir().unwrap();
        let xml_path = dir.path().join("sample.xml");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Root xmlns="http://example.com">
  <child><name>one</name></child>
  <child><name>two</name></child>
</Root>"#;
        std::fs::write(&xml_path, xml).unwrap();
        run(vec![
            sv("xml-disassembler"),
            sv("disassemble"),
            xml_path.to_string_lossy().to_string(),
        ])
        .await
        .unwrap();
        assert!(dir.path().join("sample").exists());
    }

    #[tokio::test]
    async fn run_disassemble_with_invalid_multi_level_spec_warns_and_continues() {
        let dir = tempfile::tempdir().unwrap();
        let xml_path = dir.path().join("sample.xml");
        let xml =
            r#"<?xml version="1.0" encoding="UTF-8"?><Root><child><name>a</name></child></Root>"#;
        std::fs::write(&xml_path, xml).unwrap();
        run(vec![
            sv("xml-disassembler"),
            sv("disassemble"),
            xml_path.to_string_lossy().to_string(),
            sv("--multi-level=bad-spec"),
        ])
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_reassemble_on_existing_directory_succeeds() {
        // Disassemble then reassemble via the CLI to cover the success path end-to-end.
        let dir = tempfile::tempdir().unwrap();
        let xml_path = dir.path().join("reasm.xml");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Root><child><name>one</name></child><child><name>two</name></child></Root>"#;
        std::fs::write(&xml_path, xml).unwrap();
        run(vec![
            sv("xml-disassembler"),
            sv("disassemble"),
            xml_path.to_string_lossy().to_string(),
        ])
        .await
        .unwrap();
        let disassembled_dir = dir.path().join("reasm");
        assert!(disassembled_dir.exists());
        run(vec![
            sv("xml-disassembler"),
            sv("reassemble"),
            disassembled_dir.to_string_lossy().to_string(),
        ])
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_disassemble_with_grouped_by_tag_split_tags_runs() {
        let dir = tempfile::tempdir().unwrap();
        let xml_path = dir.path().join("perms.xml");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Root>
  <objectPermissions><object>A</object><allowRead>true</allowRead></objectPermissions>
  <objectPermissions><object>B</object><allowRead>false</allowRead></objectPermissions>
</Root>"#;
        std::fs::write(&xml_path, xml).unwrap();
        run(vec![
            sv("xml-disassembler"),
            sv("disassemble"),
            xml_path.to_string_lossy().to_string(),
            sv("--strategy=grouped-by-tag"),
            sv("-p"),
            sv("objectPermissions:split:object"),
        ])
        .await
        .unwrap();
    }
}
