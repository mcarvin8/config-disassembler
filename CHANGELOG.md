# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.1](https://github.com/mcarvin8/config-disassembler/compare/v0.9.0...v0.9.1) - 2026-07-08

### ⚡ Performance


- Disassemble files within a directory concurrently ([#97](https://github.com/mcarvin8/config-disassembler/pull/97)) - ([d25c198](https://github.com/mcarvin8/config-disassembler/commit/d25c1987e4f8bc27f0fd17092f291c5b9be534ff))


## [0.9.0](https://github.com/mcarvin8/config-disassembler/compare/v0.8.3...v0.9.0) - 2026-07-07

### 🐛 Bug Fixes


- *(xml)* Verify_roundtrip mislocates reconstructed file for dotted filenames ([#95](https://github.com/mcarvin8/config-disassembler/pull/95)) - ([d45db35](https://github.com/mcarvin8/config-disassembler/commit/d45db35e4c6be721d307a7540be243b803d18730))

### ⚙️ Miscellaneous Tasks


- *(release-plz)* Include timestamp in release version entries - ([4415de6](https://github.com/mcarvin8/config-disassembler/commit/4415de67596631c92cd0249748893e8edfa30b3e))


## [0.8.3](https://github.com/mcarvin8/config-disassembler/compare/v0.8.2...v0.8.3)

### ⛰️ Features


- *(xml)* Add round-trip verify API ([#94](https://github.com/mcarvin8/config-disassembler/pull/94)) - ([84f7d12](https://github.com/mcarvin8/config-disassembler/commit/84f7d1256f5f8da37cc91af6f5ad138d2eeca8bc))

### 📚 Documentation


- Fix README CLI inaccuracies - ([5b348b0](https://github.com/mcarvin8/config-disassembler/commit/5b348b0678fc5a8b4c08689b9693d16cc88dedfe))

### ⚙️ Miscellaneous Tasks


- *(release-plz)* Add release-plz configuration - ([805dcde](https://github.com/mcarvin8/config-disassembler/commit/805dcdeb78c341d6d008f03d7f4a4573c26161a7))


## [0.8.2](https://github.com/mcarvin8/config-disassembler/compare/v0.8.1...v0.8.2) - 2026-07-07

### Other

- update Cargo.lock dependencies

## [0.8.1](https://github.com/mcarvin8/config-disassembler/compare/v0.8.0...v0.8.1) - 2026-07-01

### Other

- *(deps)* bump ignore from 0.4.25 to 0.4.27 ([#87](https://github.com/mcarvin8/config-disassembler/pull/87))
- *(deps)* bump env_logger from 0.11.10 to 0.11.11 ([#85](https://github.com/mcarvin8/config-disassembler/pull/85))
- *(deps)* bump quick-xml from 0.40.1 to 0.41.0 ([#86](https://github.com/mcarvin8/config-disassembler/pull/86))
- *(deps)* bump configparser from 3.1.0 to 3.2.0 ([#81](https://github.com/mcarvin8/config-disassembler/pull/81))
- *(deps)* bump regex from 1.12.3 to 1.12.4 ([#84](https://github.com/mcarvin8/config-disassembler/pull/84))
- *(deps)* bump log from 0.4.30 to 0.4.33 ([#82](https://github.com/mcarvin8/config-disassembler/pull/82))
- *(deps)* bump codecov/codecov-action from 6 to 7 ([#80](https://github.com/mcarvin8/config-disassembler/pull/80))
- *(deps)* bump actions/checkout from 6 to 7 ([#79](https://github.com/mcarvin8/config-disassembler/pull/79))
- *(ci)* set commit message prefixes for Dependabot

## [0.8.0](https://github.com/mcarvin8/config-disassembler/compare/v0.7.0...v0.8.0) - 2026-06-30

### Added

- preserve original format of sidecar content through round-trip ([#76](https://github.com/mcarvin8/config-disassembler/pull/76))

### Fixed

- update anyhow to 1.0.103 to address RUSTSEC-2026-0190 ([#78](https://github.com/mcarvin8/config-disassembler/pull/78))

## [0.7.0](https://github.com/mcarvin8/config-disassembler/compare/v0.6.0...v0.7.0) - 2026-06-29

### Added

- add sidecar-elements for extracting embedded non-XML blobs ([#73](https://github.com/mcarvin8/config-disassembler/pull/73))

## [0.6.0](https://github.com/mcarvin8/config-disassembler/compare/v0.5.4...v0.6.0) - 2026-06-08

### Fixed

- preserve original JSONC indentation through disassemble/reassemble ([#72](https://github.com/mcarvin8/config-disassembler/pull/72))
- four JSONC and directory-walk correctness bugs ([#69](https://github.com/mcarvin8/config-disassembler/pull/69))

### Other

- *(docs)* update Node.js bindings information in README
- avoid double disk read per split file during JSONC reassembly ([#71](https://github.com/mcarvin8/config-disassembler/pull/71))

## [0.5.4](https://github.com/mcarvin8/config-disassembler/compare/v0.5.3...v0.5.4) - 2026-06-01

### Other

- *(deps)* bump quick-xml from 0.39.2 to 0.40.1 ([#67](https://github.com/mcarvin8/config-disassembler/pull/67))
- *(deps)* bump tokio from 1.52.1 to 1.52.3 ([#65](https://github.com/mcarvin8/config-disassembler/pull/65))
- *(deps)* bump jsonc-parser from 0.32.3 to 0.32.4 ([#66](https://github.com/mcarvin8/config-disassembler/pull/66))
- *(deps)* bump log from 0.4.29 to 0.4.30 ([#63](https://github.com/mcarvin8/config-disassembler/pull/63))
- *(deps)* bump serde_json from 1.0.149 to 1.0.150 ([#62](https://github.com/mcarvin8/config-disassembler/pull/62))
- *(deps)* bump toon-format from 0.4.5 to 0.5.0 ([#64](https://github.com/mcarvin8/config-disassembler/pull/64))
- *(deps)* bump schneegans/dynamic-badges-action from 1.7.0 to 1.8.0 ([#61](https://github.com/mcarvin8/config-disassembler/pull/61))
- *(deps)* bump actions/upload-artifact from 4 to 7 ([#60](https://github.com/mcarvin8/config-disassembler/pull/60))
- *(coverage)* round 2 — target remaining 77 missed lines ([#59](https://github.com/mcarvin8/config-disassembler/pull/59))
- *(coverage)* add 15 targeted tests to close remaining coverage gaps ([#58](https://github.com/mcarvin8/config-disassembler/pull/58))
- *(format)* cover remaining 1% — empty-key null and dead-code path ([#57](https://github.com/mcarvin8/config-disassembler/pull/57))
- streamline docs for new user onboarding

## [0.5.3](https://github.com/mcarvin8/config-disassembler/compare/v0.5.2...v0.5.3) - 2026-05-20

### Other

- create formats.md
- create xml.md
- simplify readme
- *(mutation)* publish mutation score to a shields.io endpoint badge ([#54](https://github.com/mcarvin8/config-disassembler/pull/54))

## [0.5.2](https://github.com/mcarvin8/config-disassembler/compare/v0.5.1...v0.5.2) - 2026-05-14

### Fixed

- *(mutants)* anchor exclude_re on mutant-name format (`replace <fn>`) ([#37](https://github.com/mcarvin8/config-disassembler/pull/37))

### Other

- *(mutants)* exclude three loop-bound infinite-loop hazards ([#53](https://github.com/mcarvin8/config-disassembler/pull/53))
- *(mutants)* extract pure helpers from reassemble/cli ([#52](https://github.com/mcarvin8/config-disassembler/pull/52))
- *(mutants)* extract pure helpers from disassemble/multi_level ([#51](https://github.com/mcarvin8/config-disassembler/pull/51))
- *(builders)* cover decl/fields/leaf-count guards + exclude equivalent strategy branch ([#50](https://github.com/mcarvin8/config-disassembler/pull/50))
- *(build-xml-string)* cover Null/comment/tail/sibling indent + drop dead has_children ([#49](https://github.com/mcarvin8/config-disassembler/pull/49))
- disable Swatinem/rust-cache cache-bin to fix macOS flake ([#48](https://github.com/mcarvin8/config-disassembler/pull/48))
- *(strip-whitespace)* cover #cdata and #text-tail whitespace stripping ([#47](https://github.com/mcarvin8/config-disassembler/pull/47))
- replace dtolnay/rust-toolchain with actions-rust-lang/setup-rust-toolchain ([#46](https://github.com/mcarvin8/config-disassembler/pull/46))
- *(mutants)* exclude five cosmetic / equivalent mutants surfaced by full sweep ([#45](https://github.com/mcarvin8/config-disassembler/pull/45))
- *(mutants)* skip all parse_reassemble_args mutants (broaden exclude) ([#44](https://github.com/mcarvin8/config-disassembler/pull/44))
- *(mutants)* exclude destructive (None, *, true) parse_reassemble_args mutants ([#43](https://github.com/mcarvin8/config-disassembler/pull/43))
- *(mutants)* install cargo-mutants from upstream main (post-#613) ([#42](https://github.com/mcarvin8/config-disassembler/pull/42))
- *(mutants)* add --in-place to dodge cargo-mutants v27.0.0 #611 tmp-tree bug ([#41](https://github.com/mcarvin8/config-disassembler/pull/41))
- *(xml/cli)* use iterator-based loop in parse_disassemble_args ([#40](https://github.com/mcarvin8/config-disassembler/pull/40))
- *(cli)* kill run_reassemble guard mutant + exclude one equivalent ([#39](https://github.com/mcarvin8/config-disassembler/pull/39))
- *(jsonc)* kill 5 surviving JSONC arithmetic / guard mutants ([#38](https://github.com/mcarvin8/config-disassembler/pull/38))
- update Node.js binding reference in README
- *(mutants)* fall back to --file when --in-diff finds no mutants ([#36](https://github.com/mcarvin8/config-disassembler/pull/36))
- *(mutants)* skip help-text mutants and cover xml_cmd shim ([#35](https://github.com/mcarvin8/config-disassembler/pull/35))
- *(jsonc)* kill mutation-survivors in jsonc + helper paths ([#34](https://github.com/mcarvin8/config-disassembler/pull/34))
- *(xml/cli)* cover separated-form arg parsing edge cases ([#33](https://github.com/mcarvin8/config-disassembler/pull/33))
- add cargo-mutants mutation testing ([#31](https://github.com/mcarvin8/config-disassembler/pull/31))

## [0.5.1](https://github.com/mcarvin8/config-disassembler/compare/v0.5.0...v0.5.1) - 2026-05-08

### Fixed

- remove dead code for disassembled directories ([#29](https://github.com/mcarvin8/config-disassembler/pull/29))

## [0.5.0](https://github.com/mcarvin8/config-disassembler/compare/v0.4.5...v0.5.0) - 2026-05-05

### Fixed

- *(xml)* flush + shutdown disassembled file handle before returning ([#28](https://github.com/mcarvin8/config-disassembler/pull/28))
- *(xml)* sanitize unique-id values + detect sibling collisions ([#26](https://github.com/mcarvin8/config-disassembler/pull/26))

## [0.4.5](https://github.com/mcarvin8/config-disassembler/compare/v0.4.4...v0.4.5) - 2026-05-04

### Added

- *(xml)* support compound keys in unique-id-elements (`+` syntax) ([#22](https://github.com/mcarvin8/config-disassembler/pull/22))

## [0.4.4](https://github.com/mcarvin8/config-disassembler/compare/v0.4.3...v0.4.4) - 2026-05-04

### Fixed

- *(xml)* preserve dotted fullNames in disassembled output directory ([#21](https://github.com/mcarvin8/config-disassembler/pull/21))

### Other

- set dependabot to monthly

## [0.4.3](https://github.com/mcarvin8/config-disassembler/compare/v0.4.2...v0.4.3) - 2026-05-01

### Fixed

- *(xml)* hash outer element on unique-id fallback ([#18](https://github.com/mcarvin8/config-disassembler/pull/18))

## [0.4.2](https://github.com/mcarvin8/config-disassembler/compare/v0.4.1...v0.4.2) - 2026-04-30

### Fixed

- reassemble nested multi-level segments without stripping wrapper elements ([#16](https://github.com/mcarvin8/config-disassembler/pull/16))

### Other

- fix link formatting in readme

## [0.4.1](https://github.com/mcarvin8/config-disassembler/compare/v0.4.0...v0.4.1) - 2026-04-30

### Fixed

- allow multiple multi-level rules ([#15](https://github.com/mcarvin8/config-disassembler/pull/15))

### Other

- add Node.js support details

## [0.4.0](https://github.com/mcarvin8/config-disassembler/compare/v0.3.0...v0.4.0) - 2026-04-29

### Added

- add INI format ([#11](https://github.com/mcarvin8/config-disassembler/pull/11))

### Other

- *(ini)* enhance INI serialization with pretty-print options ([#13](https://github.com/mcarvin8/config-disassembler/pull/13))

## [0.3.0](https://github.com/mcarvin8/config-disassembler/compare/v0.2.0...v0.3.0) - 2026-04-28

### Added

- support JSONC ([#10](https://github.com/mcarvin8/config-disassembler/pull/10))
- add TOON format ([#9](https://github.com/mcarvin8/config-disassembler/pull/9))

### Other

- add husky notes
- *(docs)* update contributing
- test fix
- format capabilities ([#8](https://github.com/mcarvin8/config-disassembler/pull/8))
- update flaky test assert

## [0.2.0](https://github.com/mcarvin8/config-disassembler/compare/v0.1.1...v0.2.0) - 2026-04-28

### Added

- port xml-disassembler in-tree and add ignore-file/directory support across formats ([#6](https://github.com/mcarvin8/config-disassembler/pull/6))
- add INI support ([#4](https://github.com/mcarvin8/config-disassembler/pull/4))

### Other

- update readme

## [0.1.1](https://github.com/mcarvin8/config-disassembler/compare/v0.1.0...v0.1.1) - 2026-04-27

### Other

- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#3](https://github.com/mcarvin8/config-disassembler/pull/3))
- Merge pull request #2 from mcarvin8/dependabot/github_actions/actions/checkout-6
- make hook executable
