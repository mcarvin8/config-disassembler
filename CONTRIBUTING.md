# Contribution guidelines

First off, thank you for considering contributing to config-disassembler.

If your contribution is not straightforward, please first discuss the change you
wish to make by creating a new issue before making the change.

## Reporting issues

Before reporting an issue on the
[issue tracker](https://github.com/mcarvin8/config-disassembler/issues),
please check that it has not already been reported by searching for some related
keywords.

## Pull requests

Try to do one pull request per change.

### Releasing

Releases and changelog generation are automated using release-plz.

To ensure your changes are properly categorized in the changelog, please follow
[conventional commit messages](https://www.conventionalcommits.org/en/v1.0.0/).

### CI/CD and code coverage

All pull requests run automated checks:

- The test suite runs with all features on Ubuntu, Windows, and macOS.
- Ubuntu also generates coverage using cargo-llvm-cov and uploads the report to
  Codecov.
- Rustfmt checks formatting with `cargo fmt --all --check`.
- Clippy runs with `cargo clippy --all-targets --all-features --workspace -- -D warnings`.
- Documentation builds with warnings denied using `cargo doc`.
- A RustSec audit runs on pull requests, dependency changes, and a daily schedule.

Releases are automated after changes land on `main`:

- release-plz publishes crate releases and opens release PRs for version and
  changelog updates.
- Published GitHub releases trigger binary artifact uploads for macOS, Linux,
  and Windows targets.

You can optionally run this command locally to run tests and generate coverage:

```bash
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info --ignore-filename-regex 'main\.rs'
```

## Developing

### Set up

This is no different than other Rust projects.

```shell
git clone https://github.com/mcarvin8/config-disassembler
cd config-disassembler
cargo test --all-features --workspace
```

## Testing

Run all tests:

```bash
cargo test --all-features --workspace
```

- **Unit tests** - In-module tests for format parsing, disassembly,
  reassembly, XML builders, parsers, and transformers.
- **Integration tests** - Tests under `tests/` cover CLI behavior, fixture
  round trips, cross-format conversions, TOML restrictions, and XML
  disassemble/reassemble workflows.

## Git hooks

This repository uses cargo-husky to install a pre-commit hook from
`.cargo-husky/hooks/pre-commit`.

Before a commit is created, the hook:

- Runs `cargo fmt --all`.
- Re-stages any already-staged Rust files that were rewritten by rustfmt.
- Runs `cargo clippy --all-targets --all-features --workspace -- -D warnings`.

If formatting or Clippy fails, the commit is blocked. These checks intentionally
match the CI formatting and lint expectations.

### Useful Commands

- Build and run release version:

  ```shell
  cargo build --release && cargo run --release
  ```

- Run Clippy:

  ```shell
  cargo clippy --all-targets --all-features --workspace
  ```

- Run all tests:

  ```shell
  cargo test --all-features --workspace
  ```

- Run all tests with code coverage (install cargo-llvm-cov first):

  ```shell
  cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info --ignore-filename-regex 'main\.rs'
  ```

- Check to see if there are code formatting issues

  ```shell
  cargo fmt --all -- --check
  ```

- Format the code in the project

  ```shell
  cargo fmt --all
  ```
