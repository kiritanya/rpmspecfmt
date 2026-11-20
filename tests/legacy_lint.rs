//! Original semantic regressions. These intentionally exclude the new aggregate
//! formatting finding; complete-registry parity is tested in src/lint.rs.
use rpmspecfmt::lint::Diagnostic;

fn lint(source: &str) -> Vec<Diagnostic> {
    rpmspecfmt::lint::lint(source)
        .into_iter()
        .filter(|d| d.rule != "format")
        .collect()
}

fn rules(source: &str) -> Vec<&'static str> {
    lint(source).into_iter().map(|d| d.rule).collect()
}

#[test]
fn detects_missing_changelog_and_license() {
    let found = rules("Name: test\nVersion: 1\nRelease: 1\nSummary: test\n");
    assert!(found.contains(&"missing-changelog") && found.contains(&"missing-license"));
}

#[test]
fn accepts_autochangelog_and_license() {
    assert!(rules(
        "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%autochangelog\n"
    )
    .is_empty());
}

#[test]
fn suggests_evr_for_dependency_tags() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\nRequires: foo = %{version}-%{release}\n\n%changelog\n";
    assert!(lint(source)
        .iter()
        .any(|d| d.rule == "use-evr" && d.line == 6));
}

#[test]
fn does_not_suggest_evr_for_sources() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\nSource0: %{name}-%{version}-%{release}.tar.gz\n\n%changelog\n";
    assert!(!rules(source).contains(&"use-evr"));
}

#[test]
fn suggests_autosetup() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%prep\n%setup -q\n\n%changelog\n";
    assert!(lint(source)
        .iter()
        .any(|d| d.rule == "use-autosetup" && d.line == 8));
}

#[test]
fn skips_extra_setup_lines() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%prep\n%setup -q -T -D -a 1\n%setup -q -T -D -a 2\n\n%changelog\n";
    assert!(!rules(source).contains(&"use-autosetup"));
}

#[test]
fn reports_setup_once() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%prep\n%setup -q\n%setup -q\n\n%changelog\n";
    assert_eq!(
        lint(source)
            .iter()
            .filter(|d| d.rule == "use-autosetup")
            .count(),
        1
    );
}

#[test]
fn clean_spec_has_no_semantic_diagnostics() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%prep\n%autosetup\n\n%changelog\n* Sat Nov 07 2026 Tester <t@example.com> - 1\n- Initial\n";
    assert!(lint(source).is_empty());
}

#[test]
fn ignores_tag_like_text_in_sections() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%description\nRequires: nope = %{version}-%{release}\n\n%changelog\n";
    assert!(!rules(source).contains(&"use-evr"));
}

#[test]
fn finds_tags_after_parse_errors() {
    let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nURL: example.com\nLicense: MIT\nRequires: foo = %{version}-%{release}\n\n%changelog\n";
    let found = lint(source);
    assert!(found.iter().any(|d| d.rule == "use-evr" && d.line == 7));
    assert!(!found.iter().any(|d| d.rule == "missing-license"));
}

#[test]
fn reports_new_style_rules_with_original_lines() {
    let source = "Name: x\nLicense: MIT\n%gometa\nBuildRequires: pkgconfig(foo) gcc >= 1\n%define feature 1\nRelease: 1%?dist\n\n%autochangelog\n";
    let found = lint(source);
    for (rule, line) in [
        ("unsafe-preamble", 3),
        ("buildrequires-one-per-line", 4),
        ("use-global", 5),
        ("macro-placement", 5),
        ("braced-macros", 6),
    ] {
        assert!(
            found.iter().any(|d| d.rule == rule && d.line == line),
            "{found:?}"
        );
    }
}

#[test]
fn accepts_one_rich_or_versioned_dependency() {
    let source = "Name: x\nLicense: MIT\nBuildRequires: (foo if bar)\nBuildRequires: pkgconfig(foo) >= 1\n%autochangelog\n";
    assert!(lint(source).is_empty());
}

#[test]
fn lints_group_layout_when_a_macro_prevents_formatting() {
    let found = rules("Name: x\nVersion: 1\n%gometa\nBuildRequires: gcc\nSource0: x\nLicense: MIT\n%autochangelog\n");
    for rule in ["tag-alignment", "tag-group-order", "tag-group-spacing"] {
        assert!(found.contains(&rule), "{found:?}");
    }
}
