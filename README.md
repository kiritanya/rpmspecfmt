# rpmspecfmt

A formatter for RPM `.spec` files. It uses tree-sitter, so it understands the
spec structure, but it never expands macros.

## Build

```sh
cargo build --release
```

The parser comes from the fork at `../tree-sitter-rpmspec`:

```toml
tree-sitter-rpmspec = { path = "../tree-sitter-rpmspec" }
```

You can switch this to a git dependency after the fork is pushed.

## Usage

```sh
rpmspecfmt foo.spec              # print to stdout
rpmspecfmt -w foo.spec           # rewrite in place
rpmspecfmt --check foo.spec      # exit 1 if the file would change
rpmspecfmt --diff foo.spec       # show a unified diff
rpmspecfmt < foo.spec            # stdin to stdout
```

| Option | Effect |
| --- | --- |
| `--safe` | change whitespace only; no sort, wrap or reindent |
| `--no-sort` | keep the order of tags with the same name |
| `--no-wrap` | do not wrap long dependency tags |
| `--no-reindent` | do not expand leading tabs in scriptlet bodies |
| `--tag-column N` | column where tag values start (default 16) |
| `--wrap-width N` | width for wrapped dependency tags (default 100) |

## What it changes

- Tag values are aligned at column 16.
- Consecutive tags with the same name are sorted.
  Dependency tags (`BuildRequires`, `Requires`, ...) are sorted by dependency
  token and wrapped. Other tags are sorted by value.
- Long dependency lines are wrapped at the configured width.
  Values with parentheses, comparison operators, quotes or `%(` are not
  wrapped, because splitting them can change their meaning.
- One blank line is added before each section header, and no blank line is
  left after it.
- Runs of blank lines are collapsed outside description and scriptlet bodies.
- Trailing spaces are removed, and the file ends with one newline.
- Leading tabs in scriptlet bodies become four spaces.

## What it leaves alone

- Lines that contain a tree-sitter `ERROR` or `MISSING` node.
- A whole section whose node reports an error.
- Heredoc bodies.
- Everything except whitespace when `--safe` is used.

## Tests

```sh
cargo test
scripts/corpus.sh ../terra
```

The corpus script formats every spec twice. It checks that the output is
idempotent and does not lose content. It never runs `rpmspec` or `rpmbuild`,
so shell macros are not executed.

## License

AGPL-3.0-or-later. See `LICENSE`.

The `tree-sitter-rpmspec` fork is MIT licensed.
