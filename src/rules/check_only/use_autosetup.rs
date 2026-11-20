use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{support, Rule, RuleKind};
use crate::style;

#[derive(Debug, Clone)]
pub struct UseAutosetup;

impl Rule for UseAutosetup {
    fn id(&self) -> &'static str {
        "use-autosetup"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::CheckOnly
    }
    fn message(&self) -> &'static str {
        "use %autosetup instead of %setup"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        // Extra unpacking calls cannot be replaced by a single %autosetup.
        let line = context.lines.iter().find(|line| {
            style::section_word(line.text) == "setup"
                && !line
                    .text
                    .split_whitespace()
                    .skip(1)
                    .any(|arg| matches!(arg, "-T" | "-D" | "-a" | "-b"))
        });
        Ok(line.map_or_default(|line| {
            vec![self.finding(support::directive_span(context, line), None)]
        }))
    }
}
