//! Rules declare policy and return plans; only the engine changes source text.
use enum_dispatch::enum_dispatch;
use stable_eyre::eyre::Result;
use std::ops::Range;
use tree_sitter::Tree;

use crate::context::{Context, Line};
use crate::diagnostic::{Finding, Fix};

pub mod check_only;
pub mod fixable;
pub mod format;
mod support;
pub use check_only::{MissingChangelog, MissingLicense, UnsafePreamble, UseAutosetup, UseEvr};
pub use fixable::{MacroPlacement, PathMacros, UseGlobal};
pub use format::{
    BracedMacros, BuildRequiresOnePerLine, Formatting, TagAlignment, TagGroupOrder, TagGroupSpacing,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleKind {
    Format,
    Fixable,
    CheckOnly,
}

#[enum_dispatch]
pub trait Rule {
    fn id(&self) -> &'static str;
    fn kind(&self) -> RuleKind;
    fn message(&self) -> &'static str;
    /// Inspect one context and return every finding for this rule.
    ///
    /// # Errors
    ///
    /// Returns an error when the rule cannot inspect the context.
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>>;
    fn finding(&self, span: Range<usize>, fix: Option<Fix>) -> Finding {
        Finding {
            rule: self.id(),
            kind: self.kind(),
            span,
            message: self.message().into(),
            fix,
        }
    }
}

/// Structure-sensitive rules can consume the cached tree directly.
pub trait TreeRule {
    /// Inspect a parsed tree and return findings for this rule.
    ///
    /// # Errors
    ///
    /// Returns an error when the rule cannot inspect the tree.
    fn check_tree(&self, tree: &Tree, context: &Context<'_>) -> Result<Vec<Finding>>;
}

/// Line rules receive borrowed source text and byte ranges, plus file context.
pub trait LineRule {
    /// Inspect one line and return findings for this rule.
    ///
    /// # Errors
    ///
    /// Returns an error when the rule cannot inspect the line.
    fn check_line(&self, line: &Line<'_>, context: &Context<'_>) -> Result<Vec<Finding>>;
}

/// Static dispatch for built-ins, without a Box<dyn Rule> allocation per rule.
#[enum_dispatch(Rule)]
#[derive(Debug, Clone)]
pub enum RuleSet {
    Formatting,
    BracedMacros,
    BuildRequiresOnePerLine,
    TagAlignment,
    TagGroupOrder,
    TagGroupSpacing,
    UseGlobal,
    MacroPlacement,
    PathMacros,
    MissingLicense,
    MissingChangelog,
    UseEvr,
    UseAutosetup,
    UnsafePreamble,
}

const BUILTINS: [RuleSet; 14] = [
    RuleSet::Formatting(Formatting),
    RuleSet::BracedMacros(BracedMacros),
    RuleSet::BuildRequiresOnePerLine(BuildRequiresOnePerLine),
    RuleSet::TagAlignment(TagAlignment),
    RuleSet::TagGroupOrder(TagGroupOrder),
    RuleSet::TagGroupSpacing(TagGroupSpacing),
    RuleSet::UseGlobal(UseGlobal),
    RuleSet::MacroPlacement(MacroPlacement),
    RuleSet::PathMacros(PathMacros),
    RuleSet::MissingLicense(MissingLicense),
    RuleSet::MissingChangelog(MissingChangelog),
    RuleSet::UseEvr(UseEvr),
    RuleSet::UseAutosetup(UseAutosetup),
    RuleSet::UnsafePreamble(UnsafePreamble),
];

#[must_use]
pub fn builtins() -> Vec<RuleSet> {
    BUILTINS.to_vec()
}
