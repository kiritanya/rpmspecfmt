/// A single lint finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
    /// Stable rule identifier.
    pub rule: &'static str,
    /// Short human readable message.
    pub message: &'static str,
}

const DEPENDENCY_TAGS: &[&str] = &[
    "BuildRequires",
    "Requires",
    "Provides",
    "Conflicts",
    "Obsoletes",
    "Recommends",
    "Suggests",
    "Supplements",
    "Enhances",
    "BuildConflicts",
    "OrderWithRequires",
];

/// Sections whose bodies are not tag lists.
const BODY_SECTIONS: &[&str] = &[
    "description",
    "files",
    "changelog",
    "prep",
    "build",
    "install",
    "check",
    "clean",
    "pre",
    "post",
    "preun",
    "postun",
    "pretrans",
    "posttrans",
    "preuntrans",
    "postuntrans",
    "trigger",
    "triggerin",
    "triggerun",
    "triggerpostun",
    "filetrigger",
    "filetriggerin",
    "filetriggerun",
    "filetriggerpostun",
    "transfiletrigger",
    "transfiletriggerin",
    "transfiletriggerun",
    "transfiletriggerpostun",
    "generate_buildrequires",
    "sourcelist",
    "patchlist",
    "verifyscript",
    "sepolicy",
];

const EVR_PATTERNS: &[(&str, &str)] = &[
    (
        "%{version}-%{release}",
        "use %{evr} instead of %{version}-%{release}",
    ),
    (
        "%version-%release",
        "use %{evr} instead of %version-%release",
    ),
];

/// Lint a spec file.
///
/// This is a text-based linter, so it still works when the grammar reports
/// errors for parts of the file.
pub fn lint(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if source.is_empty() {
        return diagnostics;
    }

    let mut in_tags = true;
    let mut has_license = false;
    let mut has_changelog = false;
    let mut reported_setup = false;
    let lines: Vec<&str> = source.lines().collect();
    let macro_names = crate::style::macro_names(&lines);
    let preamble_end = crate::style::preamble_end(&lines);
    let name_line = lines[..preamble_end]
        .iter()
        .position(|line| split_tag_line(line.trim_start()).is_some_and(|(tag, _, _)| tag == "Name"))
        .unwrap_or(preamble_end);
    if let Some(index) = crate::style::unsafe_preamble(&lines) {
        diagnostics.push(Diagnostic {
            line: index + 1,
            column: 1,
            rule: "unsafe-preamble",
            message: "preamble contains a macro or conditional; group and align tags manually (automatic formatting is disabled)",
        });
        lint_preamble_layout(&lines[..preamble_end], &mut diagnostics);
    }

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        if !trimmed.starts_with('#')
            && crate::style::brace_macros_with_names(line, &macro_names) != line
        {
            diagnostics.push(Diagnostic {
                line: line_number,
                column: indent + 1,
                rule: "braced-macros",
                message: "use braced macro references, such as %{_bindir} and %{?dist}",
            });
        }
        if let Some((directive, _, _)) = crate::style::definition(line) {
            if directive == "define" {
                diagnostics.push(Diagnostic {
                    line: line_number,
                    column: indent + 1,
                    rule: "use-global",
                    message: "prefer %global to %define; check expansion timing before converting",
                });
            }
            if index > name_line {
                diagnostics.push(Diagnostic {
                    line: line_number, column: indent + 1,
                    rule: "macro-placement", message: "place macro definitions before Name; check scope and expansion order before moving",
                });
            }
        }

        if trimmed.starts_with('%') {
            let section = section_word(trimmed);
            match section {
                "package" => in_tags = true,
                "changelog" | "autochangelog" => {
                    has_changelog = true;
                    in_tags = false;
                }
                "setup" => {
                    // Extra `%setup -T -D -a` lines unpack more sources and
                    // cannot simply be replaced with one `%autosetup`.
                    if !reported_setup && !is_extra_setup(trimmed) {
                        diagnostics.push(Diagnostic {
                            line: line_number,
                            column: indent + 1,
                            rule: "use-autosetup",
                            message: "use %autosetup instead of %setup",
                        });
                        reported_setup = true;
                    }
                }
                _ if BODY_SECTIONS.contains(&section) => in_tags = false,
                _ => {}
            }
            continue;
        }

        if !in_tags {
            continue;
        }

        let Some((tag, value, value_offset)) = split_tag_line(trimmed) else {
            continue;
        };
        let tag = tag_name(tag);

        if tag == "BuildRequires"
            && crate::style::dependencies(value).is_some_and(|deps| deps.len() > 1)
        {
            diagnostics.push(Diagnostic {
                line: line_number,
                column: indent + value_offset + 1,
                rule: "buildrequires-one-per-line",
                message: "put each BuildRequires dependency expression on its own line",
            });
        }

        if tag == "License" {
            has_license = true;
        }

        if DEPENDENCY_TAGS.contains(&tag) {
            if let Some((offset, message)) = find_evr(value) {
                diagnostics.push(Diagnostic {
                    line: line_number,
                    column: indent + value_offset + offset + 1,
                    rule: "use-evr",
                    message,
                });
            }
        }
    }

    if !has_changelog {
        diagnostics.push(Diagnostic {
            line: 1,
            column: 1,
            rule: "missing-changelog",
            message: "no %changelog or %autochangelog section",
        });
    }
    if !has_license {
        diagnostics.push(Diagnostic {
            line: 1,
            column: 1,
            rule: "missing-license",
            message: "no License tag",
        });
    }

    diagnostics.sort_by(|a, b| (a.line, a.column, a.rule).cmp(&(b.line, b.column, b.rule)));
    diagnostics.dedup();
    diagnostics
}

fn lint_preamble_layout(lines: &[&str], diagnostics: &mut Vec<Diagnostic>) {
    let tags: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| crate::format::split_tag(line).map(|(tag, value)| (i, tag, value)))
        .collect();
    let mut previous = None;
    for (index, tag, value) in &tags {
        let order = crate::style::tag_order(tag);
        let column = tags
            .iter()
            .filter(|(_, t, _)| crate::style::tag_order(t).0 == order.0)
            .map(|(_, t, _)| t.len() + 1)
            .max()
            .unwrap();
        if !value.is_empty()
            && lines[*index] != format!("{tag}{}{value}", " ".repeat(column - tag.len()))
        {
            diagnostics.push(Diagnostic {
                line: index + 1,
                column: 1,
                rule: "tag-alignment",
                message: "align values within each tag group at the longest tag plus one space",
            });
        }
        if let Some((previous_index, previous_order)) = previous {
            if order < previous_order {
                diagnostics.push(Diagnostic { line: index + 1, column: 1,
                    rule: "tag-group-order", message: "group metadata, sources/patches, dependencies, BuildRequires, and build options in that order" });
            }
            if order.0 != previous_order.0 && *index == previous_index + 1 {
                diagnostics.push(Diagnostic {
                    line: index + 1,
                    column: 1,
                    rule: "tag-group-spacing",
                    message: "separate tag groups with a blank line",
                });
            }
        }
        previous = Some((*index, order));
    }
}

fn is_extra_setup(line: &str) -> bool {
    line.split_whitespace()
        .skip(1)
        .any(|arg| matches!(arg, "-T" | "-D" | "-a" | "-b"))
}

fn section_word(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('%') else {
        return "";
    };
    if !rest.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return "";
    }

    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Split `Name: value` into tag, value and the value offset within `line`.
fn split_tag_line(line: &str) -> Option<(&str, &str, usize)> {
    let bytes = line.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }

    let mut index = 0;
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
        index += 1;
    }

    if index < bytes.len() && bytes[index] == b'(' {
        let mut end = index + 1;
        while end < bytes.len() && bytes[end] != b')' {
            end += 1;
        }
        if end >= bytes.len() {
            return None;
        }
        index = end + 1;
    }

    if index >= bytes.len() || bytes[index] != b':' {
        return None;
    }

    let rest = &line[index + 1..];
    let value = rest.trim_start();
    let value_offset = index + 1 + (rest.len() - value.len());
    Some((&line[..index], value.trim_end(), value_offset))
}

fn tag_name(tag: &str) -> &str {
    tag.split('(').next().unwrap_or(tag)
}

fn find_evr(value: &str) -> Option<(usize, &'static str)> {
    let mut best: Option<(usize, &'static str)> = None;
    for &(pattern, message) in EVR_PATTERNS {
        if let Some(offset) = value.find(pattern) {
            if best.is_none_or(|(best_offset, _)| offset < best_offset) {
                best = Some((offset, message));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(source: &str) -> Vec<&'static str> {
        lint(source)
            .into_iter()
            .map(|diagnostic| diagnostic.rule)
            .collect()
    }

    #[test]
    fn detects_missing_changelog_and_license() {
        let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\n";
        let rules = rules(source);
        assert!(rules.contains(&"missing-changelog"), "{rules:?}");
        assert!(rules.contains(&"missing-license"), "{rules:?}");
    }

    #[test]
    fn accepts_autochangelog_and_license() {
        let source =
            "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%autochangelog\n";
        assert!(rules(source).is_empty());
    }

    #[test]
    fn suggests_evr_for_dependency_tags() {
        let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\nRequires: foo = %{version}-%{release}\n\n%changelog\n";
        let diagnostics = lint(source);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "use-evr" && diagnostic.line == 6),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn does_not_suggest_evr_for_sources() {
        let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\nSource0: %{name}-%{version}-%{release}.tar.gz\n\n%changelog\n";
        assert!(!rules(source).contains(&"use-evr"));
    }

    #[test]
    fn suggests_autosetup() {
        let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%prep\n%setup -q\n\n%changelog\n";
        let diagnostics = lint(source);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "use-autosetup" && diagnostic.line == 8),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn skips_extra_setup_lines() {
        let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%prep\n%setup -q -T -D -a 1\n%setup -q -T -D -a 2\n\n%changelog\n";
        assert!(!rules(source).contains(&"use-autosetup"));
    }

    #[test]
    fn reports_setup_once() {
        let source = "Name: test\nVersion: 1\nRelease: 1\nSummary: test\nLicense: MIT\n\n%prep\n%setup -q\n%setup -q\n\n%changelog\n";
        let count = lint(source)
            .iter()
            .filter(|diagnostic| diagnostic.rule == "use-autosetup")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn clean_spec_has_no_diagnostics() {
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
        let diagnostics = lint(source);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "use-evr" && diagnostic.line == 7),
            "{diagnostics:?}"
        );
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "missing-license"));
    }

    #[test]
    fn reports_new_style_rules_with_original_lines() {
        let src = "Name: x\nLicense: MIT\n%gometa\nBuildRequires: pkgconfig(foo) gcc >= 1\n%define feature 1\nRelease: 1%?dist\n\n%autochangelog\n";
        let diagnostics = lint(src);
        for (rule, line) in [
            ("unsafe-preamble", 3),
            ("buildrequires-one-per-line", 4),
            ("use-global", 5),
            ("macro-placement", 5),
            ("braced-macros", 6),
        ] {
            assert!(
                diagnostics.iter().any(|d| d.rule == rule && d.line == line),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn accepts_one_rich_or_versioned_dependency() {
        let src = "Name: x\nLicense: MIT\nBuildRequires: (foo if bar)\nBuildRequires: pkgconfig(foo) >= 1\n%autochangelog\n";
        assert!(lint(src).is_empty(), "{:?}", lint(src));
    }

    #[test]
    fn lints_group_layout_when_a_macro_prevents_formatting() {
        let src = "Name: x\nVersion: 1\n%gometa\nBuildRequires: gcc\nSource0: x\nLicense: MIT\n%autochangelog\n";
        let found = rules(src);
        for rule in ["tag-alignment", "tag-group-order", "tag-group-spacing"] {
            assert!(found.contains(&rule), "{found:?}");
        }
    }
}
