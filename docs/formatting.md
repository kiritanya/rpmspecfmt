# Formatting behavior

## Tag groups

Preamble tags are grouped in this order, separated by one blank line:

1. Metadata: `Name`, `Epoch`, `Version`, `Release`, `Summary`, `License`, `URL`,
   `Packager`, and other metadata/architecture tags.
2. Sources and patches, ordered numerically.
3. Runtime dependencies: `Provides`, `Supplements`, `Enhances`, `Recommends`,
   `Suggests`, `Obsoletes`, `Conflicts`, `Requires`, `OrderWithRequires`.
4. `BuildRequires` and `BuildConflicts`.
5. `BuildSystem` and `BuildOption(...)`.
6. Other tags.

Comments directly above a tag move with it. Values start one space after the
longest tag, including its colon, in each group. A nonzero `tag_column`
overrides automatic alignment. Policy tables live in
[`src/rpm.rs`](../src/rpm.rs).

## Dependencies and macros

- Each `BuildRequires:` line contains one dependency expression. Version
  constraints (`foo >= 1.2`) and rich dependencies (`(foo if bar)`) stay intact.
  `--no-wrap` does not disable splitting; use `split_buildrequires = false`
  or `--safe` to retain multiple expressions.
- Consecutive tags with the same name are sorted by value. Dependency tags are
  sorted by dependency token and wrapped, except `BuildRequires`, which uses
  one expression per line.
- Long dependency lines are wrapped at the configured width. Parentheses,
  comparisons, quotes, braced macro expressions, and `%(` prevent wrapping where splitting could change
  meaning.
- `%_bindir` becomes `%{_bindir}`, and `%?dist` becomes `%{?dist}`. Known RPM
  references and non-parameterized macros defined in the spec are recognized.
  Directives, argument-taking calls, `%%` escapes, comments, and existing
  braced/shell expressions are unchanged.
- With `--lint --fix`, safe literal `%define` declarations become `%global`, and safe literal
  declarations after `Name:` move before it. Definitions with macro expansion,
  parameters, continuations, redefinitions, or earlier uses that could change
  meaning are left for manual review.

## Whitespace and optional transformations

- Add one blank line before section headers; remove blanks immediately after.
- Collapse repeated blank lines outside description/scriptlet bodies.
- Remove trailing spaces and end the file with one newline.
- Expand leading scriptlet tabs to four spaces unless reindent is disabled.
- Optionally wrap descriptions; `--lint --fix` can also convert paths in `%files`; see
  [configuration](configuration.md).

## Safety

- Executable macros before/between preamble tags, or preamble conditionals,
  prevent automatic formatting of the entire file. The formatter prints an
  `unsafe-preamble` warning. Use `rpmspecfmt --lint` for findings.
- Executable macros after all explicit preamble tags, such as
  `%electronmeta -D`, remain after the tags. The entire trailing macro block is
  preserved verbatim while the preceding tags can still be formatted.
- Tree-sitter `ERROR` / `MISSING` lines and sections with parser errors are
  left alone, as are continuations and heredoc bodies.
- `--safe` changes whitespace only.

`--check` only detects actual formatting changes. Use the linter's exit status
to detect warnings that require manual review.
