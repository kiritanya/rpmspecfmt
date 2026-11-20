use stable_eyre::eyre::Result;

use crate::context::{Context, Line};
use crate::diagnostic::Finding;
use crate::rpm;
use crate::rules::{support, LineRule, Rule, RuleKind};

const PATTERNS: &[(&str, &str)] = &[
    (
        "%{version}-%{release}",
        "use %{evr} instead of %{version}-%{release}",
    ),
    (
        "%version-%release",
        "use %{evr} instead of %version-%release",
    ),
];

#[derive(Debug, Clone)]
pub struct UseEvr;

impl Rule for UseEvr {
    fn id(&self) -> &'static str {
        "use-evr"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::CheckOnly
    }
    fn message(&self) -> &'static str {
        PATTERNS[0].1
    }
    fn static_message(&self, text: &str) -> Option<&'static str> {
        PATTERNS
            .iter()
            .map(|(_, message)| *message)
            .find(|message| *message == text)
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        support::check_lines(self, context)
    }
}

impl LineRule for UseEvr {
    fn check_line(&self, line: &Line<'_>, context: &Context<'_>) -> Result<Vec<Finding>> {
        let Some(tag) = context
            .tag_at(line.number)
            .filter(|tag| rpm::DEPENDENCY_TAGS.contains(&tag.name()))
        else {
            return Ok(Vec::new());
        };
        let matched = PATTERNS
            .iter()
            .filter_map(|(pattern, message)| {
                tag.value.find(pattern).map(|offset| (offset, *message))
            })
            .min_by_key(|(offset, _)| *offset);
        let Some((offset, message)) = matched else {
            return Ok(Vec::new());
        };
        let mut finding = self.finding(context.point_span(tag.value_offset + offset), None);
        finding.message = message.into();
        Ok(vec![finding])
    }
}
