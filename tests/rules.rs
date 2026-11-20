use rpmspecfmt::context::Context;
use rpmspecfmt::engine::Engine;
use rpmspecfmt::rules::{
    self, BracedMacros, BuildRequiresOnePerLine, LineRule, Rule, RuleKind, UseEvr, UseGlobal,
};
use rpmspecfmt::Config;

#[test]
fn registered_rules_have_unique_ids_and_three_kinds() {
    let rules = rules::builtins();
    let mut ids = std::collections::HashSet::new();
    for rule in &rules {
        assert!(ids.insert(rule.id()), "duplicate rule: {}", rule.id());
    }
    assert_eq!(rules.len(), 14);
    for (kind, count) in [
        (RuleKind::Format, 6),
        (RuleKind::Fixable, 3),
        (RuleKind::CheckOnly, 5),
    ] {
        assert_eq!(
            rules.iter().filter(|rule| rule.kind() == kind).count(),
            count
        );
    }
}

#[test]
fn line_rules_detect_problems_directly_from_source() {
    let config = Config::default();
    let source = "Name: x\nLicense: MIT\nRelease: 1%?dist\nBuildRequires: gcc >=1.2 cmake\n%define feature 1\n%autochangelog\n";
    let context = Context::new(source, &config).unwrap();
    let braced = BracedMacros
        .check_line(&context.lines[2], &context)
        .unwrap();
    assert_eq!(braced[0].rule, "braced-macros");
    assert_eq!(&source[braced[0].span.clone()], "%?dist");
    assert!(braced[0].fix.is_some());
    assert_eq!(
        BuildRequiresOnePerLine
            .check_line(&context.lines[3], &context)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        UseGlobal
            .check_line(&context.lines[4], &context)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn text_checks_survive_parser_errors_without_diagnosing_body_text() {
    let config = Config::default();
    let source = "Name: x\nURL: invalid\nRequires: x = %{version}-%{release}\n%description\nRequires: text = %{version}-%{release}\n";
    let context = Context::new(source, &config).unwrap();
    assert!(context.tree.root_node().has_error());
    assert_eq!(
        UseEvr
            .check_line(&context.lines[2], &context)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        UseEvr.check_line(&context.lines[4], &context).unwrap(),
        [] as [rpmspecfmt::diagnostic::Finding; 0]
    );
}

#[test]
fn alignment_findings_group_lines_and_blank_gaps_but_not_conditionals_or_correct_tags() {
    let source = "%bcond feature 1\nName:       日本語\nVersion:    1\n\nLicense:    MIT\nURL:     https://example.com\nSummary:    example\n%if 1\nRelease:    1\n%endif\n%autochangelog\n";
    let findings = Engine::new(Config::default()).check(source).unwrap();
    let alignment: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule == "tag-alignment")
        .collect();
    assert_eq!(alignment.len(), 3, "{alignment:?}");
    assert_eq!(
        &source[alignment[0].span.clone()],
        "Name:       日本語\nVersion:    1\n\nLicense:    MIT"
    );
    assert_eq!(&source[alignment[1].span.clone()], "Summary:    example");
    assert_eq!(&source[alignment[2].span.clone()], "Release:    1");
    assert!(alignment.iter().all(|finding| finding.fix.is_none()));
}

#[test]
fn repeated_macro_findings_share_a_block_and_keep_the_fix() {
    let source = "Name: x\nLicense: MIT\nSource0: %version\nSource1: %release\n%autochangelog\n";
    let engine = Engine::new(Config::default());
    let findings = engine.check(source).unwrap();
    let macros: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule == "braced-macros")
        .collect();
    assert_eq!(macros.len(), 1);
    assert_eq!(
        &source[macros[0].span.clone()],
        "Source0: %version\nSource1: %release"
    );
    assert!(macros[0].fix.is_some());
    let fixed = engine
        .apply(source, rpmspecfmt::engine::FixMode::FormatOnly)
        .unwrap();
    assert!(fixed.contains("%{version}") && fixed.contains("%{release}"));
}
