# Development, CI, and containers

## Build and test

Rust and a C compiler are required: tree-sitter builds the C parser. The parser
comes from [kiritanya/tree-sitter-rpmspec](https://github.com/kiritanya/tree-sitter-rpmspec),
pinned in `Cargo.toml` and `Cargo.lock`.

```sh
cargo build --locked --release --bins
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-targets --all-features
```

Repository-local aliases in `.cargo/config.toml` make these checks easier to run:

```sh
cargo fmt-check # cargo fmt --all -- --check
cargo lint      # cargo clippy --locked --workspace --all-targets --all-features
cargo test-all  # cargo test --locked --workspace --all-targets --all-features
```

Clippy's `pedantic`, `nursery`, and `cargo` groups are enabled in `Cargo.toml`,
with documented exceptions. The standard command reports warnings without
promoting them to errors; review any warnings rather than relying only on the
exit status. CI uses the same Clippy command.

The implementation separates policy from transformations:

- `src/rpm.rs`: shared tags, group order, section names, and macro vocabulary.
- `src/path_macros.rs`: default path replacement table.
- `src/format.rs`: structure-aware formatting.
- `src/style.rs`: conservative text helpers shared with the linter.
- `src/lint.rs`: compatibility APIs that call the shared rule engine.
- `src/context.rs`, `src/rules/`, `src/engine.rs`: shared snapshots, rule traits,
  static enum dispatch, and atomic fix application; see [architecture](architecture.md).
- `src/cli.rs`: the unified CLI, output backend selection, tracing, and stable-eyre errors.
- `src/output/`: `DiagnosticOutput` trait and rich, short, and SARIF backends.

Rule detection lives in one file per rule under `src/rules/format/`,
`src/rules/fixable/`, and `src/rules/check_only/`. `Context` stores parsing facts,
not a cached result of another linter. `tests/legacy_lint.rs` preserves the
original semantic regressions, while `tests/rules.rs` calls rules directly.
`tests/output.rs` exercises output backends and error handling directly;
`tests/unified_cli.rs` covers SARIF documents, URI escaping, Unicode/CRLF ranges,
multiple files, and diagnostics after fixes.

The legacy library `format()` API retains explicitly configured safe fixes for
compatibility. New callers can use `Engine` and `FixMode` to select rule kinds.

## Coverage

Install the tool and Rust's matching LLVM tools once:

```sh
cargo install cargo-llvm-cov --version 0.9.1 --locked
rustup component add llvm-tools-preview
```

Run the same coverage commands as CI:

```sh
cargo llvm-cov clean --workspace
cargo llvm-cov --locked --workspace --all-targets --all-features --no-report
mkdir -p target/coverage
cargo llvm-cov report --lcov --output-path target/coverage/lcov.info
cargo llvm-cov report --html --output-dir target/coverage
cargo llvm-cov report --summary-only
```

Open `target/coverage/html/index.html` for the report. Coverage includes CLI
integration tests for formatting, unified lint/fix, Unicode diagnostics,
configuration providers, and the compatibility executable.

## Spec corpus

```sh
cargo build --locked --bins
scripts/corpus.sh ../terra
```

The script formats every spec twice, checking idempotence and preservation of
content while accounting for intentional macro bracing/definition conversion.
It never executes `rpmspec`, `rpmbuild`, or shell macros. The corpus is a local
check; CI does not require a checkout of Terra.

## Container

The Dockerfile follows [FyraLabs/subatomic](https://github.com/FyraLabs/subatomic):
build in `ghcr.io/terrapkg/builder:f44`, derive runtime RPM dependencies from
the binaries, and copy them into Fedora Minimal 44. Both tools are on `PATH`;
`rpmspecfmt` is the default command; `rpmspeclint` is a compatibility alias.
Rust stable is sufficient.

```sh
docker build -t rpmspecfmt .
docker run --rm -i rpmspecfmt < foo.spec
docker run --rm -v "$PWD:/work" -w /work rpmspecfmt rpmspecfmt --lint foo.spec
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$PWD:/work" -w /work rpmspecfmt rpmspecfmt --lint --fix foo.spec
```

On SELinux hosts, use `"$PWD:/work:Z"` for a private bind mount. Formatting or
fixing mounted files requires write access; `--fix` writes in place.

## GitHub Actions

- `.github/workflows/rust.yml` checks rustfmt and Clippy and runs tests with
  `cargo llvm-cov`. LCOV and HTML reports are uploaded as the `coverage` artifact.
  Clippy and coverage run in Terra's Fedora 44 builder without privileged mode
  or external services.
- `.github/workflows/docker.yml` builds native amd64 and arm64 images,
  publishes to GHCR on pushes, and signs the combined manifest.
  Pull requests/manual runs build without publishing.

Actions are pinned to commits. Publishing uses `GITHUB_TOKEN` and requires
package-write permission in the repository; no extra registry secret is used.
