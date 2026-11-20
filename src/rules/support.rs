use stable_eyre::eyre::Result;
use tree_sitter::Tree;

use super::LineRule;
use crate::context::{Context, Line};
use crate::diagnostic::{Finding, Fix};
use crate::style;

pub(super) fn check_lines(rule: &impl LineRule, context: &Context<'_>) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    for line in &context.lines {
        findings.extend(rule.check_line(line, context)?);
    }
    Ok(findings)
}

pub(super) fn format_fix(context: &Context<'_>, line: &Line<'_>) -> Option<Fix> {
    if line.editable {
        context.replacement(context.formatting_target(&context.tree))
    } else {
        None
    }
}

pub(super) fn directive_span(context: &Context<'_>, line: &Line<'_>) -> std::ops::Range<usize> {
    context.point_span(line.span.start + line.text.len() - line.text.trim_start().len())
}

#[derive(Clone, Copy)]
pub(super) enum DefinitionChange {
    Global,
    BeforeName,
}

/// Shared safety/plan selection, not detection: each definition rule decides
/// which source declaration to report before asking for a possible fix.
pub(super) fn definition_fix(
    context: &Context<'_>,
    tree: &Tree,
    line: &Line<'_>,
    change: DefinitionChange,
) -> Option<Fix> {
    if context.unsafe_preamble || !line.editable {
        return None;
    }
    let (_, name, _) = style::definition(line.text)?;
    let target = context.definition_target(tree);
    let declarations: Vec<_> = target
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            style::definition(line)
                .filter(|(_, candidate, _)| *candidate == name)
                .map(|(directive, _, _)| (index, directive))
        })
        .collect();
    if declarations.len() != 1 {
        return None;
    }
    let changed = match change {
        DefinitionChange::Global => declarations[0].1 == "global",
        DefinitionChange::BeforeName => {
            let name_index = target
                .lines()
                .position(|line| {
                    style::split_tag_line(line.trim_start())
                        .is_some_and(|(tag, _, _)| tag == "Name")
                })
                .unwrap_or(usize::MAX);
            declarations[0].0 < name_index
        }
    };
    if changed {
        context.replacement(target)
    } else {
        None
    }
}
