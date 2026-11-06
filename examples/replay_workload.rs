//! Replays sf-decomposer's crate workload so it can be profiled natively.
//!
//! sf-decomposer drives this crate through two napi calls per component:
//! `DisassembleXmlFileHandler::disassemble` on each parent `*-meta.xml`, then
//! `ReassembleXmlFileHandler::reassemble` on each disassembled directory. This
//! example makes the same calls with the same arguments (unique-id strategy,
//! per-type unique-id fields, prepurge/postpurge, up to 3 types in flight) over
//! a fixture tree from sf-decomposer's `scripts/gen-perf-fixtures.ts`, without
//! Node in the process -- so `perf`, `heaptrack`, etc. see only Rust frames.
//!
//! Usage (run `decompose` first; `recompose` consumes what it leaves behind):
//!
//! ```text
//! cargo run --release --example replay_workload -- decompose <fixture-root> [--format xml|json|json5|yaml]
//! cargo run --release --example replay_workload -- recompose <fixture-root>
//! ```
//!
//! `<fixture-root>` is the directory containing `force-app/`. It is modified in
//! place: decompose replaces each parent file with its shard directory and
//! writes `.replay-manifest.json`; recompose rebuilds the parent files from it.
//! See `.github/workflows/profile.yml` for the CI job that profiles both phases.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use config_disassembler::xml::{DisassembleXmlFileHandler, ReassembleXmlFileHandler};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// sf-decomposer's `CONCURRENCY_LIMITS.METADATA_TYPES`.
const TYPES_IN_FLIGHT: usize = 3;
const MANIFEST: &str = ".replay-manifest.json";
const IGNORE_FILE: &str = ".replay-no-ignore-file";

/// Unique-id fields sf-decomposer passes per metadata suffix: its default
/// `fullName,name` plus `src/metadata/uniqueIdElements.ts` (as of v7.11).
fn unique_id_elements(suffix: &str) -> String {
    let extra = match suffix {
        "permissionset" | "mutingpermissionset" => {
            "application,apexClass,externalDataSource,flow,object,apexPage,recordType,tab,field,agentName,externalCredentialPrincipal,servicePresenceStatus,configName"
        }
        "profile" => {
            "actionName+pageOrSobjectType+formFactor+recordType,actionName+pageOrSobjectType+formFactor,application,apexClass,externalDataSource,flow,object,apexPage,recordType,tab,field,startAddress,startAddress+endAddress,dataCategoryGroup,layout,weekdayStart,friendlyName,agentName,servicePresenceStatus,configName"
        }
        "flow" => {
            "apexClass,object,field,layout,actionName,targetReference,assignToReference,choiceText,promptText,label,flowName,text"
        }
        "app" => {
            "actionName+pageOrSobjectType+formFactor+profile+recordType,actionName+pageOrSobjectType+formFactor+profile,actionName+pageOrSobjectType+formFactor+recordType,actionName+pageOrSobjectType+formFactor"
        }
        "bot" => {
            "developerName,stepIdentifier,invocationActionName,parameterName,nlpProviderType,dialog,chatButtonName"
        }
        "entitlementProcess" => "milestoneName",
        _ => "",
    };
    if extra.is_empty() {
        "fullName,name".to_string()
    } else {
        format!("fullName,name,{extra}")
    }
}

/// One parent metadata file: where it is, its suffix, and the shard directory /
/// extension reassemble needs to rebuild it.
#[derive(serde::Serialize, serde::Deserialize)]
struct Component {
    file: String,
    suffix: String,
    shard_dir: String,
    extension: String,
}

fn find_components(root: &Path) -> Vec<Component> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("force-app")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // "Mega.permissionset-meta.xml" -> stem "Mega", suffix "permissionset".
            let Some(base) = name.strip_suffix("-meta.xml") else {
                continue;
            };
            let Some((stem, suffix)) = base.rsplit_once('.') else {
                continue;
            };
            out.push(Component {
                file: path.to_string_lossy().to_string(),
                suffix: suffix.to_string(),
                shard_dir: dir.join(stem).to_string_lossy().to_string(),
                extension: format!("{suffix}-meta.xml"),
            });
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

async fn decompose(root: PathBuf, format: String) {
    let components = find_components(&root);
    assert!(
        !components.is_empty(),
        "no *-meta.xml files under {}",
        root.join("force-app").display()
    );
    let base = root.to_string_lossy().to_string();
    let limit = Arc::new(Semaphore::new(TYPES_IN_FLIGHT));
    let mut tasks = JoinSet::new();
    for c in &components {
        let (file, suffix, base, format, limit) = (
            c.file.clone(),
            c.suffix.clone(),
            base.clone(),
            format.clone(),
            Arc::clone(&limit),
        );
        tasks.spawn(async move {
            let _permit = limit.acquire_owned().await.unwrap();
            let start = Instant::now();
            DisassembleXmlFileHandler::new()
                .disassemble(
                    &file,
                    Some(&unique_id_elements(&suffix)),
                    Some("unique-id"),
                    true,
                    true,
                    IGNORE_FILE,
                    &format,
                    None,
                    None,
                    None,
                    Some(&base),
                )
                .await
                .unwrap_or_else(|e| panic!("disassemble {file}: {e}"));
            (file, start.elapsed())
        });
    }
    while let Some(done) = tasks.join_next().await {
        let (file, took) = done.unwrap();
        eprintln!("decompose {:>8.1} ms  {file}", took.as_secs_f64() * 1e3);
    }
    std::fs::write(
        root.join(MANIFEST),
        serde_json::to_string_pretty(&components).unwrap(),
    )
    .unwrap();
}

async fn recompose(root: PathBuf) {
    let manifest = std::fs::read_to_string(root.join(MANIFEST))
        .unwrap_or_else(|_| panic!("{MANIFEST} missing: run `decompose` on this root first"));
    let components: Vec<Component> = serde_json::from_str(&manifest).unwrap();
    let limit = Arc::new(Semaphore::new(TYPES_IN_FLIGHT));
    let mut tasks = JoinSet::new();
    for c in components {
        let limit = Arc::clone(&limit);
        tasks.spawn(async move {
            let _permit = limit.acquire_owned().await.unwrap();
            let start = Instant::now();
            ReassembleXmlFileHandler::new()
                .reassemble(&c.shard_dir, Some(&c.extension), true, None)
                .await
                .unwrap_or_else(|e| panic!("reassemble {}: {e}", c.shard_dir));
            assert!(
                Path::new(&c.file).is_file(),
                "reassemble did not recreate {}",
                c.file
            );
            (c.file, start.elapsed())
        });
    }
    while let Some(done) = tasks.join_next().await {
        let (file, took) = done.unwrap();
        eprintln!("recompose {:>8.1} ms  {file}", took.as_secs_f64() * 1e3);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: replay_workload <decompose|recompose> <fixture-root> [--format xml|json|json5|yaml]";
    let (Some(phase), Some(root)) = (args.first(), args.get(1)) else {
        panic!("{usage}");
    };
    let format = args
        .iter()
        .position(|a| a == "--format")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "xml".to_string());
    let root = PathBuf::from(root);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let start = Instant::now();
    match phase.as_str() {
        "decompose" => runtime.block_on(decompose(root, format)),
        "recompose" => runtime.block_on(recompose(root)),
        _ => panic!("{usage}"),
    }
    eprintln!(
        "{phase} total {:.1} ms",
        start.elapsed().as_secs_f64() * 1e3
    );
}
