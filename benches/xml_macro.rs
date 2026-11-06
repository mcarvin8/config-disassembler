//! End-to-end XML benchmarks through the public handlers: real filesystem
//! I/O and the multi-thread tokio runtime, i.e. what the napi binding (and so
//! sf-decomposer) actually calls. Two shapes:
//!
//! * `single_file`: one large permission set. Dominated by parse / unique-id /
//!   write of many sibling shards.
//! * `many_files`: a directory of small permission sets. The only shape that
//!   exercises the concurrent fan-out in `handle_directory`.
//!
//! Every iteration runs against a fresh copy of the fixture (prepared in
//! criterion's untimed setup), because disassemble/reassemble rewrite the tree.

use std::path::Path;

use config_disassembler::xml::{DisassembleXmlFileHandler, ReassembleXmlFileHandler};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use tempfile::TempDir;
use tokio::runtime::Runtime;

mod support;

const SINGLE_FILE_N: usize = 2_000;
const MANY_FILES: usize = 100;
const MANY_FILES_N: usize = 100;
const FILE_NAME: &str = "Bench.permissionset-meta.xml";

fn path_str(p: &Path) -> &str {
    p.to_str().expect("utf-8 temp path")
}

/// Same flags sf-decomposer uses for its `unique-id` decompose.
async fn disassemble(target: &Path, base: &Path) {
    DisassembleXmlFileHandler::new()
        .disassemble(
            path_str(target),
            Some(support::PERMISSION_SET_UNIQUE_IDS),
            Some("unique-id"),
            true,
            true,
            support::NO_IGNORE_FILE,
            "xml",
            None,
            None,
            None,
            Some(path_str(base)),
        )
        .await
        .expect("disassemble");
}

fn fresh_copy(template: &Path) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    support::copy_dir_all(template, dir.path());
    dir
}

fn bench_disassemble(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("disassemble");
    group.sample_size(10);

    let single = TempDir::new().expect("temp dir");
    std::fs::write(
        single.path().join(FILE_NAME),
        support::permission_set_xml(SINGLE_FILE_N),
    )
    .expect("write fixture");
    group.bench_function("single_file", |b| {
        b.to_async(&rt).iter_batched(
            || fresh_copy(single.path()),
            |dir| async move {
                disassemble(&dir.path().join(FILE_NAME), dir.path()).await;
                dir // dropped (deleted) outside the timed section
            },
            BatchSize::PerIteration,
        )
    });

    let many = TempDir::new().expect("temp dir");
    support::write_many_files(many.path(), MANY_FILES, MANY_FILES_N);
    group.bench_function("many_files", |b| {
        b.to_async(&rt).iter_batched(
            || fresh_copy(many.path()),
            |dir| async move {
                disassemble(dir.path(), dir.path()).await;
                dir
            },
            BatchSize::PerIteration,
        )
    });

    group.finish();
}

fn bench_reassemble(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("reassemble");
    group.sample_size(10);

    // Disassemble once up front; each iteration reassembles a fresh copy of
    // that output. (Disassembling inside the setup closure isn't possible:
    // criterion calls it from within the runtime's block_on.)
    let template = TempDir::new().expect("temp dir");
    std::fs::write(
        template.path().join(FILE_NAME),
        support::permission_set_xml(SINGLE_FILE_N),
    )
    .expect("write fixture");
    rt.block_on(disassemble(
        &template.path().join(FILE_NAME),
        template.path(),
    ));
    // Output dir name = file stem minus its last extension segment ("Bench").
    let shard_dir = "Bench";
    assert!(
        template.path().join(shard_dir).is_dir(),
        "expected disassembled output at {shard_dir}/"
    );

    group.bench_function("single_file", |b| {
        b.to_async(&rt).iter_batched(
            || fresh_copy(template.path()),
            |dir| async move {
                ReassembleXmlFileHandler::new()
                    .reassemble(
                        path_str(&dir.path().join(shard_dir)),
                        Some("permissionset-meta.xml"),
                        true,
                        None,
                    )
                    .await
                    .expect("reassemble");
                dir
            },
            BatchSize::PerIteration,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_disassemble, bench_reassemble);
criterion_main!(benches);
