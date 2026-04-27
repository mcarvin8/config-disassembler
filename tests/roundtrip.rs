//! End-to-end round-trip tests for the JSON/JSON5/YAML disassembler.

use std::fs;
use std::path::PathBuf;

use config_disassembler::disassemble::{self, DisassembleOptions};
use config_disassembler::format::Format;
use config_disassembler::reassemble::{self, ReassembleOptions};
use serde_json::{json, Value};

fn write_input(dir: &std::path::Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

fn parse_value(format: Format, path: &std::path::Path) -> Value {
    format.load(path).unwrap()
}

#[test]
fn json_object_roundtrip_through_yaml_files() {
    let tmp = tempfile::tempdir().unwrap();
    let original = json!({
        "name": "demo",
        "version": 3,
        "enabled": true,
        "settings": {
            "retries": 5,
            "endpoints": ["a", "b", "c"]
        },
        "tags": ["alpha", "beta"]
    });
    let input = write_input(
        tmp.path(),
        "config.json",
        &serde_json::to_string_pretty(&original).unwrap(),
    );

    let disassembled = disassemble::disassemble(DisassembleOptions {
        input: input.clone(),
        input_format: Some(Format::Json),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Yaml),
        unique_id: None,
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    assert!(disassembled.join("settings.yaml").exists());
    assert!(disassembled.join("tags.yaml").exists());
    assert!(disassembled.join("_main.yaml").exists());
    assert!(disassembled
        .join(config_disassembler::meta::META_FILENAME)
        .exists());

    let output = reassemble::reassemble(ReassembleOptions {
        input_dir: disassembled,
        output: Some(tmp.path().join("rebuilt.json")),
        output_format: Some(Format::Json),
        post_purge: false,
    })
    .unwrap();

    let rebuilt = parse_value(Format::Json, &output);
    assert_eq!(rebuilt, original);
}

#[test]
fn yaml_array_roundtrip_with_unique_id() {
    let tmp = tempfile::tempdir().unwrap();
    let yaml = r#"
- name: alpha
  weight: 1
- name: beta
  weight: 2
- name: gamma
  weight: 3
"#;
    let input = write_input(tmp.path(), "items.yaml", yaml);

    let disassembled = disassemble::disassemble(DisassembleOptions {
        input: input.clone(),
        input_format: Some(Format::Yaml),
        output_dir: Some(tmp.path().join("items")),
        output_format: Some(Format::Json),
        unique_id: Some("name".into()),
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    assert!(disassembled.join("alpha.json").exists());
    assert!(disassembled.join("beta.json").exists());
    assert!(disassembled.join("gamma.json").exists());

    let output = reassemble::reassemble(ReassembleOptions {
        input_dir: disassembled,
        output: Some(tmp.path().join("rebuilt.yaml")),
        output_format: Some(Format::Yaml),
        post_purge: false,
    })
    .unwrap();

    let rebuilt = parse_value(Format::Yaml, &output);
    let original = parse_value(Format::Yaml, &input);
    assert_eq!(rebuilt, original);
}

#[test]
fn json5_roundtrip_preserves_structure() {
    let tmp = tempfile::tempdir().unwrap();
    let json5_text = r#"{
  // A small json5 sample
  name: 'sample',
  enabled: true,
  count: 42,
  servers: [
    { host: 'a.example.com', port: 8080 },
    { host: 'b.example.com', port: 8081 },
  ],
  meta: {
    region: 'us-east-1',
    flags: ['x', 'y'],
  },
}
"#;
    let input = write_input(tmp.path(), "config.json5", json5_text);

    let disassembled = disassemble::disassemble(DisassembleOptions {
        input: input.clone(),
        input_format: Some(Format::Json5),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Json5),
        unique_id: None,
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    let output = reassemble::reassemble(ReassembleOptions {
        input_dir: disassembled,
        output: Some(tmp.path().join("rebuilt.json5")),
        output_format: Some(Format::Json5),
        post_purge: false,
    })
    .unwrap();

    let rebuilt = parse_value(Format::Json5, &output);
    let original = parse_value(Format::Json5, &input);
    assert_eq!(rebuilt, original);
}

#[test]
fn toml_roundtrip_preserves_structure() {
    let tmp = tempfile::tempdir().unwrap();
    let toml_text = r#"title = "TOML Example"
version = 1
enabled = true

[owner]
name = "Tom"
joined = "1979-05-27"

[database]
server = "192.168.1.1"
ports = [8001, 8002]
connection_max = 5000
"#;
    let input = write_input(tmp.path(), "config.toml", toml_text);

    let disassembled = disassemble::disassemble(DisassembleOptions {
        input: input.clone(),
        input_format: Some(Format::Toml),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Toml),
        unique_id: None,
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    assert!(disassembled.join("owner.toml").exists());
    assert!(disassembled.join("database.toml").exists());
    assert!(disassembled.join("_main.toml").exists());

    let output = reassemble::reassemble(ReassembleOptions {
        input_dir: disassembled,
        output: Some(tmp.path().join("rebuilt.toml")),
        output_format: Some(Format::Toml),
        post_purge: false,
    })
    .unwrap();

    let rebuilt = parse_value(Format::Toml, &output);
    let original = parse_value(Format::Toml, &input);
    assert_eq!(rebuilt, original);
}

#[test]
fn toml_to_json_disassemble_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let input = write_input(tmp.path(), "c.toml", "[a]\nb = 1\n");

    let err = disassemble::disassemble(DisassembleOptions {
        input,
        input_format: Some(Format::Toml),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Json),
        unique_id: None,
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap_err();
    assert!(
        err.to_string().contains("TOML can only be converted"),
        "got: {err}"
    );
}

#[test]
fn toml_disassembled_dir_cannot_reassemble_to_json() {
    let tmp = tempfile::tempdir().unwrap();
    let input = write_input(
        tmp.path(),
        "c.toml",
        "name = \"demo\"\n\n[settings]\nx = 1\n",
    );

    let dir = disassemble::disassemble(DisassembleOptions {
        input,
        input_format: Some(Format::Toml),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Toml),
        unique_id: None,
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    let err = reassemble::reassemble(ReassembleOptions {
        input_dir: dir,
        output: Some(tmp.path().join("rebuilt.json")),
        output_format: Some(Format::Json),
        post_purge: false,
    })
    .unwrap_err();
    assert!(
        err.to_string().contains("TOML can only be reassembled"),
        "got: {err}"
    );
}

#[test]
fn corrupted_toml_wrapper_key_returns_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let input = write_input(
        tmp.path(),
        "c.toml",
        "name = \"demo\"\n\n[settings]\nx = 1\n",
    );

    let dir = disassemble::disassemble(DisassembleOptions {
        input,
        input_format: Some(Format::Toml),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Toml),
        unique_id: None,
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    // Replace settings.toml's wrapper key with an unexpected one so
    // reassembly's unwrap step has to report a clear error.
    fs::write(dir.join("settings.toml"), "[wrong]\nx = 1\n").unwrap();

    let err = reassemble::reassemble(ReassembleOptions {
        input_dir: dir,
        output: Some(tmp.path().join("rebuilt.toml")),
        output_format: Some(Format::Toml),
        post_purge: false,
    })
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("does not contain expected wrapper key"),
        "got: {msg}"
    );
    assert!(msg.contains("settings"), "got: {msg}");
}

#[test]
fn json_to_toml_reassemble_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let input = write_input(tmp.path(), "c.json", r#"{"a":{"b":1}}"#);

    let dir = disassemble::disassemble(DisassembleOptions {
        input,
        input_format: Some(Format::Json),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Json),
        unique_id: None,
        pre_purge: false,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    let err = reassemble::reassemble(ReassembleOptions {
        input_dir: dir,
        output: Some(tmp.path().join("rebuilt.toml")),
        output_format: Some(Format::Toml),
        post_purge: false,
    })
    .unwrap_err();
    assert!(
        err.to_string().contains("TOML can only be reassembled"),
        "got: {err}"
    );
}

#[test]
fn pre_purge_clears_output_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let input = write_input(tmp.path(), "c.json", r#"{"a": {"b": 1}}"#);
    let out = tmp.path().join("split");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("stale.txt"), "old data").unwrap();

    disassemble::disassemble(DisassembleOptions {
        input,
        input_format: Some(Format::Json),
        output_dir: Some(out.clone()),
        output_format: Some(Format::Json),
        unique_id: None,
        pre_purge: true,
        post_purge: false,
        ignore_path: None,
    })
    .unwrap();

    assert!(!out.join("stale.txt").exists());
    assert!(out.join("a.json").exists());
}

#[test]
fn post_purge_removes_input_file() {
    let tmp = tempfile::tempdir().unwrap();
    let input = write_input(tmp.path(), "c.json", r#"{"a": {"b": 1}}"#);

    disassemble::disassemble(DisassembleOptions {
        input: input.clone(),
        input_format: Some(Format::Json),
        output_dir: Some(tmp.path().join("split")),
        output_format: Some(Format::Json),
        unique_id: None,
        pre_purge: false,
        post_purge: true,
        ignore_path: None,
    })
    .unwrap();

    assert!(!input.exists());
}
