use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind};
use crate::style;

#[derive(Debug, Clone)]
pub struct TagGroupOrder;

impl Rule for TagGroupOrder {
    fn id(&self) -> &'static str {
        "tag-group-order"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Format
    }
    fn message(&self) -> &'static str {
        "group metadata, sources/patches, dependencies, BuildRequires, and build options in that order"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.group_tags || !context.unsafe_preamble {
            return Ok(Vec::new());
        }
        let mut previous = None;
        let mut findings = Vec::new();
        for tag in context.preamble_tags() {
            let order = style::tag_order(tag.key);
            if previous.is_some_and(|previous| order < previous) {
                findings.push(
                    self.finding(context.point_span(context.lines[tag.line].span.start), None),
                );
            }
            previous = Some(order);
        }
        Ok(findings)
    }
}
