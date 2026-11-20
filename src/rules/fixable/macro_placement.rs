use stable_eyre::eyre::Result;
use tree_sitter::Tree;

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{support, Rule, RuleKind, TreeRule};
use crate::style;

#[derive(Debug, Clone)]
pub struct MacroPlacement;

impl Rule for MacroPlacement {
    fn id(&self) -> &'static str {
        "macro-placement"
    }
    fn kind(&self) -> RuleKind {
        RuleKind::Fixable
    }
    fn message(&self) -> &'static str {
        "place macro definitions before Name; check scope and expansion order before moving"
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        self.check_tree(&context.tree, context)
    }
}

impl TreeRule for MacroPlacement {
    fn check_tree(&self, tree: &Tree, context: &Context<'_>) -> Result<Vec<Finding>> {
        if !context.config.global_macros {
            return Ok(Vec::new());
        }
        let mut findings = Vec::new();
        for (index, line) in context.lines.iter().enumerate() {
            if index <= context.name_line || style::definition(line.text).is_none() {
                continue;
            }
            let fix =
                support::definition_fix(context, tree, line, support::DefinitionChange::BeforeName);
            findings.push(self.finding(support::directive_span(context, line), fix));
        }
        Ok(findings)
    }
}
