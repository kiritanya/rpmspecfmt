# rpmspecfmt

A formatter for RPM `.spec` files, plus a small linter. It uses tree-sitter, so
it understands the spec structure, but it never expands macros.

## Build

```sh
cargo build --locked --release --bins
```

Requires Rust and a C compiler. `rpmspeclint` is also built as a compatibility alias.

## Usage

```sh
rpmspecfmt foo.spec              # print to stdout
rpmspecfmt -w foo.spec           # rewrite in place
rpmspecfmt --check foo.spec      # exit 1 if the file would change
rpmspecfmt --diff foo.spec       # show a unified diff
rpmspecfmt < foo.spec            # stdin to stdout
rpmspecfmt --lint foo.spec       # report formatting and lint findings
rpmspecfmt --lint --fix foo.spec # apply safe fixes, then report what remains
```

## Development checks

```sh
cargo fmt-check
cargo lint
cargo test-all
```

These repository-local Cargo aliases run the standard formatting, Clippy, and
test commands. Clippy enables `pedantic`, `nursery`, and `cargo` checks through
`Cargo.toml`; see [development](docs/development.md) for the full commands.

## Documentation

- [Formatting behavior and safety](docs/formatting.md)
- [CLI options and configuration](docs/configuration.md)
- [Linter, safe fixes, and rule IDs](docs/lint.md)
- [Development, coverage, CI, and Docker](docs/development.md)
- [Rule engine and extension design](docs/architecture.md)

## License

AGPL-3.0-or-later. See `LICENSE`.

The `tree-sitter-rpmspec` fork is MIT licensed.
