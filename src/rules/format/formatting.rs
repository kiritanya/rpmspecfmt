use stable_eyre::eyre::Result;
use tree_sitter::Tree;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind, TreeRule};

#[derive(Debug, Clone)]
pub struct Formatting;

impl Rule for Formatting {
    fn id(&self) -> &'static str {
        "format"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Format
    }
    fn message(&self) -> &'static str {
        "spec layout differs from the configured formatting style"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        self.check_tree(&context.tree, context)
    }
}

impl TreeRule for Formatting {
    fn check_tree(&self, tree: &Tree, context: &Context<'_>) -> Result<Vec<Finding>> {
        let target = context.formatting_target(tree);
        let Some(fix) = context.replacement(target) else {
            return Ok(Vec::new());
        };
        let offset = context
            .source
            .bytes()
            .zip(target.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| context.source.len().min(target.len()));
        let span = context
            .lines
            .iter()
            .find(|line| line.span.end >= offset)
            .map(|line| line.span.clone())
            .unwrap_or(offset..offset);
        Ok(vec![self.finding(span, Some(fix))])
    }
}
