use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind};

#[derive(Debug, Clone)]
pub struct PathMacros;

impl Rule for PathMacros {
    fn id(&self) -> &'static str {
        "path-macros"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Fixable
    }
    fn message(&self) -> &'static str {
        "use configured RPM path macros in %files"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.convert_paths || context.unsafe_preamble {
            return Ok(Vec::new());
        }
        let target = context.path_target();
        let Some(fix) = context.replacement(target) else {
            return Ok(Vec::new());
        };
        let offset = context
            .source
            .bytes()
            .zip(target.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        let span = context
            .lines
            .iter()
            .find(|line| line.span.end >= offset)
            .map(|line| line.span.clone())
            .unwrap_or(0..0);
        Ok(vec![self.finding(span, Some(fix))])
    }
}
