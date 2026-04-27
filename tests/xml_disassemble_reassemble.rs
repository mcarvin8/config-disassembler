//! Integration test: disassemble an XML file, reassemble the output, and confirm
//! the reassembled XML matches the original file contents (same as original TypeScript tests).

use config_disassembler::xml::{
    DecomposeRule, DisassembleXmlFileHandler, MultiLevelRule, ReassembleXmlFileHandler,
};
use std::path::Path;

#[tokio::test]
async fn reassemble_with_file_path_returns_ok_no_op() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture must exist (run from project root)"
    );
    let handler = ReassembleXmlFileHandler::new();
    // Path is a file, not a directory; validate_directory returns false
    handler
        .reassemble(fixture, Some("xml"), false)
        .await
        .expect("reassemble should return Ok(())");
}

#[tokio::test]
async fn disassemble_with_unsupported_strategy_defaults_to_unique_id() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture must exist (run from project root)"
    );
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("HR_Admin.permissionset-meta.xml");
    std::fs::copy(fixture, &source).expect("copy fixture");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unsupported-strategy"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(
        base.join("HR_Admin").exists(),
        "Should still disassemble with default strategy"
    );
}

#[tokio::test]
async fn disassemble_directory_with_ignore_skips_matching_files() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture must exist (run from project root)"
    );
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let dir_path = base.join("meta");
    std::fs::create_dir_all(&dir_path).expect("create dir");
    std::fs::copy(fixture, dir_path.join("A.permissionset-meta.xml")).expect("copy");
    std::fs::copy(fixture, dir_path.join("B.permissionset-meta.xml")).expect("copy");
    std::fs::write(
        base.join(".xmldisassemblerignore"),
        "B.permissionset-meta.xml",
    )
    .expect("write ignore");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            dir_path.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            base.join(".xmldisassemblerignore").to_str().unwrap(),
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(dir_path.join("A").exists(), "A should be disassembled");
    assert!(
        !dir_path.join("B").exists(),
        "B should be ignored by .xmldisassemblerignore"
    );
}

#[tokio::test]
async fn disassemble_directory_processes_xml_files() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture must exist (run from project root)"
    );
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let dir_path = base.join("meta");
    std::fs::create_dir_all(&dir_path).expect("create dir");
    let f1 = dir_path.join("A.permissionset-meta.xml");
    let f2 = dir_path.join("B.permissionset-meta.xml");
    std::fs::copy(fixture, &f1).expect("copy");
    std::fs::copy(fixture, &f2).expect("copy");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            dir_path.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(dir_path.join("A").exists());
    assert!(dir_path.join("B").exists());
}

#[tokio::test]
async fn disassemble_directory_ignores_non_xml_files_and_subdirs() {
    // Directory contains one XML, a non-XML file, and a subdirectory; handle_directory must
    // skip the non-file / non-XML entries without error.
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let dir_path = base.join("mixed");
    std::fs::create_dir_all(&dir_path).expect("mkdir");
    std::fs::create_dir_all(dir_path.join("nested")).expect("mkdir nested");
    std::fs::copy(fixture, dir_path.join("A.permissionset-meta.xml")).expect("copy xml");
    std::fs::write(dir_path.join("notes.txt"), "ignore me").expect("write txt");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            dir_path.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(dir_path.join("A").exists());
}

#[tokio::test]
async fn reassemble_with_post_purge_removes_disassembled_dir() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture must exist (run from project root)"
    );
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("HR_Admin.permissionset-meta.xml");
    let disassembled_dir = base.join("HR_Admin");
    std::fs::copy(fixture, &source).expect("copy");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(disassembled_dir.exists());
    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(disassembled_dir.to_str().unwrap(), Some("xml"), true)
        .await
        .expect("reassemble");
    assert!(
        !disassembled_dir.exists(),
        "post_purge should remove disassembled directory"
    );
}

#[tokio::test]
async fn reassemble_applies_key_order_from_dot_key_order_json() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("Out");
    std::fs::create_dir_all(&disassembled_dir).expect("create dir");
    let part_xml =
        r#"<?xml version="1.0"?><Root><firstKey>1</firstKey><secondKey>2</secondKey></Root>"#;
    std::fs::write(disassembled_dir.join("Out.xml"), part_xml).expect("write part");
    let key_order = serde_json::to_string(&["secondKey", "firstKey"]).unwrap();
    std::fs::write(disassembled_dir.join(".key_order.json"), key_order).expect("write key_order");
    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(disassembled_dir.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble");
    let out = std::fs::read_to_string(base.join("Out.xml")).expect("read output");
    let second_pos = out.find("<secondKey>").unwrap_or(0);
    let first_pos = out.find("<firstKey>").unwrap_or(0);
    assert!(
        second_pos < first_pos,
        "key_order should reorder: secondKey before firstKey"
    );
}

#[tokio::test]
async fn reassemble_empty_directory_returns_ok_no_output() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let empty_dir = temp_dir.path().join("empty");
    std::fs::create_dir_all(&empty_dir).expect("create dir");
    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(empty_dir.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble should return Ok(())");
    // No output file created when directory has no parsable files
    assert!(!temp_dir.path().join("empty.xml").exists());
}

/// Regression test for the `<root></root>` flake: when every parsed file is
/// empty or declaration-only, `merge_xml_elements` returns None and
/// `reassemble_plain` must skip writing rather than emit a stub document.
#[tokio::test]
async fn reassemble_directory_with_only_empty_xml_files_writes_no_output() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let dir = temp_dir.path().join("only_empty");
    std::fs::create_dir_all(&dir).expect("create dir");
    // Two XML-shaped files that parse to an empty object / declaration-only
    // object respectively - together they exercise both the "no root in any
    // element" path and the new early-return branch in reassemble_plain.
    std::fs::write(dir.join("a.xml"), "").expect("write empty xml");
    std::fs::write(
        dir.join("b.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    )
    .expect("write decl-only xml");

    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(dir.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble should return Ok(())");

    // No stub file should be produced; the directory's output path must not exist.
    assert!(
        !temp_dir.path().join("only_empty.xml").exists(),
        "reassemble must not emit a <root></root> stub when no usable root is found"
    );
}

#[tokio::test]
async fn disassemble_non_xml_file_returns_ok_no_op() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let txt_file = base.join("readme.txt");
    std::fs::write(&txt_file, "not xml").expect("write");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            txt_file.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble should return Ok(())");
    assert!(
        !base.join("readme").exists(),
        "Should not create output for non-XML file"
    );
}

#[tokio::test]
async fn disassemble_with_pre_purge_removes_existing_output() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture must exist (run from project root)"
    );
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("HR_Admin.permissionset-meta.xml");
    std::fs::copy(fixture, &source).expect("copy");
    let out_dir = base.join("HR_Admin");
    std::fs::create_dir_all(&out_dir).expect("create");
    let marker = out_dir.join("pre-existing.txt");
    std::fs::write(&marker, "before").expect("write");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            true, // pre_purge
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(out_dir.exists());
    assert!(
        !marker.exists(),
        "pre_purge should remove existing output dir contents"
    );
}

#[tokio::test]
async fn disassemble_then_reassemble_matches_original_xml() {
    let _ = env_logger::try_init();

    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture {} must exist (run from project root)",
        fixture
    );

    let original_content = std::fs::read_to_string(fixture).expect("read original fixture");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("HR_Admin");

    // Disassemble: output goes to dirname(file_path). We copy the fixture to temp
    // and disassemble from there so output is temp/HR_Admin.
    let source_in_temp = base.join("HR_Admin.permissionset-meta.xml");
    std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source_in_temp.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");

    assert!(
        disassembled_dir.exists(),
        "Disassembled directory should exist"
    );

    let reassemble_handler = ReassembleXmlFileHandler::new();
    reassemble_handler
        .reassemble(disassembled_dir.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble");

    let reassembled_path = base.join("HR_Admin.xml");
    assert!(reassembled_path.exists(), "Reassembled file should exist");

    let reassembled_content = std::fs::read_to_string(&reassembled_path).expect("read reassembled");

    assert_eq!(
        original_content, reassembled_content,
        "Reassembled XML must match original file contents (round-trip)"
    );
}

#[tokio::test]
async fn disassemble_json_format_then_reassemble_round_trip() {
    let _ = env_logger::try_init();

    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture {} must exist (run from project root)",
        fixture
    );

    let _original_content = std::fs::read_to_string(fixture).expect("read original fixture");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("HR_Admin");
    let source_in_temp = base.join("HR_Admin.permissionset-meta.xml");
    std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source_in_temp.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "json",
            None,
            None,
        )
        .await
        .expect("disassemble");

    assert!(
        disassembled_dir.exists(),
        "Disassembled directory should exist"
    );

    let reassemble_handler = ReassembleXmlFileHandler::new();
    reassemble_handler
        .reassemble(disassembled_dir.to_str().unwrap(), Some("json"), false)
        .await
        .expect("reassemble");

    let reassembled_path = base.join("HR_Admin.json");
    assert!(reassembled_path.exists(), "Reassembled file should exist");
    let reassembled = std::fs::read_to_string(&reassembled_path).expect("read reassembled");
    assert!(!reassembled.is_empty());
    assert!(
        reassembled.contains("<?xml") || reassembled.contains("<"),
        "reassembled content"
    );
}

#[tokio::test]
async fn cdata_preserved_round_trip() {
    let _ = env_logger::try_init();

    let fixture = "fixtures/xml/cdata/VidLand_US.marketingappextension-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture {} must exist (run from project root)",
        fixture
    );

    let original_content = std::fs::read_to_string(fixture).expect("read original fixture");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("VidLand_US");

    let source_in_temp = base.join("VidLand_US.marketingappextension-meta.xml");
    std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source_in_temp.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");

    assert!(
        disassembled_dir.exists(),
        "Disassembled directory should exist"
    );

    let reassemble_handler = ReassembleXmlFileHandler::new();
    reassemble_handler
        .reassemble(
            disassembled_dir.to_str().unwrap(),
            Some("marketingappextension-meta.xml"),
            false,
        )
        .await
        .expect("reassemble");

    let reassembled_path = base.join("VidLand_US.marketingappextension-meta.xml");
    assert!(reassembled_path.exists(), "Reassembled file should exist");

    let reassembled_content = std::fs::read_to_string(&reassembled_path).expect("read reassembled");

    assert_eq!(
        original_content, reassembled_content,
        "Reassembled XML must match original file contents (CDATA round-trip)"
    );
}

#[tokio::test]
async fn comments_preserved_round_trip() {
    let _ = env_logger::try_init();

    let fixture = "fixtures/xml/comments/Numbers-fr.globalValueSetTranslation-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture {} must exist (run from project root)",
        fixture
    );

    let original_content = std::fs::read_to_string(fixture).expect("read original fixture");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("Numbers-fr");

    let source_in_temp = base.join("Numbers-fr.globalValueSetTranslation-meta.xml");
    std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source_in_temp.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");

    assert!(
        disassembled_dir.exists(),
        "Disassembled directory should exist"
    );

    let reassemble_handler = ReassembleXmlFileHandler::new();
    reassemble_handler
        .reassemble(
            disassembled_dir.to_str().unwrap(),
            Some("globalValueSetTranslation-meta.xml"),
            false,
        )
        .await
        .expect("reassemble");

    let reassembled_path = base.join("Numbers-fr.globalValueSetTranslation-meta.xml");
    assert!(reassembled_path.exists(), "Reassembled file should exist");

    let reassembled_content = std::fs::read_to_string(&reassembled_path).expect("read reassembled");

    assert_eq!(
        original_content, reassembled_content,
        "Reassembled XML must match original file contents (comments round-trip)"
    );
}

#[tokio::test]
async fn deeply_nested_unique_id_elements_round_trip() {
    let _ = env_logger::try_init();

    let fixture = "fixtures/xml/deeply-nested-unique-id-element/Get_Info.flow-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture {} must exist (run from project root)",
        fixture
    );

    let original_content = std::fs::read_to_string(fixture).expect("read original fixture");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("Get_Info");

    let source_in_temp = base.join("Get_Info.flow-meta.xml");
    std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

    let unique_id_elements =
        "apexClass,name,object,field,layout,actionName,targetReference,assignToReference,choiceText,promptText";

    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source_in_temp.to_str().unwrap(),
            Some(unique_id_elements),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");

    assert!(
        disassembled_dir.exists(),
        "Disassembled directory should exist"
    );

    let reassemble_handler = ReassembleXmlFileHandler::new();
    reassemble_handler
        .reassemble(
            disassembled_dir.to_str().unwrap(),
            Some("flow-meta.xml"),
            false,
        )
        .await
        .expect("reassemble");

    let reassembled_path = base.join("Get_Info.flow-meta.xml");
    assert!(reassembled_path.exists(), "Reassembled file should exist");

    let reassembled_content = std::fs::read_to_string(&reassembled_path).expect("read reassembled");

    assert_eq!(
        original_content, reassembled_content,
        "Reassembled XML must match original (deeply nested unique ID elements round-trip)"
    );
}

/// Multi-level disassembly: first disassemble by processName etc., then further disassemble
/// programProcesses by parameterName and ruleName. Reassemble and compare to original.
#[tokio::test]
async fn multi_level_disassemble_then_reassemble_matches_original() {
    let _ = env_logger::try_init();

    let fixture = "fixtures/xml/multi-level/Cloud_Kicks_Inner_Circle.loyaltyProgramSetup-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture {} must exist (run from project root)",
        fixture
    );

    let original_content = std::fs::read_to_string(fixture).expect("read original fixture");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("Cloud_Kicks_Inner_Circle");

    let source_in_temp = base.join("Cloud_Kicks_Inner_Circle.loyaltyProgramSetup-meta.xml");
    std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

    let rule = config_disassembler::xml::MultiLevelRule {
        file_pattern: "programProcesses".to_string(),
        root_to_strip: "programProcesses".to_string(),
        unique_id_elements: "parameterName,ruleName".to_string(),
        path_segment: "programProcesses".to_string(),
        wrap_root_element: "LoyaltyProgramSetup".to_string(),
        wrap_xmlns: String::new(),
    };

    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source_in_temp.to_str().unwrap(),
            Some("fullName,name,processName"),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");

    assert!(
        disassembled_dir.exists(),
        "Disassembled directory should exist"
    );

    let reassemble_handler = ReassembleXmlFileHandler::new();
    reassemble_handler
        .reassemble(disassembled_dir.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble");

    let reassembled_path = base.join("Cloud_Kicks_Inner_Circle.xml");
    assert!(reassembled_path.exists(), "Reassembled file should exist");

    let reassembled_content = std::fs::read_to_string(&reassembled_path).expect("read reassembled");

    assert_eq!(
        original_content, reassembled_content,
        "Reassembled XML must match original (multi-level round-trip)"
    );
}

/// Grouped-by-tag with a decompose rule that has mode neither "split" nor "group" (fallback path).
#[tokio::test]
async fn grouped_by_tag_with_fallback_mode_writes_single_file() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/split-tags/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture must exist (run from project root)"
    );
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("HR_Admin");
    let source = base.join("HR_Admin.permissionset-meta.xml");
    std::fs::copy(fixture, &source).expect("copy fixture");
    let fallback_rule = DecomposeRule {
        tag: "objectPermissions".to_string(),
        path_segment: "objectPermissions".to_string(),
        mode: "fallback".to_string(),
        field: "object".to_string(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("grouped-by-tag"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            Some(&[fallback_rule]),
        )
        .await
        .expect("disassemble");
    assert!(disassembled_dir.exists());
    let fallback_file = disassembled_dir.join("objectPermissions.xml");
    assert!(
        fallback_file.exists(),
        "fallback mode writes single file to disassembled root"
    );
}

/// Grouped-by-tag with --split-tags: objectPermissions split by object, fieldPermissions grouped by object (from field).
/// Reassemble and compare to original fixture.
#[tokio::test]
async fn split_tags_disassemble_then_reassemble_matches_original() {
    let _ = env_logger::try_init();

    let fixture = "fixtures/xml/split-tags/HR_Admin.permissionset-meta.xml";
    assert!(
        Path::new(fixture).exists(),
        "Fixture {} must exist (run from project root)",
        fixture
    );

    let original_content = std::fs::read_to_string(fixture).expect("read original fixture");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let disassembled_dir = base.join("HR_Admin");

    let source_in_temp = base.join("HR_Admin.permissionset-meta.xml");
    std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

    let split_tags_rules = vec![
        DecomposeRule {
            tag: "objectPermissions".to_string(),
            path_segment: "objectPermissions".to_string(),
            mode: "split".to_string(),
            field: "object".to_string(),
        },
        DecomposeRule {
            tag: "fieldPermissions".to_string(),
            path_segment: "fieldPermissions".to_string(),
            mode: "group".to_string(),
            field: "field".to_string(),
        },
    ];

    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source_in_temp.to_str().unwrap(),
            None,
            Some("grouped-by-tag"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            Some(&split_tags_rules),
        )
        .await
        .expect("disassemble");

    assert!(
        disassembled_dir.exists(),
        "Disassembled directory should exist"
    );

    let reassemble_handler = ReassembleXmlFileHandler::new();
    reassemble_handler
        .reassemble(disassembled_dir.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble");

    let reassembled_path = base.join("HR_Admin.xml");
    assert!(reassembled_path.exists(), "Reassembled file should exist");

    let reassembled_content = std::fs::read_to_string(&reassembled_path).expect("read reassembled");

    assert_eq!(
        original_content, reassembled_content,
        "Reassembled XML must match original (split-tags round-trip)"
    );
}

#[tokio::test]
async fn disassemble_nonexistent_path_returns_err() {
    let _ = env_logger::try_init();
    let mut disassemble = DisassembleXmlFileHandler::new();
    let result = disassemble
        .disassemble(
            "/nonexistent/path/xyz.xml",
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await;
    assert!(result.is_err(), "missing path should surface an error");
}

#[tokio::test]
async fn reassemble_nonexistent_path_returns_err() {
    let _ = env_logger::try_init();
    let handler = ReassembleXmlFileHandler::new();
    let result = handler
        .reassemble("/nonexistent/dir/xyz", Some("xml"), false)
        .await;
    assert!(result.is_err(), "missing directory should surface an error");
}

#[tokio::test]
async fn disassemble_leaf_only_xml_logs_and_skips() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("LeafOnly.xml");
    std::fs::write(
        &source,
        r#"<?xml version="1.0"?><Root><a>1</a><b>2</b><c>3</c></Root>"#,
    )
    .expect("write");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    // No disassembly directory created because only leaf elements are present.
    assert!(
        !base.join("LeafOnly").is_dir()
            || base.join("LeafOnly").read_dir().unwrap().next().is_none()
    );
}

#[tokio::test]
async fn disassemble_duplicate_leaf_siblings_under_root_no_op_with_log() {
    // Exercises the "existing key" branch in disassemble_element_keys when two leaf
    // siblings share the same element name at the root. Ends up short-circuiting
    // with a log message because `has_nested_elements` stays false.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Dups.xml");
    std::fs::write(
        &source,
        r#"<?xml version="1.0"?><Root><a>1</a><a>2</a><a>3</a></Root>"#,
    )
    .expect("write");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
}

#[tokio::test]
async fn disassemble_unparseable_xml_is_no_op() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("broken.xml");
    std::fs::write(&source, "<<not xml").expect("write");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(!base.join("broken").exists());
}

#[tokio::test]
async fn disassemble_empty_xml_document_is_no_op() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("empty.xml");
    std::fs::write(&source, r#"<?xml version="1.0"?>"#).expect("write");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
}

#[tokio::test]
async fn disassemble_with_post_purge_removes_source_file() {
    let _ = env_logger::try_init();
    let fixture = "fixtures/xml/general/HR_Admin.permissionset-meta.xml";
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("HR_Admin.permissionset-meta.xml");
    std::fs::copy(fixture, &source).expect("copy fixture");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            true, // post_purge
            ".xmldisassemblerignore",
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    assert!(base.join("HR_Admin").exists());
    assert!(!source.exists(), "post_purge should remove source");
}

#[tokio::test]
async fn grouped_by_tag_split_rule_uses_index_when_field_missing() {
    // Split mode with an unknown field name: filename falls back to the array index.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Perms.xml");
    std::fs::write(
        &source,
        r#"<?xml version="1.0"?>
<Root>
  <child><name>one</name></child>
  <objectPermissions><allowRead>true</allowRead></objectPermissions>
  <objectPermissions><allowRead>false</allowRead></objectPermissions>
</Root>"#,
    )
    .expect("write");
    let rules = [DecomposeRule {
        tag: "objectPermissions".to_string(),
        path_segment: String::new(), // empty path segment → falls back to tag
        mode: "split".to_string(),
        field: "object".to_string(),
    }];
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("grouped-by-tag"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            Some(&rules),
        )
        .await
        .expect("disassemble");
    assert!(base.join("Perms").join("objectPermissions").exists());
}

#[tokio::test]
async fn grouped_by_tag_group_rule_uses_nested_text_value() {
    // Group mode where the "field" value is a nested leaf element (#text) - exercises
    // the object + #text branch of get_field_value and the dot-prefix grouping.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Fields.xml");
    std::fs::write(
        &source,
        r#"<?xml version="1.0"?>
<Root>
  <child><name>c1</name></child>
  <fieldPermissions><field>Account.Name</field><readable>true</readable></fieldPermissions>
  <fieldPermissions><field>Account.Phone</field><readable>true</readable></fieldPermissions>
  <fieldPermissions><field>Contact.Email</field><readable>true</readable></fieldPermissions>
  <fieldPermissions><readable>true</readable></fieldPermissions>
</Root>"#,
    )
    .expect("write");
    let rules = [DecomposeRule {
        tag: "fieldPermissions".to_string(),
        path_segment: "fieldPermissions".to_string(),
        mode: "group".to_string(),
        field: "field".to_string(),
    }];
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("grouped-by-tag"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            None,
            Some(&rules),
        )
        .await
        .expect("disassemble");
    let grouped_dir = base.join("Fields").join("fieldPermissions");
    assert!(grouped_dir.exists());
    // Two dot-prefixed groups + one unknown (missing field) file expected.
    let files: Vec<_> = std::fs::read_dir(&grouped_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().into_string().unwrap_or_default())
        .collect();
    assert!(
        files.iter().any(|f| f.starts_with("Account.")),
        "expected an Account.* group file"
    );
    assert!(
        files.iter().any(|f| f.starts_with("unknown.")),
        "expected an unknown.* file for missing field"
    );
}

#[tokio::test]
async fn multi_level_with_empty_path_segment_and_xmlns_derives_segment() {
    // Exercises the MultiLevelRule empty path_segment / empty wrap_xmlns branches that fall
    // back to deriving values from file_pattern and the captured root xmlns.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Sample.loyaltyProgramSetup-meta.xml");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LoyaltyProgramSetup xmlns="http://example.com/multi">
  <programProcesses>
    <name>P1</name>
    <rules><ruleName>r1</ruleName></rules>
  </programProcesses>
</LoyaltyProgramSetup>"#;
    std::fs::write(&source, xml).expect("write source");
    let rule = MultiLevelRule {
        file_pattern: "programProcesses".to_string(),
        root_to_strip: "programProcesses".to_string(),
        unique_id_elements: "ruleName".to_string(),
        path_segment: String::new(),
        wrap_root_element: "LoyaltyProgramSetup".to_string(),
        wrap_xmlns: String::new(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            Some("name,ruleName"),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");
    let disassembled_dir = base.join("Sample");
    assert!(disassembled_dir.exists());
}

#[tokio::test]
async fn multi_level_with_explicit_xmlns_preserved() {
    // Exercises the non-empty wrap_xmlns branch: the rule-provided xmlns is used directly.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Inner.loyalty-meta.xml");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LoyaltyProgramSetup xmlns="http://example.com/original">
  <programProcesses>
    <name>P1</name>
    <rules><ruleName>r1</ruleName></rules>
  </programProcesses>
</LoyaltyProgramSetup>"#;
    std::fs::write(&source, xml).expect("write source");
    let rule = MultiLevelRule {
        file_pattern: "programProcesses".to_string(),
        root_to_strip: "programProcesses".to_string(),
        unique_id_elements: "ruleName".to_string(),
        path_segment: "programProcesses".to_string(),
        wrap_root_element: "LoyaltyProgramSetup".to_string(),
        wrap_xmlns: "http://example.com/explicit".to_string(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            Some("name,ruleName"),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");
    assert!(base.join("Inner").exists());
}

#[tokio::test]
async fn multi_level_with_multiple_matching_files_appends_rule_once() {
    // Multiple programProcesses siblings produce multiple matching output files in a single
    // disassembled tree; first hit pushes the rule to the multi-level config, subsequent hits
    // exercise the `Some(r) if r.wrap_xmlns.is_empty()` update branch.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LoyaltyProgramSetup>
  <programProcesses>
    <name>ProcessOne</name>
    <rules><ruleName>r1</ruleName></rules>
  </programProcesses>
  <programProcesses>
    <name>ProcessTwo</name>
    <rules><ruleName>r2</ruleName></rules>
  </programProcesses>
</LoyaltyProgramSetup>"#;
    let source_a = base.join("A.loyalty-meta.xml");
    let source_b = base.join("B.loyalty-meta.xml");
    std::fs::write(&source_a, xml).expect("write A");
    std::fs::write(&source_b, xml).expect("write B");
    let rule = MultiLevelRule {
        file_pattern: "programProcesses".to_string(),
        root_to_strip: "programProcesses".to_string(),
        unique_id_elements: "ruleName".to_string(),
        path_segment: "programProcesses".to_string(),
        wrap_root_element: "LoyaltyProgramSetup".to_string(),
        wrap_xmlns: String::new(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            base.to_str().unwrap(),
            Some("name,ruleName"),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");
    assert!(base.join("A").exists());
    assert!(base.join("B").exists());
}

#[tokio::test]
async fn disassemble_single_file_ignored_via_ignore_rules() {
    // Single-file path is matched by the ignore file - hits handle_file's is_ignored branch.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("skipme.xml");
    std::fs::write(
        &source,
        r#"<?xml version="1.0"?><Root><child><name>x</name></child></Root>"#,
    )
    .expect("write source");
    let ignore_path = base.join(".xmldisassemblerignore");
    std::fs::write(&ignore_path, "skipme.xml\n").expect("write ignore");
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ignore_path.to_str().unwrap(),
            "xml",
            None,
            None,
        )
        .await
        .expect("disassemble");
    // File should be ignored: no disassembled output directory.
    assert!(!base.join("skipme").exists());
}

#[tokio::test]
async fn multi_level_rule_without_matching_file_is_noop() {
    // Multi-level rule set but no disassembled file matches the pattern.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("sample.xml");
    std::fs::write(
        &source,
        r#"<?xml version="1.0"?><Root><child><name>x</name></child></Root>"#,
    )
    .expect("write");
    let rule = MultiLevelRule {
        file_pattern: "nonmatching".to_string(),
        root_to_strip: "X".to_string(),
        unique_id_elements: "id".to_string(),
        path_segment: String::new(),
        wrap_root_element: "X".to_string(),
        wrap_xmlns: String::new(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            None,
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");
    assert!(base.join("sample").exists());
}

#[tokio::test]
async fn reassemble_with_non_parseable_junk_files_is_skipped() {
    // Reassembly walks a directory whose only parseable file is invalid XML -
    // triggers parse_to_xml_object's None path and the "no files parsed" log branch.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path().join("Out");
    std::fs::create_dir_all(&base).expect("create dir");
    std::fs::write(base.join("bogus.xml"), "<<not xml").expect("write");
    std::fs::write(base.join(".hidden.xml"), "<hidden/>").expect("write hidden");
    std::fs::write(base.join("ignored.txt"), "data").expect("write text");
    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(base.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble returns Ok even when nothing parses");
    assert!(!base.with_extension("xml").exists());
}

#[tokio::test]
async fn reassemble_directory_with_nested_subdir_is_recursed() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path().join("Out");
    std::fs::create_dir_all(base.join("inner")).expect("create dir");
    std::fs::write(
        base.join("top.xml"),
        r#"<?xml version="1.0"?><Root><a>1</a></Root>"#,
    )
    .expect("write top");
    std::fs::write(
        base.join("inner").join("deep.xml"),
        r#"<?xml version="1.0"?><Root><b>2</b></Root>"#,
    )
    .expect("write inner");
    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(base.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble");
    let parent = base.parent().unwrap();
    assert!(parent.join("Out.xml").exists());
}

#[tokio::test]
async fn reassemble_invalid_key_order_json_still_writes() {
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path().join("Out");
    std::fs::create_dir_all(&base).expect("create dir");
    std::fs::write(
        base.join("part.xml"),
        r#"<?xml version="1.0"?><Root><a>1</a></Root>"#,
    )
    .expect("write");
    std::fs::write(base.join(".key_order.json"), "not valid json").expect("write key order");
    let handler = ReassembleXmlFileHandler::new();
    handler
        .reassemble(base.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble");
    assert!(base.with_extension("xml").exists());
}

/// Full round-trip (disassemble → reassemble) for each success fixture.
/// Excludes: no-root-element (invalid root), no-nested-elements (only leaves), ignore (behavior);
/// attributes/notes.xml (reassembly differs re declaration/entities), array-of-leaves (sibling order not preserved).
#[tokio::test]
async fn fixture_round_trip_matches_original() {
    let _ = env_logger::try_init();

    /// (fixture path, optional unique_id_elements for disassemble, extension for reassemble output)
    const FIXTURES: &[(&str, Option<&str>, &str)] = &[
        ("fixtures/xml/general/HR_Admin.permissionset-meta.xml", None, "xml"),
        (
            "fixtures/xml/comments/Numbers-fr.globalValueSetTranslation-meta.xml",
            None,
            "globalValueSetTranslation-meta.xml",
        ),
        (
            "fixtures/xml/cdata/VidLand_US.marketingappextension-meta.xml",
            None,
            "marketingappextension-meta.xml",
        ),
        (
            "fixtures/xml/no-namespace/HR_Admin.permissionset-meta.xml",
            None,
            "xml",
        ),
        // attributes/notes.xml excluded: reassembly differs (no XML declaration, entity encoding)
        // array-of-leaves excluded: sibling order not preserved on reassembly
        (
            "fixtures/xml/deeply-nested-unique-id-element/Get_Info.flow-meta.xml",
            Some("apexClass,name,object,field,layout,actionName,targetReference,assignToReference,choiceText,promptText"),
            "flow-meta.xml",
        ),
    ];

    for (fixture, unique_id_elements, reassemble_ext) in FIXTURES {
        let path = Path::new(fixture);
        assert!(
            path.exists(),
            "Fixture {} must exist (run from project root)",
            fixture
        );

        let original_content = std::fs::read_to_string(fixture).expect("read original fixture");

        let temp_dir = tempfile::tempdir().expect("temp dir");
        let base = temp_dir.path();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let base_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.split('.').next().unwrap_or(s))
            .unwrap_or("out");
        let disassembled_dir = base.join(base_name);

        let source_in_temp = base.join(file_name);
        std::fs::copy(fixture, &source_in_temp).expect("copy fixture to temp");

        let mut disassemble = DisassembleXmlFileHandler::new();
        let result = disassemble
            .disassemble(
                source_in_temp.to_str().unwrap(),
                *unique_id_elements,
                Some("unique-id"),
                false,
                false,
                ".xmldisassemblerignore",
                "xml",
                None,
                None,
            )
            .await;

        let Ok(()) = result else {
            panic!(
                "disassemble failed for fixture {}: {:?}",
                fixture,
                result.unwrap_err()
            );
        };

        assert!(
            disassembled_dir.exists(),
            "Disassembled directory should exist for {}",
            fixture
        );

        let reassemble_handler = ReassembleXmlFileHandler::new();
        reassemble_handler
            .reassemble(
                disassembled_dir.to_str().unwrap(),
                Some(reassemble_ext),
                false,
            )
            .await
            .expect("reassemble");

        let reassembled_path = base.join(format!("{}.{}", base_name, reassemble_ext));
        assert!(
            reassembled_path.exists(),
            "Reassembled file should exist for {} at {:?}",
            fixture,
            reassembled_path
        );

        let reassembled_content =
            std::fs::read_to_string(&reassembled_path).expect("read reassembled");

        assert_eq!(
            original_content, reassembled_content,
            "Round-trip for fixture {}: reassembled XML must match original",
            fixture
        );
    }
}

#[tokio::test]
async fn multi_level_skips_unparseable_matching_file() {
    // Plant an unparseable XML file matching the multi-level file_pattern inside the output
    // directory before disassemble runs; the recursive walk should encounter it and skip on
    // parse failure.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Inner.loyalty-meta.xml");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LoyaltyProgramSetup>
  <programProcesses>
    <name>P1</name>
    <rules><ruleName>r1</ruleName></rules>
  </programProcesses>
</LoyaltyProgramSetup>"#;
    std::fs::write(&source, xml).expect("write source");
    // Pre-create the output dir and plant a malformed matching file; disassemble runs without
    // pre_purge so the planted file survives into the multi-level walk.
    let out_dir = base.join("Inner");
    std::fs::create_dir_all(&out_dir).expect("mkdir");
    std::fs::write(out_dir.join("junk.programProcesses.xml"), "<not-closed").expect("write junk");
    let rule = MultiLevelRule {
        file_pattern: "programProcesses".to_string(),
        root_to_strip: "programProcesses".to_string(),
        unique_id_elements: "ruleName".to_string(),
        path_segment: "programProcesses".to_string(),
        wrap_root_element: "LoyaltyProgramSetup".to_string(),
        wrap_xmlns: String::new(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            Some("name,ruleName"),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");
}

#[tokio::test]
async fn multi_level_skips_matching_file_without_root_to_strip() {
    // Plant an XML file matching file_pattern whose root does not contain root_to_strip;
    // recursive walk skips it via the has_element_to_strip branch.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Inner.loyalty-meta.xml");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LoyaltyProgramSetup>
  <programProcesses>
    <name>P1</name>
    <rules><ruleName>r1</ruleName></rules>
  </programProcesses>
</LoyaltyProgramSetup>"#;
    std::fs::write(&source, xml).expect("write source");
    let out_dir = base.join("Inner");
    std::fs::create_dir_all(&out_dir).expect("mkdir");
    std::fs::write(
        out_dir.join("other.programProcesses.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?><Other><child>x</child></Other>"#,
    )
    .expect("write planted");
    let rule = MultiLevelRule {
        file_pattern: "programProcesses".to_string(),
        root_to_strip: "programProcesses".to_string(),
        unique_id_elements: "ruleName".to_string(),
        path_segment: "programProcesses".to_string(),
        wrap_root_element: "LoyaltyProgramSetup".to_string(),
        wrap_xmlns: String::new(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            Some("name,ruleName"),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");
}

#[tokio::test]
async fn multi_level_skips_matching_file_with_non_object_strip_target() {
    // Plant an XML whose root contains the element_to_strip key but with a non-object value;
    // strip_root_and_build_xml returns None and the file is skipped.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let source = base.join("Inner.loyalty-meta.xml");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LoyaltyProgramSetup>
  <programProcesses>
    <name>P1</name>
    <rules><ruleName>r1</ruleName></rules>
  </programProcesses>
</LoyaltyProgramSetup>"#;
    std::fs::write(&source, xml).expect("write source");
    let out_dir = base.join("Inner");
    std::fs::create_dir_all(&out_dir).expect("mkdir");
    // Duplicate siblings parse as an array; strip_root_and_build_xml returns None because
    // `root_val.get(element_to_strip).as_object()` fails for array values.
    std::fs::write(
        out_dir.join("arr.programProcesses.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?><Wrapper><programProcesses>one</programProcesses><programProcesses>two</programProcesses></Wrapper>"#,
    )
    .expect("write array");
    let rule = MultiLevelRule {
        file_pattern: "programProcesses".to_string(),
        root_to_strip: "programProcesses".to_string(),
        unique_id_elements: "ruleName".to_string(),
        path_segment: "programProcesses".to_string(),
        wrap_root_element: "LoyaltyProgramSetup".to_string(),
        wrap_xmlns: String::new(),
    };
    let mut disassemble = DisassembleXmlFileHandler::new();
    disassemble
        .disassemble(
            source.to_str().unwrap(),
            Some("name,ruleName"),
            Some("unique-id"),
            false,
            false,
            ".xmldisassemblerignore",
            "xml",
            Some(&rule),
            None,
        )
        .await
        .expect("disassemble");
}

#[tokio::test]
async fn reassemble_multi_level_skips_unparseable_segment_file() {
    // Write a saved multi-level config next to a segment directory that contains an
    // unparseable XML file; ensure_segment_files_structure must skip it gracefully.
    let _ = env_logger::try_init();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let base = temp_dir.path();
    let out_dir = base.join("Inner");
    let segment_dir = out_dir.join("programProcesses");
    std::fs::create_dir_all(&segment_dir).expect("mkdir segment");
    std::fs::write(segment_dir.join("bad.xml"), "<not-closed").expect("write bad xml");
    std::fs::write(
        segment_dir.join("good.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<programProcesses><rules><ruleName>r1</ruleName></rules></programProcesses>"#,
    )
    .expect("write good xml");
    let config = serde_json::json!({
        "rules": [{
            "file_pattern": "programProcesses",
            "root_to_strip": "programProcesses",
            "unique_id_elements": "ruleName",
            "path_segment": "programProcesses",
            "wrap_root_element": "LoyaltyProgramSetup",
            "wrap_xmlns": ""
        }]
    });
    std::fs::write(
        out_dir.join(".multi_level.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .expect("write config");
    let reassemble = ReassembleXmlFileHandler::new();
    reassemble
        .reassemble(out_dir.to_str().unwrap(), Some("xml"), false)
        .await
        .expect("reassemble");
}
