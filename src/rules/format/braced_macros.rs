use stable_eyre::eyre::Result;

use crate::context::{Context, Line};
use crate::diagnostic::Finding;
use crate::rules::{support, LineRule, Rule, RuleKind};
use crate::style;

#[derive(Debug, Clone)]
pub struct BracedMacros;

impl Rule for BracedMacros {
    fn id(&self) -> &'static str {
        "braced-macros"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Format
    }
    fn message(&self) -> &'static str {
        "use braced macro references, such as %{_bindir} and %{?dist}"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        support::check_lines(self, context)
    }
}

impl LineRule for BracedMacros {
    fn check_line(&self, line: &Line<'_>, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.normalize_macros {
            return Ok(Vec::new());
        }
        let braced = style::brace_macros_with_names(line.text, &context.macro_declarations);
        let Some(offset) = line
            .text
            .bytes()
            .zip(braced.bytes())
            .position(|(a, b)| a != b)
        else {
            return Ok(Vec::new());
        };
        let start = line.span.start + offset.saturating_sub(1);
        let mut end = start + 1;
        while context
            .source
            .as_bytes()
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || b"_!?".contains(byte))
        {
            end += 1;
        }
        Ok(vec![
            self.finding(start..end, support::format_fix(context, line))
        ])
    }
}
