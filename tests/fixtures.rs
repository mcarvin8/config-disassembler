//! Data-driven fixture harness.
//!
//! Walks the `fixtures/` directory and runs a 3×3 cross-format round-trip
//! matrix on every scenario. Each scenario directory must contain exactly
//! one `input.<ext>` file (`json`, `json5`, `yaml`, or `yml`) and may
//! optionally contain a `case.json` file describing options:
//!
//! ```json
//! {
//!   "description": "human-readable summary",
//!   "unique_id": "name",
//!   "skip": "reason if this fixture should be skipped"
//! }
//! ```
//!
//! For every fixture we run, for each `(mid, out)` pair drawn from
//! `{JSON, JSON5, YAML} × {JSON, JSON5, YAML}`:
//!
//! 1. Disassemble the input file, writing split files in `mid` format.
//! 2. Reassemble back into a file using `out` format.
//! 3. Parse the reassembled output as `out`, parse the original as its
//!    detected input format, and assert structural (semantic) equality.
//!
//! All fixture failures are aggregated so a single failed test run reports
//! every broken case at once.

use std::fs;
use std::path::{Path, PathBuf};

use config_disassembler::disassemble::{disassemble, DisassembleOptions};
use config_disassembler::format::Format;
use config_disassembler::reassemble::{reassemble, ReassembleOptions};

/// Formats that participate in cross-format round-trips. TOML is
/// intentionally excluded because TOML can only be converted to and
/// from TOML; TOML fixtures are matrixed only against `[Format::Toml]`.
const CROSS_FORMATS: &[Format] = &[Format::Json, Format::Json5, Format::Yaml];
const TOML_ONLY: &[Format] = &[Format::Toml];

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Case {
    /// Human-readable summary; ignored at runtime, useful for grepping.
    #[allow(dead_code)]
    description: Option<String>,
    /// For array roots, the field name on each element to use for filenames.
    unique_id: Option<String>,
    /// If set, the fixture is skipped with this reason.
    skip: Option<String>,
}

#[test]
fn fixtures_roundtrip_matrix() {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    if !fixtures_dir.exists() {
        panic!(
            "fixtures directory does not exist at {}",
            fixtures_dir.display()
        );
    }

    let cases = collect_cases(&fixtures_dir);
    assert!(
        !cases.is_empty(),
        "no fixtures found under {}",
        fixtures_dir.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0usize;
    let mut skipped: Vec<String> = Vec::new();

    for fixture in &cases {
        if let Some(reason) = &fixture.case.skip {
            skipped.push(format!("{}: {reason}", fixture.label()));
            continue;
        }
        let formats: &[Format] = if fixture.input_format == Format::Toml {
            TOML_ONLY
        } else {
            CROSS_FORMATS
        };
        for &mid in formats {
            for &out in formats {
                ran += 1;
                if let Err(e) = run_one(fixture, mid, out) {
                    failures.push(format!(
                        "{} [in={} mid={} out={}]\n    {e}",
                        fixture.label(),
                        fixture.input_format,
                        mid,
                        out
                    ));
                }
            }
        }
    }

    eprintln!(
        "fixtures: {} fixtures, {} matrix runs, {} skipped",
        cases.len(),
        ran,
        skipped.len()
    );
    for skip in &skipped {
        eprintln!("  skipped: {skip}");
    }

    if !failures.is_empty() {
        let count = failures.len();
        let joined = failures.join("\n");
        panic!("\n{count} fixture matrix failures:\n{joined}\n");
    }
}

#[derive(Debug)]
struct Fixture {
    dir: PathBuf,
    input: PathBuf,
    input_format: Format,
    case: Case,
}

impl Fixture {
    fn label(&self) -> String {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        self.dir
            .strip_prefix(manifest)
            .unwrap_or(&self.dir)
            .display()
            .to_string()
            .replace('\\', "/")
    }
}

fn collect_cases(root: &Path) -> Vec<Fixture> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let dir = entry.path();
        let Some((input, format)) = find_input(dir) else {
            continue;
        };
        let case = read_case(dir);
        out.push(Fixture {
            dir: dir.to_path_buf(),
            input,
            input_format: format,
            case,
        });
    }
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

fn find_input(dir: &Path) -> Option<(PathBuf, Format)> {
    const EXTS: &[(&str, Format)] = &[
        ("json", Format::Json),
        ("json5", Format::Json5),
        ("yaml", Format::Yaml),
        ("yml", Format::Yaml),
        ("toml", Format::Toml),
    ];
    for (ext, format) in EXTS {
        let candidate = dir.join(format!("input.{ext}"));
        if candidate.is_file() {
            return Some((candidate, *format));
        }
    }
    None
}

fn read_case(dir: &Path) -> Case {
    let path = dir.join("case.json");
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            panic!("could not parse {}: {e}", path.display());
        }),
        Err(_) => Case::default(),
    }
}

fn run_one(fixture: &Fixture, mid: Format, out: Format) -> Result<(), String> {
    let tmp = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let split_dir = tmp.path().join("split");
    let rebuilt = tmp.path().join(format!("rebuilt.{}", out.extension()));

    disassemble(DisassembleOptions {
        input: fixture.input.clone(),
        input_format: Some(fixture.input_format),
        output_dir: Some(split_dir.clone()),
        output_format: Some(mid),
        unique_id: fixture.case.unique_id.clone(),
        pre_purge: false,
        post_purge: false,
    })
    .map_err(|e| format!("disassemble failed: {e}"))?;

    reassemble(ReassembleOptions {
        input_dir: split_dir,
        output: Some(rebuilt.clone()),
        output_format: Some(out),
        post_purge: false,
    })
    .map_err(|e| format!("reassemble failed: {e}"))?;

    let original = fixture
        .input_format
        .load(&fixture.input)
        .map_err(|e| format!("could not parse original {}: {e}", fixture.input.display()))?;
    let rebuilt_value = out
        .load(&rebuilt)
        .map_err(|e| format!("could not parse rebuilt {}: {e}", rebuilt.display()))?;

    if !values_equal(&original, &rebuilt_value) {
        return Err(format!(
            "round-tripped value did not match original\n--- original ---\n{}\n--- rebuilt ---\n{}",
            pretty(&original),
            pretty(&rebuilt_value)
        ));
    }
    Ok(())
}

fn pretty(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| format!("{v:?}"))
}

/// Semantic equality between two `Value`s.
///
/// Differs from `PartialEq` in one place: numbers compare equal whenever they
/// are mathematically equal, even if one was parsed as an integer and the
/// other as a float. This is necessary because cross-format round-trips can
/// legitimately convert `0.0` to `0` (e.g., the `json5` crate's serializer
/// drops trailing zeroes from integer-valued floats). For a configuration
/// disassembler the meaningful invariant is that the *value* is preserved,
/// not its textual representation.
fn values_equal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value::*;
    match (a, b) {
        (Null, Null) => true,
        (Bool(x), Bool(y)) => x == y,
        (String(x), String(y)) => x == y,
        (Number(x), Number(y)) => numbers_equal(x, y),
        (Array(x), Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Object(x), Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).map(|w| values_equal(v, w)).unwrap_or(false))
        }
        _ => false,
    }
}

fn numbers_equal(a: &serde_json::Number, b: &serde_json::Number) -> bool {
    if let (Some(ai), Some(bi)) = (a.as_i64(), b.as_i64()) {
        return ai == bi;
    }
    if let (Some(au), Some(bu)) = (a.as_u64(), b.as_u64()) {
        return au == bu;
    }
    match (a.as_f64(), b.as_f64()) {
        (Some(af), Some(bf)) => {
            if af.is_nan() && bf.is_nan() {
                true
            } else {
                af == bf
            }
        }
        _ => false,
    }
}
