use stable_eyre::eyre::Result;

use crate::context::{Context, Line};
use crate::diagnostic::Finding;
use crate::rules::{support, LineRule, Rule, RuleKind};
use crate::style;

#[derive(Debug, Clone)]
pub struct UseGlobal;

impl Rule for UseGlobal {
    fn id(&self) -> &'static str {
        "use-global"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Fixable
    }
    fn message(&self) -> &'static str {
        "prefer %global to %define; check expansion timing before converting"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        support::check_lines(self, context)
    }
}

impl LineRule for UseGlobal {
    fn check_line(&self, line: &Line<'_>, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.global_macros
            || style::definition(line.text).is_none_or(|(directive, _, _)| directive != "define")
        {
            return Ok(Vec::new());
        }
        let fix = support::definition_fix(
            context,
            &context.tree,
            line,
            support::DefinitionChange::Global,
        );
        Ok(vec![
            self.finding(support::directive_span(context, line), fix)
        ])
    }
}
