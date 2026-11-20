use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind};
use crate::style;

#[derive(Debug, Clone)]
pub struct TagGroupSpacing;

impl Rule for TagGroupSpacing {
    fn id(&self) -> &'static str {
        "tag-group-spacing"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Format
    }
    fn message(&self) -> &'static str {
        "separate tag groups with a blank line"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.group_tags || !context.unsafe_preamble {
            return Ok(Vec::new());
        }
        let mut previous = None;
        let mut findings = Vec::new();
        for tag in context.preamble_tags() {
            let group = style::tag_order(tag.key).0;
            if previous.is_some_and(|(line, previous_group)| {
                tag.line == line + 1 && group != previous_group
            }) {
                findings.push(
                    self.finding(context.point_span(context.lines[tag.line].span.start), None),
                );
            }
            previous = Some((tag.line, group));
        }
        Ok(findings)
    }
}
