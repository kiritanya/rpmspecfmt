use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind};
use crate::{rpm, style};

#[derive(Debug, Clone)]
pub struct TagAlignment;

impl Rule for TagAlignment {
    fn id(&self) -> &'static str {
        "tag-alignment"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Format
    }
    fn message(&self) -> &'static str {
        "align values within each tag group at the longest tag plus one space"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.group_tags || !context.unsafe_preamble {
            return Ok(Vec::new());
        }
        let mut widths = vec![0; rpm::TAG_GROUPS.len() + 1];
        for tag in context.preamble_tags() {
            let group = style::tag_order(tag.key).0;
            widths[group] = widths[group].max(tag.key.len() + 2);
        }
        let mut findings = Vec::new();
        for tag in context.preamble_tags() {
            let column = if context.config.tag_column > 0 {
                context.config.tag_column
            } else {
                widths[style::tag_order(tag.key).0]
            };
            let expected = format!(
                "{}:{}{}",
                tag.key,
                " ".repeat(column.saturating_sub(tag.key.len() + 1).max(1)),
                tag.value
            );
            let line = &context.lines[tag.line];
            if !tag.value.is_empty() && line.text != expected {
                findings.push(self.finding(context.point_span(line.span.start), None));
            }
        }
        Ok(findings)
    }
}
