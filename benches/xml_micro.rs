//! CPU-only XML benchmarks: no filesystem, no tokio. These are the stable
//! signal (CodSpeed runs them under CPU simulation), and every group runs at
//! two sizes, `N` and `2N`, so super-linear growth -- e.g. a sibling-collision
//! check that compares every sibling with every other -- shows up as the 2N
//! case costing far more than twice the N case.

use std::hint::black_box;

use config_disassembler::xml::build_xml_string;
use config_disassembler::xml::parsers::{
    parse_unique_id_element, parse_xml_from_str, short_hash_for_element,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

mod support;

const SIZES: [usize; 2] = [1_000, 2_000];
const FILE_NAME: &str = "Bench.permissionset-meta.xml";

fn parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_xml_from_str");
    for n in SIZES {
        let xml = support::permission_set_xml(n);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &xml, |b, xml| {
            b.iter(|| parse_xml_from_str(black_box(xml), FILE_NAME))
        });
    }
    group.finish();
}

fn unique_ids(c: &mut Criterion) {
    let mut group = c.benchmark_group("unique_id");
    for n in SIZES {
        let doc =
            parse_xml_from_str(&support::permission_set_xml(n), FILE_NAME).expect("fixture parses");
        let siblings = support::largest_sibling_array(&doc).to_vec();
        assert_eq!(
            siblings.len(),
            n,
            "expected the n <fieldPermissions> siblings"
        );
        group.throughput(Throughput::Elements(siblings.len() as u64));

        // Configured-field lookup: the path every sibling takes when one of
        // the unique-id fields is present (`<field>` here).
        group.bench_with_input(
            BenchmarkId::new("configured_field", n),
            &siblings,
            |b, siblings| {
                b.iter(|| {
                    for el in siblings {
                        black_box(parse_unique_id_element(
                            el,
                            Some(support::PERMISSION_SET_UNIQUE_IDS),
                        ));
                    }
                })
            },
        );

        // SHA-256 fallback: what siblings with no configured field (e.g.
        // CustomApplication actionOverrides) pay per element.
        group.bench_with_input(
            BenchmarkId::new("hash_fallback", n),
            &siblings,
            |b, siblings| {
                b.iter(|| {
                    for el in siblings {
                        black_box(short_hash_for_element(el));
                    }
                })
            },
        );
    }
    group.finish();
}

fn build(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_xml_string");
    for n in SIZES {
        let xml = support::permission_set_xml(n);
        let doc = parse_xml_from_str(&xml, FILE_NAME).expect("fixture parses");
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &doc, |b, doc| {
            b.iter(|| build_xml_string(black_box(doc)))
        });
    }
    group.finish();
}

criterion_group!(benches, parse, unique_ids, build);
criterion_main!(benches);
