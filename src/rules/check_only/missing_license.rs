use stable_eyre::eyre::Result;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind};

#[derive(Debug, Clone)]
pub struct MissingLicense;

impl Rule for MissingLicense {
    fn id(&self) -> &'static str {
        "missing-license"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::CheckOnly
    }
    fn message(&self) -> &'static str {
        "no License tag"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        if context.source.is_empty() || context.tags.iter().any(|tag| tag.name() == "License") {
            return Ok(Vec::new());
        }
        Ok(vec![self.finding(context.point_span(0), None)])
    }
}
