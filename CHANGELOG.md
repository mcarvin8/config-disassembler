# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
