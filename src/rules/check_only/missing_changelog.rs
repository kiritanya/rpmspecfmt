use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind};
use crate::style;

#[derive(Debug, Clone)]
pub struct MissingChangelog;

impl Rule for MissingChangelog {
    fn id(&self) -> &'static str {
        "missing-changelog"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::CheckOnly
    }
    fn message(&self) -> &'static str {
        "no %changelog or %autochangelog section"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        if context.source.is_empty()
            || context.lines.iter().any(|line| {
                matches!(
                    style::section_word(line.text),
                    "changelog" | "autochangelog"
                )
            })
        {
            return Ok(Vec::new());
        }
        Ok(vec![self.finding(context.point_span(0), None)])
    }
}
