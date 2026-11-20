# Shared rule engine

## Rule kinds and entry points

Every built-in rule implements `Rule` and declares one of three kinds:

- `Format`: presentation changes. `BuildRequires` splitting belongs here
  because dependency expressions themselves remain intact.
- `Fixable`: lint recommendations with potentially safe fixes, such as literal
  definition conversion/movement and configured path macros.
- `CheckOnly`: findings that require manual review.

Rule kind and fix availability are separate. Parser errors, heredocs,
continuations, preamble macros, and expansion-sensitive definitions can remove
the automatic fix without changing a rule's kind.

`rpmspecfmt` applies Format rules. `--lint` inspects all three kinds without
writing. `--lint --fix` applies Format and Fixable plans, then reports remaining
findings. `rpmspeclint` uses the same CLI with lint mode selected.

## Source context

`Context` parses a source revision once with tree-sitter and holds:

- the original `&str` and owned `tree_sitter::Tree`;
- borrowed lines, 1-based line numbers, UTF-8 byte ranges, and editability;
- configuration, tag-region facts, macro declarations, and preamble boundaries;
- cached formatting, definition, and path-replacement plans.

The parser never expands macros. An ERROR tree does not stop line-based
checks. A normalized copy ending in a newline avoids artificial EOF errors;
all diagnostics and edits still refer to the original source's byte offsets.

## Traits and dispatch

```rust,ignore
trait Rule {
    fn id(&self) -> &'static str;
    fn kind(&self) -> RuleKind;
    fn message(&self) -> &'static str;
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>>;
}
```

`TreeRule::check_tree` receives `&Tree` and the context for structural rules.
`LineRule::check_line` receives a borrowed `Line` (including its `&str`) and
context. Both adapt to `Rule`.

`RuleSet` uses `enum_dispatch` for built-ins. Add an implementation and register
its variant/instance in `src/rules/mod.rs`; there is no trait-object allocation
per rule. Formatting reuses the tested tree-aware backend. Every other rule
detects its own findings directly from the context's
tree, borrowed lines, or parsed tag facts. `Context` never calls the linter or
generates diagnostics.

Rules are organized one per file, grouped by kind:

```text
src/rules/
  mod.rs                         # traits, metadata, enum_dispatch registry
  support.rs                     # shared iteration and safe-plan helpers
  format/
    formatting.rs
    braced_macros.rs
    buildrequires_one_per_line.rs
    tag_alignment.rs
    tag_group_order.rs
    tag_group_spacing.rs
  fixable/
    use_global.rs
    macro_placement.rs
    path_macros.rs
  check_only/
    missing_license.rs
    missing_changelog.rs
    use_evr.rs
    use_autosetup.rs
    unsafe_preamble.rs
```

`lint.rs` contains only compatibility entry points and diagnostic conversion.
`lint_with_config()` calls `Engine::check()`, which uses the registry above.
It returns a `Result`; `lint()` retains the original Vec-returning API with
default configuration. Detection logic and pattern tables live in their
individual rule files, not in a shared string-ID dispatcher.

## Findings and fixes

A `Finding` owns its ID, kind, message, source range, and optional `Fix`.
No tree node is retained across source changes. A `Fix` contains one or more
byte-range `Edit`s, each with replacement text. Rules never write files.
Replacement text is shared with `Arc<str>`, so hundreds of diagnostics can
reference the same layout snapshot without copying the whole spec hundreds
of times. Tag facts are indexed by line; rules do not scan a precomputed
diagnostic list for every source line.

The engine groups repeated findings with the same rule, kind, message, and
fix plan into a single multiline range. Adjacent affected lines and intervening
blank lines can form one block; unaffected nonblank lines and conditionals
separate blocks. Single findings retain their precise ranges. Grouping changes
diagnostic presentation, not the edits in a fix. Ariadne elides the middle of
long blocks; short diagnostics keep their one-line format at the block's start.

The engine:

1. Checks the cached source context.
2. Filters fixes by kind, with Format plans first.
3. Validates bounds, UTF-8 boundaries, and overlap within each atomic fix.
4. Defers an entire fix if it overlaps an accepted plan.
5. Applies accepted edits from the end of the source backwards.
6. Parses the resulting revision and checks again until unchanged.

Broad layout plans may replace the whole source. Definition/path plans are
then evaluated on the next revision, not mixed with stale offsets. A 16-pass
limit and cycle detection prevent runaway interactions. CheckOnly fixes are
never applied, even if an implementation erroneously offers one.

`Outcome` returns the final source and findings from that exact revision.
The CLI uses it to avoid a redundant final parse and to report correct line
numbers after edits. Ariadne uses byte-indexed labels; short reports translate
them to character-based columns, including Unicode input.

## Configuration and observability

Figment merges defaults, TOML, prefixed environment variables, and explicit
CLI values. `--safe` is applied last as a safety invariant. Path mapping entries
extend/override the built-in table and remain sorted longest-prefix first.

`stable-eyre` handles operational errors; lint findings are not operational
errors. `tracing` writes engine logs to stderr, filtered by
`clap_verbosity_flag` and `RUST_LOG`. Ariadne reports are independent of logging.
