# Formatter configuration

The formatter reads the first `.rpmspecfmt.toml` in the current directory or
one of its parents. `--config PATH` selects a file directly. Figment applies
**defaults → TOML → `RPMSPECFMT_*` environment → CLI**. `--safe` then disables
non-whitespace changes regardless of providers. This applies to linting and
safe fixes too. A missing explicit configuration file is an error.

```sh
RPMSPECFMT_MAX_WIDTH=80 rpmspecfmt foo.spec
rpmspecfmt --lint --fix --config style.toml foo.spec
```

## CLI options

| Option | Effect |
| --- | --- |
| `--safe` | Whitespace only; no grouping, macro conversion, sorting, wrapping, or reindent |
| `--no-sort` | Keep the order of values for tags with the same name |
| `--no-wrap` | Do not wrap long dependency tags; `BuildRequires` splitting is separate |
| `--no-reindent` | Do not expand leading tabs in scriptlets |
| `--config PATH` | Read this configuration file |
| `--tag-column N` | Fixed value column; default `0` aligns each group automatically |
| `--max-width N` | Wrapping width, default `100`; alias `--wrap-width` |
| `--lint` | Inspect Format, Fixable, and CheckOnly rules without rewriting |
| `--lint --fix` | Apply Format/Fixable changes in place and report remaining findings |
| `--diagnostic-format rich\|short` | Ariadne excerpts (stderr) or one-line diagnostics (stdout) |
| `--color auto\|always\|never` | Diagnostic/log colors |
| `-v` / `-q` | Increase/decrease tracing verbosity; does not hide findings |

## Configuration file

This example includes all settings. Optional description/path transformations
are disabled by default.

```toml
tag_column = 0
max_width = 100
group_tags = true
split_buildrequires = true
normalize_macros = true
global_macros = true
sort = true
wrap = true
reindent = true
wrap_description = false
convert_paths = false

[path_macros]
"/opt/example" = "%{_prefix}/opt/example"
```

In `--lint --fix` mode, the defaults for `sort`, `wrap`, and `reindent` are
`false`; providers can override these. `global_macros` and `convert_paths`
configure Fixable rules, so they only transform source with `--lint --fix`.
Disabled rules do not produce their associated findings.

Tracing writes to stderr. `-v` enables info, `-vv` debug, and `-vvv` trace;
`RUST_LOG` can override the filter. Diagnostic output remains visible with `-q`.

`wrap_description` reflows long `%description` lines at `max_width`, preserving
blank lines, indentation, and `*` / `-` / `+` bullets.

## Path macros

`convert_paths` replaces leading paths in `%files` with macros. Defaults cover
the Paths section of the Terra RPM macro docs, for example:

```text
/usr/share/foo             -> %{_datadir}/foo
/usr/share/doc/foo         -> %{_docdir}/foo
/usr/lib/systemd/system/x  -> %{_unitdir}/x
/var/lib/foo               -> %{_sharedstatedir}/foo
```

Some defaults require `anda-srpm-macros` (Terra only). Entries in `[path_macros]`
extend/override the defaults; the longest prefix wins. Literal prefixes still
match paths containing macros: `/usr/share/doc/%{name}` becomes
`%{_docdir}/%{name}`. Comments and quoted paths are unchanged.

The built-in table lives in [`src/path_macros.rs`](../src/path_macros.rs).
