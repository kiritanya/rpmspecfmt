# rpmspecfmt

A formatter for RPM `.spec` files, plus a small linter. It uses tree-sitter, so
it understands the spec structure, but it never expands macros.

## Build

```sh
cargo build --release
```

The parser comes from
[kiritanya/tree-sitter-rpmspec](https://github.com/kiritanya/tree-sitter-rpmspec),
pinned to a revision:

```toml
tree-sitter-rpmspec = { git = "https://github.com/kiritanya/tree-sitter-rpmspec", rev = "0166fc90271150ec93bb6168d58672a60f5a6a50" }
```

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
| `--config PATH` | read this config file instead of searching for one |
| `--tag-column N` | column where tag values start (default 16) |
| `--max-width N` | width for wrapping (default 100; alias `--wrap-width`) |

## Configuration

The formatter reads the first `.rpmspecfmt.toml` it finds in the current
directory or one of its parents. Use `--config PATH` to pick a file directly.
Command line options override the file, and `--safe` turns off every change
that is not whitespace.

```toml
tag_column = 16
max_width = 80
sort = true
wrap = true
reindent = true
wrap_description = true
convert_paths = true

[path_macros]
"/opt/example" = "%{_prefix}/opt/example"
```

`wrap_description` re-flows long lines in `%description` at `max_width`.
It keeps blank lines, indentation and `*` / `-` / `+` bullets.

`convert_paths` replaces leading paths in `%files` with macros. The default
table covers the Paths section of the Terra RPM macro docs, for example:

```spec
/usr/share/foo             -> %{_datadir}/foo
/usr/share/doc/foo         -> %{_docdir}/foo
/usr/lib/systemd/system/x  -> %{_unitdir}/x
/var/lib/foo               -> %{_sharedstatedir}/foo
```

Some entries use macros from `anda-srpm-macros` (Terra only). Entries in
`[path_macros]` are added to the default table, and the longest prefix wins.
A path like `/usr/share/doc/%{name}` still matches the literal
`/usr/share/doc` prefix, so it becomes `%{_docdir}/%{name}`.
Comment lines and quoted paths are not changed.

## Linter

`rpmspeclint` is a separate binary for semantic checks. It never rewrites files.
It reads tags line by line, so it still works when the grammar reports errors.

```sh
rpmspeclint foo.spec
rpmspeclint < foo.spec
```

It prints one line per finding:

```
foo.spec:6:23: warning: use %{evr} instead of %{version}-%{release} [use-evr]
```

| Rule | What it looks for |
| --- | --- |
| `missing-changelog` | no `%changelog` or `%autochangelog` |
| `missing-license` | no `License:` tag |
| `use-evr` | `%{version}-%{release}` or `%version-%release` in dependency tags |
| `use-autosetup` | `%setup`; `%autosetup` is preferred |

`use-evr` needs the `%{evr}` macro. On Terra it comes from
`anda-srpm-macros`.

Exit status: 0 means no findings, 1 means findings, 2 means a read error.

## What it changes

- Tag values are aligned at `tag_column` (16 by default).
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
- If enabled in the config, long `%description` lines are wrapped and leading
  paths in `%files` become macros.

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
