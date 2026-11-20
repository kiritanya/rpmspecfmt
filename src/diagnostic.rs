use std::ops::Range;
use std::sync::Arc;

use crate::rules::RuleKind;

/// Source positions are UTF-8 byte offsets, matching tree-sitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub kind: RuleKind,
    pub span: Range<usize>,
    pub message: String,
    pub fix: Option<Fix>,
}

/// All edits in a fix are accepted or deferred together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub edits: Vec<Edit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    /// Shared between diagnostics when a broad formatting plan is reused.
    pub replacement: Arc<str>,
}
