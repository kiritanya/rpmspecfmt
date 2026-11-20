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
| `--tag-column N` | fixed column where tag values start; default `0` aligns each group automatically |
| `--max-width N` | width for wrapping (default 100; alias `--wrap-width`) |

## Configuration

The formatter reads the first `.rpmspecfmt.toml` it finds in the current
directory or one of its parents. Use `--config PATH` to pick a file directly.
Command line options override the file, and `--safe` turns off every change
that is not whitespace.

```toml
tag_column = 0
group_tags = true
split_buildrequires = true
normalize_macros = true
global_macros = true
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

`rpmspeclint` is a separate binary for semantic and style checks. It only
rewrites files when `--fix` is specified.
It reads tags line by line, so it still works when the grammar reports errors.

```sh
rpmspeclint foo.spec
rpmspeclint < foo.spec
rpmspeclint --fix foo.spec       # apply safe fixes in place, then lint again
rpmspeclint --fix foo.spec bar.spec
```

`--fix` uses the built-in style defaults to group/align tags, split
`BuildRequires`, brace macro references, and convert/hoist safe literal macro
definitions. It also normalizes whitespace, but does not sort dependency
values, wrap runtime dependencies or descriptions, reindent scriptlets, or
convert file paths. It does not load formatter configuration files.

Unsafe preambles remain completely unchanged. Macro calls after all explicit
preamble tags are kept in place and do not prevent formatting the tags before
them. Definitions whose scope or
expansion timing could change also remain in place. `use-evr`, `use-autosetup`,
missing license, and missing changelog findings need manual review and are not
automatically fixed. Only remaining findings are printed, with line numbers
from the fixed file. Without `--fix`, linting remains read-only.

`--fix` requires file paths; stdin is lint-only. For formatted stdin output,
use `rpmspecfmt` instead.

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
| `buildrequires-one-per-line` | multiple dependency expressions in one `BuildRequires:` line |
| `braced-macros` | bare macro references such as `%_bindir` and `%?dist` |
| `use-global` | `%define`; check expansion timing before converting to `%global` |
| `macro-placement` | macro definitions after `Name:` |
| `unsafe-preamble` | executable macros before/between preamble tags, or preamble conditionals; automatic formatting is disabled |
| `tag-group-order` | incorrect tag group order in a lint-only preamble |
| `tag-group-spacing` | no blank line between adjacent groups in a lint-only preamble |
| `tag-alignment` | values not aligned within their group in a lint-only preamble |

`use-evr` needs the `%{evr}` macro. On Terra it comes from
`anda-srpm-macros`.

Exit status: 0 means no remaining findings, 1 means findings remain, 2 means
an input, formatting, or write error (including `--fix` without file paths).

## What it changes

- Preamble tags are grouped in this order, with one blank line between groups:
  metadata (`Name`, `Version`, `Release`, `Summary`, `License`, `URL`, `Packager`,
  etc.), sources and patches, runtime dependencies (`Provides`, `Supplements`,
  `Enhances`, `Recommends`, `Obsoletes`, `Conflicts`, `Requires`, etc.),
  `BuildRequires` / `BuildConflicts`, and `BuildSystem` / `BuildOption(...)`.
  Other tags form a final group. Sources and patches are ordered numerically.
  Comments directly above tags move with them.
- Values start one space after the longest tag (including its colon) in each
  group. A nonzero `tag_column` overrides automatic alignment.
- Each `BuildRequires:` line contains one dependency expression, including
  version constraints such as `foo >= 1.2` and rich dependencies such as
  `(foo if bar)`. `--no-wrap` does not disable this; use
  `split_buildrequires = false` or `--safe` to retain multiple expressions.
- Bare macro references become braced: `%_bindir` becomes `%{_bindir}` and
  `%?dist` becomes `%{?dist}`. Known RPM references and non-parameterized macros
  defined in the spec are handled; directives, argument-taking calls, `%%`
  escapes, comments, and existing braced/shell expressions are left alone.
- Safe literal `%define` declarations become `%global`, and safe literal
  declarations after `Name:` move before it. Definitions with macro expansion,
  parameters, continuations, redefinitions, or earlier uses that would change
  meaning are not automatically converted or moved; the linter reports them.
- Consecutive tags with the same name are sorted.
  Dependency tags (`BuildRequires`, `Requires`, ...) are sorted by dependency
  token and wrapped (except `BuildRequires`, which uses one expression per line).
  Other tags are sorted by value.
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
- A spec with executable macros before/between preamble tags, or preamble
  conditionals: the entire file is returned unchanged, and the
  formatter prints an `unsafe-preamble` warning to stderr. Use `rpmspeclint`
  for layout and style findings in these files. `--check` only reports actual
  formatting changes; use the linter's exit status to detect these findings.
- Executable macro calls after all explicit preamble tags (for example
  `%electronmeta -D`) stay after the tags, with the entire trailing macro block
  preserved verbatim. The tags before that block can still be formatted.
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
