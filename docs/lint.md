# Linter

`rpmspecfmt --lint` checks semantic and style issues without expanding macros. It
uses structural facts and text checks that can still run when tree-sitter
reports errors. Semantic replacement recommendations such as `use-evr`
require a recognized, error-free dependency subtree.

```sh
rpmspecfmt --lint foo.spec
rpmspecfmt --lint < foo.spec
rpmspecfmt --lint --fix foo.spec
rpmspecfmt --lint --fix foo.spec bar.spec
```

## Safe fixes

Without `--fix`, linting is read-only. With `--fix`, safe changes are written
in place and only remaining findings are printed, using the fixed file's line
numbers. File paths are required; stdin is lint-only.

`--lint --fix` uses the same TOML/environment/CLI configuration layering as
formatting. Its default profile does not sort dependency values, wrap runtime
dependencies/descriptions, or reindent scriptlets; explicit configuration can
enable these. Optional path conversion remains disabled by default. It
groups and aligns tags, splits `BuildRequires`, braces macro references, and
converts/hoists safe literal macro definitions. Whitespace is also normalized.

Macros before/between preamble tags and preamble conditionals prevent all
automatic changes. Macro calls after all explicit preamble tags remain in
place and do not prevent formatting the tags before them. Definitions whose
scope or expansion timing could change remain in place for manual review.

`use-evr`, `use-autosetup`, missing license, and missing changelog findings are
not automatically fixed. See [formatting safety](formatting.md#safety).

## Diagnostic format and exit status

The default Ariadne reports show source excerpts and highlight affected ranges
on stderr. Formatted spec output and diffs remain on stdout. Use
`--color auto|always|never` to control colors (`NO_COLOR` is respected in auto
mode). For one-line diagnostic output on stdout, use:

```sh
rpmspecfmt --lint --diagnostic-format short foo.spec
```

Each finding includes its stable rule ID:

```text
foo.spec:6:23: warning: use %{evr} instead of %{version}-%{release} [use-evr]
```

For SARIF 2.1.0 JSON, use:

```sh
rpmspecfmt --lint --diagnostic-format sarif foo.spec bar.spec > results.sarif
rpmspecfmt --lint --fix --diagnostic-format sarif foo.spec > remaining.sarif
```

SARIF requires `--lint` to avoid mixing JSON with formatted source or diffs.
All files in one invocation appear in a single run/document on stdout, even
when there are no findings (`results: []`). Logs and operational errors remain
on stderr; colors do not affect JSON. Exit status remains `1` when findings
exist, so CI scripts should distinguish that status from operational errors.

Results include stable rule IDs and catalog indexes, warning severity,
messages, and complete start/end ranges (including grouped multiline findings).
Columns are one-based Unicode code points, explicitly declared with SARIF's
`columnKind`. Paths are percent-encoded URI references; absolute paths use
`file://` URIs and stdin uses the logical URI `%3Cstdin%3E`.

Available safe fixes are represented as SARIF artifact changes and replacements
against the reported source revision. CheckOnly results never offer fixes.
With `--fix`, results refer to the rewritten files and contain only remaining
findings. If reading, inspection, or fixing fails before serialization, no
partial SARIF document is emitted (earlier files may already have been fixed
with `--fix`). Output/write failures are reported as operational errors.

| Exit status | Meaning |
| --- | --- |
| `0` | No remaining findings |
| `1` | Findings remain |
| `2` | Input, formatting, or write error; also `--fix` without file paths |

## Rule IDs

Rules have three kinds: **Format**, **Fixable**, and **CheckOnly**. A Format or
Fixable finding can still lack a fix when parser/macro safety guards block the
change. Ordinary formatting applies only Format rules; `--lint --fix` also
applies Fixable rules, never CheckOnly rules.

| Rule | Kind | What it looks for |
| --- | --- | --- |
| `format` | Format | Enabled whitespace, grouping/alignment, sorting/wrapping, and presentation changes |
| `missing-changelog` | CheckOnly | No `%changelog` or `%autochangelog` |
| `missing-license` | CheckOnly | No `License:` tag |
| `use-evr` | CheckOnly | `%{version}-%{release}` or `%version-%release` in dependency tags |
| `use-autosetup` | CheckOnly | `%setup`; `%autosetup` is preferred |
| `buildrequires-one-per-line` | Format | Multiple dependency expressions in one `BuildRequires:` line |
| `braced-macros` | Format | Bare references such as `%_bindir` and `%?dist` |
| `use-global` | Fixable | `%define`; check expansion timing before converting to `%global` |
| `macro-placement` | Fixable | Macro definitions after `Name:` |
| `path-macros` | Fixable | Literal paths in `%files` when `convert_paths` is enabled |
| `unsafe-preamble` | CheckOnly | Executable macros before/between preamble tags, or preamble conditionals |
| `tag-group-order` | Format | Incorrect tag group order in a lint-only preamble |
| `tag-group-spacing` | Format | No blank line between adjacent groups in a lint-only preamble |
| `tag-alignment` | Format | Values not aligned within their group in a lint-only preamble |

`use-evr` requires the `%{evr}` macro. On Terra it comes from
`anda-srpm-macros`.

`%{evr}` already includes the optional epoch. For
`%{?epoch:%{epoch}:}%{version}-%{release}`, the recommendation applies to the
**entire expression**, not just the version-release suffix: retaining the
explicit epoch prefix would duplicate the epoch. Plain version-release
recommendations are suppressed when an `Epoch:` tag or an `epoch` definition
is present. Nested macro fragments, unrecognized epoch prefixes, and version
expressions with additional prefixes/suffixes are left for manual review
without a replacement recommendation. `use-evr` never supplies an automatic fix.
Detection inspects dependency version fields, scalar macro nodes, and the
conditional epoch subtree, not a substring search through tag text. It does
not inspect dependency names/qualifiers or nested macro bodies. Damaged
dependency subtrees are skipped without a text-search fallback; unrelated
clean dependency subtrees can still be checked on an ERROR tree.

## Old executable

`rpmspeclint` remains a thin alias for `rpmspecfmt --lint`. It defaults to short
diagnostics for existing scripts; `--diagnostic-format rich` opts into Ariadne.
An explicit `--diagnostic-format sarif` selects SARIF through the same backend.
Its `--fix` option is equivalent to `rpmspecfmt --lint --fix`. There is no
separate linter implementation or configuration search.
