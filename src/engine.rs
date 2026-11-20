use std::collections::HashSet;
use std::ops::Range;

use stable_eyre::eyre::{bail, ensure, Result};

use crate::context::Context;
use crate::diagnostic::{Edit, Finding, Fix};
use crate::rules::{self, Rule, RuleKind, RuleSet};
use crate::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixMode {
    FormatOnly,
    SafeFix,
}

pub struct Engine {
    pub config: Config,
    rules: Vec<RuleSet>,
    max_passes: usize,
}

/// Findings refer to this exact source revision, including after safe fixes.
pub struct Outcome {
    pub source: String,
    pub findings: Vec<Finding>,
}

impl Engine {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            rules: rules::builtins(),
            max_passes: 16,
        }
    }

    /// Run every registered rule against one source revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the source context cannot be constructed or a
    /// rule fails while inspecting it. Parser errors are findings, not errors.
    pub fn check(&self, source: &str) -> Result<Vec<Finding>> {
        let context = Context::new(source, &self.config)?;
        self.check_context(&context)
    }

    fn check_context(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        let mut findings = Vec::new();
        for rule in &self.rules {
            tracing::trace!(rule = rule.id(), kind = ?rule.kind(), "checking rule");
            findings.extend(group_findings(context, rule.check(context)?));
        }
        findings.sort_by(|a, b| (a.span.start, a.rule).cmp(&(b.span.start, b.rule)));
        findings.dedup();
        Ok(findings)
    }

    /// Apply eligible fixes repeatedly and return the rewritten source.
    ///
    /// # Errors
    ///
    /// Returns an error when rule inspection, fix validation, or convergence
    /// fails.
    pub fn apply(&self, source: &str, mode: FixMode) -> Result<String> {
        self.apply_with_findings(source, mode)
            .map(|outcome| outcome.source)
    }

    /// Apply eligible fixes and report findings for the returned revision.
    ///
    /// # Errors
    ///
    /// Returns an error when rule inspection, fix validation, or convergence
    /// fails.
    pub fn apply_with_findings(&self, source: &str, mode: FixMode) -> Result<Outcome> {
        let mut current = source.to_string();
        let mut seen = HashSet::from([current.clone()]);
        for pass in 0..self.max_passes {
            let context = Context::new(&current, &self.config)?;
            let mut findings = self.check_context(&context)?;
            if context.unsafe_preamble {
                return Ok(Outcome {
                    source: current,
                    findings,
                });
            }
            // Layout first. Broad layout edits defer narrower/later plans to
            // a fresh tree rather than mixing offsets from different revisions.
            findings.sort_by_key(|finding| finding.kind);
            let fixes = fixes_for_mode(&findings, mode);
            let next = apply_fixes(&current, fixes)?;
            if next == current {
                findings.sort_by(|a, b| (a.span.start, a.rule).cmp(&(b.span.start, b.rule)));
                return Ok(Outcome {
                    source: current,
                    findings,
                });
            }
            ensure!(seen.insert(next.clone()), "rule fixes entered a cycle");
            tracing::debug!(
                pass,
                bytes_before = current.len(),
                bytes_after = next.len(),
                "applied rule fixes"
            );
            current = next;
        }
        bail!(
            "rule fixes did not converge after {} passes",
            self.max_passes
        )
    }
}

/// Group repeated findings into source blocks without changing their fix plans.
fn group_findings(context: &Context<'_>, mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by_key(|finding| (finding.span.start, finding.span.end));
    findings.dedup();
    let line_index = |offset| {
        context
            .lines
            .partition_point(|line| line.span.start <= offset)
            .saturating_sub(1)
    };
    let mut grouped: Vec<Finding> = Vec::new();
    for finding in findings {
        if let Some(previous) = grouped.last_mut() {
            let previous_end =
                line_index(previous.span.end.saturating_sub(1).max(previous.span.start));
            let next_start = line_index(finding.span.start);
            let compatible = previous.rule == finding.rule
                && previous.kind == finding.kind
                && previous.message == finding.message
                && previous.fix == finding.fix;
            if compatible
                && next_start > previous_end
                && context.lines[previous_end + 1..next_start]
                    .iter()
                    .all(|line| line.text.trim().is_empty())
            {
                previous.span.start = context.lines[line_index(previous.span.start)].span.start;
                let next_end =
                    line_index(finding.span.end.saturating_sub(1).max(finding.span.start));
                previous.span.end = context.lines[next_end].span.end;
                continue;
            }
        }
        grouped.push(finding);
    }
    grouped
}

fn fixes_for_mode(findings: &[Finding], mode: FixMode) -> impl Iterator<Item = Fix> + '_ {
    findings
        .iter()
        .filter(move |finding| match mode {
            FixMode::FormatOnly => finding.kind == RuleKind::Format,
            FixMode::SafeFix => finding.kind != RuleKind::CheckOnly,
        })
        .filter_map(|finding| finding.fix.clone())
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    if a.is_empty() {
        return (b.start..=b.end).contains(&a.start);
    }
    if b.is_empty() {
        return (a.start..=a.end).contains(&b.start);
    }
    a.start < b.end && b.start < a.end
}

/// Validate plans against the original source and apply compatible fixes as
/// atomic units. Conflicting plans are deferred to the next engine revision.
///
/// # Errors
///
/// Returns an error when a fix range is outside the source, splits a UTF-8
/// character, or overlaps another edit inside the same fix.
pub fn apply_fixes(source: &str, fixes: impl IntoIterator<Item = Fix>) -> Result<String> {
    let mut accepted: Vec<Edit> = Vec::new();
    for fix in fixes {
        for (index, edit) in fix.edits.iter().enumerate() {
            ensure!(
                edit.range.start <= edit.range.end && edit.range.end <= source.len(),
                "fix range is outside the source"
            );
            ensure!(
                source.is_char_boundary(edit.range.start)
                    && source.is_char_boundary(edit.range.end),
                "fix range splits a UTF-8 character"
            );
            ensure!(
                !fix.edits[..index]
                    .iter()
                    .any(|earlier| overlaps(&earlier.range, &edit.range)),
                "overlapping edits inside one fix"
            );
        }
        if fix.edits.iter().any(|edit| {
            accepted
                .iter()
                .any(|earlier| overlaps(&earlier.range, &edit.range))
        }) {
            tracing::trace!("deferred overlapping fix");
            continue;
        }
        accepted.extend(fix.edits);
    }
    accepted.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut out = source.to_string();
    for edit in accepted.into_iter().rev() {
        out.replace_range(edit.range, &edit.replacement);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicting_multi_edit_fixes_are_atomic() {
        let fixes = [
            Fix {
                edits: vec![Edit {
                    range: 0..1,
                    replacement: "A".into(),
                }],
            },
            Fix {
                edits: vec![
                    Edit {
                        range: 0..1,
                        replacement: "B".into(),
                    },
                    Edit {
                        range: 2..3,
                        replacement: "C".into(),
                    },
                ],
            },
        ];
        assert_eq!(apply_fixes("abc", fixes).unwrap(), "Abc");
    }

    #[test]
    fn rejects_invalid_ranges_and_utf8_boundaries() {
        for range in [0..9, 1..2] {
            let fix = Fix {
                edits: vec![Edit {
                    range,
                    replacement: "x".into(),
                }],
            };
            assert!(apply_fixes("あ", [fix]).is_err());
        }
    }

    #[test]
    fn format_mode_does_not_apply_fixable_rules() {
        let source = "Name: x\nLicense: MIT\n%define feature 1\n%autochangelog\n";
        let engine = Engine::new(Config::default());
        let formatted = engine.apply(source, FixMode::FormatOnly).unwrap();
        assert!(formatted.contains("%define feature 1"));
        let fixed = engine.apply(source, FixMode::SafeFix).unwrap();
        assert!(fixed.starts_with("%global feature 1\n"), "{fixed}");
        assert_eq!(engine.check(&fixed).unwrap(), [] as [Finding; 0]);
    }

    #[test]
    fn check_only_fixes_are_never_applied_even_if_a_rule_offers_one() {
        let findings = [Finding {
            rule: "test-check-only",
            kind: RuleKind::CheckOnly,
            span: 0..1,
            message: "manual".into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: 0..1,
                    replacement: "changed".into(),
                }],
            }),
        }];
        assert_eq!(
            apply_fixes("x", fixes_for_mode(&findings, FixMode::SafeFix)).unwrap(),
            "x"
        );
    }

    #[test]
    fn expansion_sensitive_definitions_have_no_fix() {
        let source = "Name: x\nLicense: MIT\n%define delayed %{version}\n%define literal 1\n%autochangelog\n";
        let engine = Engine::new(Config::default());
        let findings = engine.check(source).unwrap();
        let delayed_offset = source.find("%define delayed").unwrap();
        for finding in findings.iter().filter(|finding| {
            finding.kind == RuleKind::Fixable && finding.span.start == delayed_offset
        }) {
            assert!(finding.fix.is_none());
        }
        let fixed = engine.apply(source, FixMode::SafeFix).unwrap();
        assert!(fixed.contains("%define delayed %{version}"));
        assert!(fixed.starts_with("%global literal 1\n"));
    }

    #[test]
    fn parser_errors_do_not_disable_text_checks() {
        let source = "Name: x\nURL: invalid\nLicense: MIT\nRequires: x = %{version}-%{release}\n%autochangelog\n";
        let findings = Engine::new(Config::default()).check(source).unwrap();
        assert!(findings.iter().any(|finding| finding.rule == "use-evr"));
    }

    #[test]
    fn safe_fixes_preserve_protected_crlf_heredoc_lines() {
        let source = "Name: x\r\nLicense: MIT\r\n%define feature 1\r\n\r\n%build\r\ncat <<EOF\r\nraw\t text  \r\nEOF\r\n\r\n%autochangelog\r\n";
        let fixed = Engine::new(Config::default())
            .apply(source, FixMode::SafeFix)
            .unwrap();
        assert!(fixed.contains("raw\t text  \r\nEOF\r\n"), "{fixed:?}");
    }

    #[test]
    fn excessive_fix_iterations_return_an_error() {
        let mut engine = Engine::new(Config::default());
        engine.max_passes = 1;
        assert!(engine.apply("Name:    x\n", FixMode::FormatOnly).is_err());
    }

    #[test]
    fn empty_input_is_a_noop() {
        let engine = Engine::new(Config::default());
        assert_eq!(engine.apply("", FixMode::SafeFix).unwrap(), "");
        assert_eq!(engine.check("").unwrap(), [] as [Finding; 0]);
    }

    #[test]
    fn detailed_findings_share_the_formatting_snapshot() {
        let source = format!(
            "Name: x\nLicense: MIT\n\n%files\n{}",
            "%_bindir/file\n".repeat(500)
        );
        let findings = Engine::new(Config::default()).check(&source).unwrap();
        let replacements: Vec<_> = findings
            .iter()
            .filter_map(|finding| finding.fix.as_ref())
            .map(|fix| &fix.edits[0].replacement)
            .collect();
        // One aggregate layout finding and one 500-line macro block.
        assert_eq!(replacements.len(), 2);
        let macros = findings
            .iter()
            .find(|finding| finding.rule == "braced-macros")
            .unwrap();
        assert_eq!(source[macros.span.clone()].lines().count(), 500);
        assert!(replacements
            .iter()
            .all(|text| std::sync::Arc::ptr_eq(text, replacements[0])));
    }

    #[test]
    fn grouping_keeps_different_messages_kinds_and_fixes_separate() {
        let config = Config::default();
        let context = Context::new("a\nb", &config).unwrap();
        let first = Finding {
            rule: "test",
            kind: RuleKind::Format,
            span: 0..1,
            message: "same".into(),
            fix: None,
        };
        let second = Finding {
            span: 2..3,
            ..first.clone()
        };
        let merged = group_findings(&context, vec![first.clone(), second.clone()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].span, 0..3);
        for different in [
            Finding {
                message: "different".into(),
                ..second.clone()
            },
            Finding {
                kind: RuleKind::CheckOnly,
                ..second.clone()
            },
            Finding {
                rule: "other",
                ..second.clone()
            },
            Finding {
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: 2..3,
                        replacement: "c".into(),
                    }],
                }),
                ..second
            },
        ] {
            assert_eq!(
                group_findings(&context, vec![first.clone(), different]).len(),
                2
            );
        }
    }
}
