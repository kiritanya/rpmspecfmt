//! Compatibility entry points backed by the same rule registry as the CLI.
use stable_eyre::eyre::{eyre, Result};

use crate::engine::{Engine, FixMode};
use crate::{rules, Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number.
    pub line: usize,
    /// 1-based character column, matching short CLI diagnostics.
    pub column: usize,
    pub rule: &'static str,
    pub message: &'static str,
}

/// Apply the conservative safe-fix profile and leave manual findings intact.
///
/// # Errors
/// Returns an error if inspection, fix validation, or convergence fails.
pub fn fix(source: &str) -> Result<String> {
    let config = Config {
        sort: false,
        wrap: false,
        reindent: false,
        ..Config::default()
    };
    Engine::new(config).apply(source, FixMode::SafeFix)
}

/// Inspect all registered rule kinds with default configuration.
///
/// This retains the original Vec-returning API. Use `lint_with_config` for
/// fallible handling of engine initialization errors. ERROR trees are normal
/// inputs and do not cause an error.
///
/// # Panics
/// Panics if the built-in engine cannot initialize or convert its diagnostics.
#[must_use]
pub fn lint(source: &str) -> Vec<Diagnostic> {
    lint_with_config(source, &Config::default()).expect("built-in lint engine could not initialize")
}

/// Configuration-aware, fallible compatibility wrapper. No detection lives
/// here: Engine dispatches the rules registered in rules/mod.rs.
///
/// # Errors
/// Returns an error if inspection fails or a rule supplies an unknown message.
pub fn lint_with_config(source: &str, config: &Config) -> Result<Vec<Diagnostic>> {
    Engine::new(config.clone())
        .check(source)?
        .into_iter()
        .map(|finding| {
            let prefix = &source[..finding.span.start];
            let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
            let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            let message = rules::message(finding.rule, &finding.message).ok_or_else(|| {
                eyre!(
                    "rule {} returned an unknown compatibility message",
                    finding.rule
                )
            })?;
            Ok(Diagnostic {
                line,
                column,
                rule: finding.rule,
                message,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_wrapper_uses_the_entire_registry() {
        let source = "Name: x\nRequires: x = %{version}-%{release}\n%define feature 1\n";
        let config = Config::default();
        let findings = Engine::new(config.clone()).check(source).unwrap();
        let diagnostics = lint_with_config(source, &config).unwrap();
        assert_eq!(
            diagnostics.iter().map(|d| d.rule).collect::<Vec<_>>(),
            findings.iter().map(|d| d.rule).collect::<Vec<_>>()
        );
        for id in [
            "format",
            "use-global",
            "macro-placement",
            "use-evr",
            "missing-license",
            "missing-changelog",
        ] {
            assert!(diagnostics.iter().any(|d| d.rule == id), "{diagnostics:?}");
        }
    }

    #[test]
    fn configuration_is_shared_with_registered_rules() {
        let source = "Name: x\nLicense: MIT\nRelease: 1%?dist\nBuildRequires: gcc cmake\n%define feature 1\n%autochangelog\n";
        let config = Config {
            normalize_macros: false,
            split_buildrequires: false,
            global_macros: false,
            ..Config::default()
        };
        let diagnostics = lint_with_config(source, &config).unwrap();
        for id in [
            "braced-macros",
            "buildrequires-one-per-line",
            "use-global",
            "macro-placement",
        ] {
            assert!(!diagnostics.iter().any(|d| d.rule == id), "{diagnostics:?}");
        }
    }

    #[test]
    // RPM macro braces are literal text, not Rust formatting placeholders.
    #[allow(clippy::literal_string_with_formatting_args)]
    fn compatibility_messages_and_unicode_columns_are_preserved() {
        let source =
            "Name: x\nLicense: MIT\nRequires: 日本語 = %version-%release\n%autochangelog\n";
        let diagnostics = lint(source);
        let evr = diagnostics.iter().find(|d| d.rule == "use-evr").unwrap();
        let expected = "use %{evr} instead of %version-%release";
        assert_eq!(evr.message, expected);
        assert_eq!((evr.line, evr.column), (3, 17));
    }
}
