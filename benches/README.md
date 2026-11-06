# Benchmarks

Two [criterion](https://github.com/bheisler/criterion.rs) bench targets. Both
use `codspeed-criterion-compat`, which behaves exactly like upstream criterion
under `cargo bench` and reports to [CodSpeed](https://codspeed.io) under
`cargo codspeed run`.

| Target      | What it measures | Runs in CI |
| ----------- | ---------------- | ---------- |
| `xml_micro` | Pure CPU, no I/O: `parse_xml_from_str`, `parse_unique_id_element` (configured field), `short_hash_for_element` (SHA-256 fallback), `build_xml_string` | Yes, on CodSpeed (CPU simulation) on every PR and push to `main` |
| `xml_macro` | End to end through `DisassembleXmlFileHandler` / `ReassembleXmlFileHandler`: real filesystem I/O and the multi-thread tokio runtime. Shapes: `single_file` (one large permission set) and `many_files` (a directory of small ones, which exercises the concurrent fan-out in `handle_directory`) | No. Simulation doesn't count syscalls and runs threads one at a time, so it can't measure these. |

Fixtures come from `benches/support/mod.rs`. They are deterministic,
counter-based permission sets shaped like sf-decomposer's
`scripts/gen-perf-fixtures.ts`, so no fixture files are checked in.

## Running locally

```bash
cargo bench --bench xml_micro                 # all micro benches
cargo bench --bench xml_micro -- unique_id    # filter by name
cargo bench --bench xml_macro                 # end to end (slow: ~1-2 s per iteration)

# Compare a change against main:
git checkout main && cargo bench --bench xml_micro -- --save-baseline main
git checkout -    && cargo bench --bench xml_micro -- --baseline main
```

Wall-clock numbers on a laptop or a shared runner can swing by tens of
percent between runs. Trust CodSpeed's simulated numbers for small changes.

## Spotting work that grows faster than the input

Every `xml_micro` group runs at two sizes, `1000` and `2000` siblings. The
`2000` case should cost about 2× the `1000` case. A clearly larger ratio means
a code path now does more work per element as the input grows, for example a
sibling-collision check that compares every sibling with every other. The
same comparison shows up between the two size rows in CodSpeed's report.

## Adding a benchmark

- CPU-only work goes in `xml_micro.rs`. Run it at both sizes in `SIZES`.
- Anything that touches the filesystem or the tokio runtime goes in
  `xml_macro.rs`. Do untimed setup (fixture copies) in `iter_batched`'s
  setup closure. That closure runs inside the runtime, so it must not call
  `block_on`; prepare async preconditions once, before the bench function.
- Leave logging off (no `env_logger::init()`), so formatting log lines
  isn't timed.
