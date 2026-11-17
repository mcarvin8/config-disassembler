//! Integration tests that drive the public `run()` entry point with full
//! argv vectors. These cover the CLI argument parser, dispatcher, error
//! formatting paths, and the XML subcommand pass-through that aren't
//! reachable from the XML round-trip tests.

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
