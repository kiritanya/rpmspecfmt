use stable_eyre::eyre::Result;

use crate::context::{Context, Line};
use crate::diagnostic::Finding;
use crate::rules::{support, LineRule, Rule, RuleKind};
use crate::style;

#[derive(Debug, Clone)]
pub struct BuildRequiresOnePerLine;

impl Rule for BuildRequiresOnePerLine {
    fn id(&self) -> &'static str {
        "buildrequires-one-per-line"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Format
    }
    fn message(&self) -> &'static str {
        "put each BuildRequires dependency expression on its own line"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        support::check_lines(self, context)
    }
}

impl LineRule for BuildRequiresOnePerLine {
    fn check_line(&self, line: &Line<'_>, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.split_buildrequires {
            return Ok(Vec::new());
        }
        let Some(tag) = context
            .tag_at(line.number)
            .filter(|tag| tag.name() == "BuildRequires")
        else {
            return Ok(Vec::new());
        };
        if style::dependencies(tag.value).is_none_or(|dependencies| dependencies.len() <= 1) {
            return Ok(Vec::new());
        }
        Ok(vec![self.finding(
            context.point_span(tag.value_offset),
            support::format_fix(context, line),
        )])
    }
}
