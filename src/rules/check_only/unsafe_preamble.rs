use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind};

#[derive(Debug, Clone)]
pub struct UnsafePreamble;

impl Rule for UnsafePreamble {
    fn id(&self) -> &'static str {
        "unsafe-preamble"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::CheckOnly
    }
    fn message(&self) -> &'static str {
        "preamble contains a macro before tags or a conditional; group and align tags manually (automatic formatting is disabled)"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        let Some(index) = context.unsafe_preamble_line else {
            return Ok(Vec::new());
        };
        Ok(vec![self.finding(
            context.point_span(context.lines[index].span.start),
            None,
        )])
    }
}
