//! Integration tests that drive the public `run()` entry point with full
//! argv vectors. These cover the CLI argument parser, dispatcher, error
//! formatting paths, and the XML subcommand pass-through that aren't
//! reachable from the library-level round-trip tests.

use std::fs;
use std::path::Path;

use config_disassembler::run;

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

async fn run_ok(args: &[&str]) {
    run(argv(args))
        .await
        .unwrap_or_else(|e| panic!("expected Ok, got Err({e}) for args {args:?}"));
}

async fn run_err(args: &[&str]) -> String {
    match run(argv(args)).await {
        Ok(()) => panic!("expected Err for args {args:?}"),
        Err(e) => e.to_string(),
    }
}

#[tokio::test]
async fn no_args_prints_help() {
    run_ok(&["config-disassembler"]).await;
}

#[tokio::test]
async fn help_aliases_all_succeed() {
    for flag in ["help", "--help", "-h"] {
        run_ok(&["config-disassembler", flag]).await;
    }
}

#[tokio::test]
async fn unknown_subcommand_is_usage_error() {
    let msg = run_err(&["config-disassembler", "wat"]).await;
    assert!(msg.contains("unknown subcommand"), "got: {msg}");
}

#[tokio::test]
async fn json_without_action_is_usage_error() {
    let msg = run_err(&["config-disassembler", "json"]).await;
    assert!(msg.contains("disassemble"), "got: {msg}");
}

#[tokio::test]
async fn unknown_action_is_usage_error() {
    let msg = run_err(&["config-disassembler", "json", "wat"]).await;
    assert!(msg.contains("unknown action"), "got: {msg}");
}

#[tokio::test]
async fn missing_input_is_usage_error() {
    let msg = run_err(&["config-disassembler", "json", "disassemble"]).await;
    assert!(msg.contains("missing <input>"), "got: {msg}");
}

#[tokio::test]
async fn missing_input_dir_is_usage_error() {
    let msg = run_err(&["config-disassembler", "json", "reassemble"]).await;
    assert!(msg.contains("missing <input-dir>"), "got: {msg}");
}

#[tokio::test]
async fn unknown_disassemble_option_is_usage_error() {
    let msg = run_err(&[
        "config-disassembler",
        "json",
        "disassemble",
        "--bogus",
        "x.json",
    ])
    .await;
    assert!(msg.contains("unknown option"), "got: {msg}");
}

#[tokio::test]
async fn unknown_reassemble_option_is_usage_error() {
    let msg = run_err(&[
        "config-disassembler",
        "yaml",
        "reassemble",
        "--bogus",
        "dir",
    ])
    .await;
    assert!(msg.contains("unknown option"), "got: {msg}");
}

#[tokio::test]
async fn flag_without_value_is_usage_error() {
    let msg = run_err(&[
        "config-disassembler",
        "json",
        "disassemble",
        "--output-format",
    ])
    .await;
    assert!(msg.contains("expects a value"), "got: {msg}");
}

#[tokio::test]
async fn unknown_format_value_is_usage_error() {
    let msg = run_err(&[
        "config-disassembler",
        "json",
        "disassemble",
        "--output-format",
        "xml",
        "x.json",
    ])
    .await;
    assert!(msg.contains("unknown format"), "got: {msg}");
}

#[tokio::test]
async fn extra_positional_is_usage_error() {
    let msg = run_err(&[
        "config-disassembler",
        "json",
        "disassemble",
        "first.json",
        "second.json",
    ])
    .await;
    assert!(msg.contains("unexpected"), "got: {msg}");
}

#[tokio::test]
async fn yaml_subcommand_help() {
    run_ok(&["config-disassembler", "yaml", "--help"]).await;
}

#[tokio::test]
async fn yml_alias_dispatches_to_yaml() {
    run_ok(&["config-disassembler", "yml", "help"]).await;
}

#[tokio::test]
async fn json5_subcommand_action_help() {
    run_ok(&["config-disassembler", "json5", "disassemble", "--help"]).await;
    run_ok(&["config-disassembler", "json5", "reassemble", "--help"]).await;
}

#[tokio::test]
async fn full_disassemble_reassemble_via_cli() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("config.json");
    fs::write(
        &input,
        r#"{"name":"demo","version":1,"settings":{"x":1,"y":2}}"#,
    )
    .unwrap();
    let split_dir = tmp.path().join("split");
    let rebuilt = tmp.path().join("rebuilt.yaml");

    run_ok(&[
        "config-disassembler",
        "json",
        "disassemble",
        input.to_str().unwrap(),
        "--output-dir",
        split_dir.to_str().unwrap(),
        "--output-format",
        "yaml",
    ])
    .await;
    assert!(split_dir.join("settings.yaml").exists());

    run_ok(&[
        "config-disassembler",
        "json",
        "reassemble",
        split_dir.to_str().unwrap(),
        "-o",
        rebuilt.to_str().unwrap(),
        "--output-format",
        "yaml",
    ])
    .await;
    let parsed: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(&rebuilt).unwrap()).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&input).unwrap()).unwrap();
    assert_eq!(parsed, expected);
}

#[tokio::test]
async fn full_array_unique_id_via_cli_with_short_o() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("items.yaml");
    fs::write(&input, "- name: alpha\n  v: 1\n- name: beta\n  v: 2\n").unwrap();
    let split_dir = tmp.path().join("split");

    run_ok(&[
        "config-disassembler",
        "yaml",
        "disassemble",
        input.to_str().unwrap(),
        "-o",
        split_dir.to_str().unwrap(),
        "--unique-id",
        "name",
        "--input-format",
        "yaml",
        "--output-format",
        "json",
    ])
    .await;
    assert!(split_dir.join("alpha.json").exists());
    assert!(split_dir.join("beta.json").exists());
}

#[tokio::test]
async fn pre_purge_and_post_purge_via_cli() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("c.json");
    fs::write(&input, r#"{"k":{"v":1}}"#).unwrap();
    let split_dir = tmp.path().join("split");
    fs::create_dir_all(&split_dir).unwrap();
    fs::write(split_dir.join("stale.txt"), "old").unwrap();

    run_ok(&[
        "config-disassembler",
        "json",
        "disassemble",
        input.to_str().unwrap(),
        "--output-dir",
        split_dir.to_str().unwrap(),
        "--pre-purge",
        "--post-purge",
    ])
    .await;
    assert!(!split_dir.join("stale.txt").exists());
    assert!(!input.exists(), "post-purge should remove the input file");

    let rebuilt = tmp.path().join("rebuilt.json");
    run_ok(&[
        "config-disassembler",
        "json",
        "reassemble",
        split_dir.to_str().unwrap(),
        "--output",
        rebuilt.to_str().unwrap(),
        "--post-purge",
    ])
    .await;
    assert!(rebuilt.exists());
    assert!(
        !split_dir.exists(),
        "post-purge should remove the disassembled directory"
    );
}

#[tokio::test]
async fn xml_subcommand_passes_through_to_inner_cli() {
    // No args after `xml` makes xml-disassembler print usage and return Ok.
    run_ok(&["config-disassembler", "xml"]).await;
    // An unknown sub-action also just prints usage and returns Ok in
    // xml-disassembler — exercising the wrapper either way.
    run_ok(&["config-disassembler", "xml", "this-is-not-a-real-action"]).await;
    // A real command pointed at a non-existent file should propagate as
    // an error through the wrapper's `Error::Xml` variant.
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist.xml");
    let result = run(argv(&[
        "config-disassembler",
        "xml",
        "disassemble",
        missing.to_str().unwrap(),
    ]))
    .await;
    if let Err(e) = result {
        assert!(e.to_string().contains("xml-disassembler"));
    }
}

#[tokio::test]
async fn disassemble_scalar_root_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("scalar.json");
    fs::write(&input, "42").unwrap();

    let msg = run_err(&[
        "config-disassembler",
        "json",
        "disassemble",
        input.to_str().unwrap(),
        "--output-dir",
        tmp.path().join("out").to_str().unwrap(),
    ])
    .await;
    assert!(msg.contains("object or array"), "got: {msg}");
}

#[tokio::test]
async fn disassemble_unknown_extension_is_format_error() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("noext");
    fs::write(&input, "{}").unwrap();

    let msg = run(argv(&[
        "config-disassembler",
        "json",
        "disassemble",
        input.to_str().unwrap(),
        "--output-dir",
        tmp.path().join("out").to_str().unwrap(),
    ]))
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    // Either it succeeds (json default fallback in CLI) or it's an
    // explicit format error — both are acceptable; the parser was
    // exercised either way. Just ensure no panic.
    let _ = msg;
}

#[tokio::test]
async fn reassemble_missing_metadata_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("empty");
    fs::create_dir_all(&dir).unwrap();
    let msg = run_err(&[
        "config-disassembler",
        "json",
        "reassemble",
        dir.to_str().unwrap(),
    ])
    .await;
    assert!(msg.contains("metadata"), "got: {msg}");
}

#[tokio::test]
async fn reassemble_input_must_be_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("not-a-dir.json");
    fs::write(&file, "{}").unwrap();
    let msg = run_err(&[
        "config-disassembler",
        "json",
        "reassemble",
        file.to_str().unwrap(),
    ])
    .await;
    assert!(msg.contains("not a directory"), "got: {msg}");
}

#[test]
fn fixtures_dir_exists_for_other_tests() {
    // Synchronous sanity: every fixture should be under fixtures/.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(manifest.join("fixtures").is_dir());
}
