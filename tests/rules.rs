use rpmspecfmt::context::Context;
use rpmspecfmt::engine::Engine;
use rpmspecfmt::rules::{
    self, BracedMacros, BuildRequiresOnePerLine, LineRule, Rule, RuleKind, TreeRule, UseEvr,
    UseGlobal,
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
fn tree_checks_accept_valid_dependencies_after_errors_without_diagnosing_body_text() {
    let config = Config::default();
    let source = "Name: x\nURL: invalid\n%package libs\nRequires: x = %{version}-%{release}\n%description libs\nRequires: text = %{version}-%{release}\n";
    let context = Context::new(source, &config).unwrap();
    assert!(context.tree.root_node().has_error());
    let findings = UseEvr.check_tree(&context.tree, &context).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(context.location(findings[0].span.start).0, 4);
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

#[test]
fn evr_recommendations_cover_the_entire_optional_epoch_expression() {
    let config = Config::default();
    for expression in [
        "%{?epoch:%{epoch}:}%{version}-%{release}",
        "%{?epoch:%{epoch}:}%version-%release",
    ] {
        let source = format!("Name: x\nEpoch: 1\nRequires: x = {expression}\n");
        let context = Context::new(&source, &config).unwrap();
        let findings = UseEvr.check_tree(&context.tree, &context).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(&source[findings[0].span.clone()], expression);
        assert!(findings[0].message.contains(expression));
        assert!(findings[0].message.contains("already includes the epoch"));
        assert!(findings[0].fix.is_none());
    }
}

#[test]
fn evr_does_not_recommend_adding_epoch_or_replacing_nested_fragments() {
    let engine = Engine::new(Config::default());
    for source in [
        "Name: x\nEpoch: 1\nRequires: x = %{version}-%{release}\n",
        "%global epoch 1\nName: x\nRequires: x = %version-%release\n",
        "Name: x\nRequires: x = %{epoch}:%{version}-%{release}\n",
        "Name: x\nRequires: x = 1:%{version}-%{release}\n",
        "Name: x\nRequires: x = %{?feature:%{version}-%{release}}\n",
        "Name: x\nRequires: x = %(echo %{version}-%{release})\n",
        "Name: x\nRequires: x = %%{version}-%{release}\n",
        "Name: x\nRequires: x = prefix-%{version}-%{release}\n",
        "Name: x\nRequires: x = %{version}-%{release}%{?suffix}\n",
        "Name: x\nRequires: x = %{?epoch:0:}%{version}-%{release}\n",
    ] {
        let findings = engine.check(source).unwrap();
        assert!(
            !findings.iter().any(|finding| finding.rule == "use-evr"),
            "{source}\n{findings:?}"
        );
    }
}

#[test]
fn plain_evr_recommendations_keep_complete_ranges_without_an_epoch() {
    let config = Config::default();
    for expression in ["%{version}-%{release}", "%version-%release"] {
        let source = format!("Name: x\nRequires: (x >= {expression} if y)\n");
        let context = Context::new(&source, &config).unwrap();
        let findings = UseEvr.check_tree(&context.tree, &context).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(&source[findings[0].span.clone()], expression);
        assert!(findings[0].fix.is_none());
    }
}

#[test]
fn evr_tree_rule_checks_qualified_version_fields_not_names_or_macro_arguments() {
    let config = Config::default();
    let source = "Name: x\nRequires(post): pkgconfig(x) = %{version}-%{release}\nProvides: capability(%{version}-%{release})\nRequires: %{version}-%{release}\nRequires: x = %{expand:%{version}-%{release}}\n";
    let context = Context::new(source, &config).unwrap();
    let findings = UseEvr.check_tree(&context.tree, &context).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(context.location(findings[0].span.start).0, 2);
    assert_eq!(&source[findings[0].span.clone()], "%{version}-%{release}");
}

#[test]
fn evr_tree_rule_skips_malformed_dependencies_without_a_text_fallback() {
    let engine = Engine::new(Config::default());
    for source in [
        "Name: x\nRequires: x = %{version}-%{release\n",
        "Name: x\nRequires: x = %{version}-%{release}suffix\n",
        "Name: x\nRequires: x = %{version}-%{release} %{?broken:\n",
    ] {
        assert!(
            !engine
                .check(source)
                .unwrap()
                .iter()
                .any(|finding| finding.rule == "use-evr"),
            "{source}"
        );
    }
}

#[test]
fn engine_checks_all_rule_kinds_and_respects_disabled_rules() {
    let source = "Name: x\nRequires: x = %{version}-%{release}\n%define feature 1\n";
    let findings = Engine::new(Config::default()).check(source).unwrap();
    for id in [
        "format",
        "use-global",
        "macro-placement",
        "use-evr",
        "missing-license",
        "missing-changelog",
    ] {
        assert!(
            findings.iter().any(|finding| finding.rule == id),
            "{findings:?}"
        );
    }
    let config = Config {
        normalize_macros: false,
        split_buildrequires: false,
        global_macros: false,
        ..Config::default()
    };
    let findings = Engine::new(config).check("Name: x\nLicense: MIT\nRelease: 1%?dist\nBuildRequires: gcc cmake\n%define feature 1\n%autochangelog\n").unwrap();
    for id in [
        "braced-macros",
        "buildrequires-one-per-line",
        "use-global",
        "macro-placement",
    ] {
        assert!(
            !findings.iter().any(|finding| finding.rule == id),
            "{findings:?}"
        );
    }
}
