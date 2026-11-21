use stable_eyre::eyre::Result;
use tree_sitter::{Node, Tree};

use crate::context::Context;
use crate::diagnostic::Finding;
use crate::rules::{Rule, RuleKind, TreeRule};

// Exact spellings only select a diagnostic message after the AST has
// established the complete dependency version and each macro's role.
const EXPRESSIONS: &[(&str, &str)] = &[
    (
        "%{?epoch:%{epoch}:}%{version}-%{release}",
        "use %{evr} instead of the entire %{?epoch:%{epoch}:}%{version}-%{release} expression; %{evr} already includes the epoch",
    ),
    (
        "%{?epoch:%{epoch}:}%version-%release",
        "use %{evr} instead of the entire %{?epoch:%{epoch}:}%version-%release expression; %{evr} already includes the epoch",
    ),
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
        EXPRESSIONS[2].1
    }
    fn check(&self, context: &Context<'_>) -> Result<Vec<Finding>> {
        self.check_tree(&context.tree, context)
    }
}

impl TreeRule for UseEvr {
    fn check_tree(&self, tree: &Tree, context: &Context<'_>) -> Result<Vec<Finding>> {
        // A bare V-R expression must not silently gain an epoch.
        let allow_plain = !context.tags.iter().any(|tag| tag.name() == "Epoch")
            && !context
                .macro_declarations
                .iter()
                .any(|name| name.split('(').next() == Some("epoch"));
        let mut findings = Vec::new();
        let mut pending = vec![tree.root_node()];
        while let Some(node) = pending.pop() {
            // Never inspect fragments inside macro bodies or recovered ERRORs.
            if node.is_error() || node.is_missing() || node.kind().starts_with("macro_") {
                continue;
            }
            if matches!(node.kind(), "preamble_tag" | "package_tag") {
                if !node.has_error()
                    && node
                        .named_child(0)
                        .is_some_and(|tag| tag.kind() == "dependency_tag")
                    && context.tag_at(node.start_position().row + 1).is_some()
                {
                    check_dependencies(self, node, context.source, allow_plain, &mut findings);
                }
                continue;
            }
            let mut cursor = node.walk();
            pending.extend(node.named_children(&mut cursor));
        }
        findings.sort_by_key(|finding| finding.span.start);
        Ok(findings)
    }
}

fn check_dependencies(
    rule: &UseEvr,
    tag: Node<'_>,
    source: &str,
    allow_plain: bool,
    findings: &mut Vec<Finding>,
) {
    let mut pending = vec![tag];
    while let Some(node) = pending.pop() {
        if matches!(node.kind(), "version_dependency" | "qualified_dependency") {
            if let Some((span, message)) = dependency_version(node, source, allow_plain) {
                let mut finding = rule.finding(span, None);
                finding.message = message.into();
                findings.push(finding);
            }
            // Do not recurse into names, qualifiers, or nested version macros.
            continue;
        }
        if node.kind().starts_with("macro_") {
            continue;
        }
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
}

fn dependency_version(
    node: Node<'_>,
    source: &str,
    allow_plain: bool,
) -> Option<(std::ops::Range<usize>, &'static str)> {
    // This grammar flattens the version field: operator, optional epoch macro,
    // version macro, hyphen, release macro. Inspect all children in that field.
    let mut cursor = node.walk();
    let parts: Vec<_> = node
        .children_by_field_name("version", &mut cursor)
        .collect();
    let (first, version, hyphen, release) = match parts.as_slice() {
        [operator, version, hyphen, release] if allow_plain && comparison(*operator) => {
            (*version, *version, *hyphen, *release)
        }
        [operator, epoch, version, hyphen, release]
            if comparison(*operator) && optional_epoch(*epoch, source) =>
        {
            (*epoch, *version, *hyphen, *release)
        }
        _ => return None,
    };
    if !scalar_macro(version, "version", source)
        || hyphen.kind() != "-"
        || !scalar_macro(release, "release", source)
    {
        return None;
    }
    // A parser can split an adjacent suffix into another dependency even on a
    // clean tree. Such a node is not a complete version expression.
    if node.next_sibling().is_some_and(|next| {
        next.start_byte() == node.end_byte() && !matches!(next.kind(), "," | ")")
    }) {
        return None;
    }
    let span = first.start_byte()..release.end_byte();
    let text = source.get(span.clone())?;
    let (_, message) = EXPRESSIONS
        .iter()
        .find(|(expression, _)| *expression == text)?;
    Some((span, message))
}

fn scalar_macro(node: Node<'_>, name: &str, source: &str) -> bool {
    let identifier_kind = match node.kind() {
        "macro_expansion" if node.child_count() == 3 => "identifier",
        "macro_simple_expansion" if node.child_count() == 2 => "simple_macro",
        _ => return false,
    };
    node.named_child_count() == 1
        && node.named_child(0).is_some_and(|identifier| {
            identifier.kind() == identifier_kind
                && identifier.utf8_text(source.as_bytes()).ok() == Some(name)
        })
}

fn optional_epoch(node: Node<'_>, source: &str) -> bool {
    if node.kind() != "macro_expansion" || node.child_count() != 3 || node.named_child_count() != 1
    {
        return false;
    }
    let Some(conditional) = node
        .named_child(0)
        .filter(|child| child.kind() == "conditional_expansion" && child.child_count() == 4)
    else {
        return false;
    };
    let Some(consequence) = conditional
        .child_by_field_name("consequence")
        .filter(|child| child.kind() == "text" && child.named_child_count() == 2)
    else {
        return false;
    };
    conditional
        .child(0)
        .is_some_and(|child| child.kind() == "?")
        && conditional
            .child_by_field_name("condition")
            .is_some_and(|child| {
                child.kind() == "identifier"
                    && child.utf8_text(source.as_bytes()).ok() == Some("epoch")
            })
        && consequence
            .named_child(0)
            .is_some_and(|child| scalar_macro(child, "epoch", source))
        && consequence.named_child(1).is_some_and(|child| {
            child.kind() == "text_content" && child.utf8_text(source.as_bytes()).ok() == Some(":")
        })
}

fn comparison(node: Node<'_>) -> bool {
    matches!(node.kind(), "=" | "<" | ">" | "<=" | ">=")
}
