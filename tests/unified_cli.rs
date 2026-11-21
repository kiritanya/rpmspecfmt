use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = if std::path::Path::new("/tmp/opencode").is_dir() {
            PathBuf::from("/tmp/opencode")
        } else {
            std::env::temp_dir()
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = root.join(format!(
            "rpmspecfmt-unified-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, source).unwrap();
        path
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rpmspecfmt"));
        command
            .current_dir(&self.0)
            .env("RUST_LOG", "off")
            .env("NO_COLOR", "1");
        command
    }

    fn run(&self, args: &[&str], input: Option<&str>) -> Output {
        let mut command = self.command();
        command.args(args);
        if let Some(source) = input {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        } else {
            command.output().unwrap()
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn lint_and_fix_share_one_cli_and_recheck_fixed_source() {
    let fixture = Fixture::new();
    let path = fixture.write(
        "input.spec",
        "Name: x\nLicense: MIT\nRelease: 1%?dist\n%define feature 1\n%autochangelog\n",
    );
    let original = std::fs::read_to_string(&path).unwrap();
    let result = fixture.run(
        &["--lint", "--diagnostic-format", "short", "input.spec"],
        None,
    );
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    let output = String::from_utf8(result.stdout).unwrap();
    for id in ["format", "braced-macros", "use-global", "macro-placement"] {
        assert!(output.contains(&format!("[{id}]")), "{output}");
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let result = fixture.run(&["--lint", "--fix", "input.spec"], None);
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    assert!(
        result.stdout.is_empty() && result.stderr.is_empty(),
        "{result:?}"
    );
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .starts_with("%global feature 1\n"));
}

#[test]
fn normal_formatting_leaves_fixable_changes_for_lint_fix() {
    let fixture = Fixture::new();
    fixture.write(
        "input.spec",
        "Name: x\nLicense: MIT\nRelease: 1%?dist\n%define feature 1\n%autochangelog\n",
    );
    let result = fixture.run(&["input.spec"], None);
    assert!(result.status.success(), "{result:?}");
    let formatted = String::from_utf8(result.stdout).unwrap();
    assert!(formatted.contains("%define feature 1"));
    assert!(formatted.contains("1%{?dist}"));
    let result = fixture.run(&["--check", "input.spec"], None);
    assert_eq!(result.status.code(), Some(1));
    fixture.run(&["-w", "input.spec"], None);
    assert_eq!(
        fixture.run(&["--check", "input.spec"], None).status.code(),
        Some(0)
    );
}

#[test]
fn ariadne_reports_unicode_source_and_precise_macro_labels() {
    let fixture = Fixture::new();
    let source = "Name: 日本語\nLicense: MIT\nRelease: 1%?dist\n%autochangelog\n";
    let result = fixture.run(&["--lint", "--color", "never"], Some(source));
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert_eq!(result.stdout, b"");
    let output = String::from_utf8(result.stderr).unwrap();
    assert!(output.contains("日本語"), "{output}");
    assert!(output.contains("[braced-macros]"), "{output}");
    assert!(output.contains("%?dist"), "{output}");
    assert!(!output.contains('\u{1b}'), "{output}");
    let short = fixture.run(&["--lint", "--diagnostic-format", "short"], Some(source));
    assert!(String::from_utf8(short.stdout)
        .unwrap()
        .contains("<stdin>:3:11:"));
}

#[test]
fn config_priority_is_toml_then_env_then_cli() {
    let fixture = Fixture::new();
    fixture.write("input.spec", "Name: x\n");
    fixture.write(".rpmspecfmt.toml", "tag_column = 12\n");
    let file = fixture.run(&["input.spec"], None);
    assert_eq!(
        String::from_utf8(file.stdout).unwrap(),
        format!("Name:{}x\n", " ".repeat(7))
    );
    let env = fixture
        .command()
        .env("RPMSPECFMT_TAG_COLUMN", "22")
        .arg("input.spec")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(env.stdout).unwrap(),
        format!("Name:{}x\n", " ".repeat(17))
    );
    let cli = fixture
        .command()
        .env("RPMSPECFMT_TAG_COLUMN", "22")
        .args(["--tag-column", "18", "input.spec"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(cli.stdout).unwrap(),
        format!("Name:{}x\n", " ".repeat(13))
    );
}

#[test]
fn invalid_config_or_flag_combinations_do_not_write() {
    let fixture = Fixture::new();
    let source = "Name: x\n";
    let path = fixture.write("input.spec", source);
    for args in [
        vec!["--fix", "input.spec"],
        vec!["--lint", "-w", "input.spec"],
        vec!["--lint", "--diff", "input.spec"],
    ] {
        assert_eq!(fixture.run(&args, None).status.code(), Some(2));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
    fixture.write("broken.toml", "unknown_setting = true\n");
    let result = fixture.run(&["-w", "--config", "broken.toml", "input.spec"], None);
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn lint_fix_rejects_stdin_and_preserves_manual_findings() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture
            .run(&["--lint", "--fix"], Some("Name: x\n"))
            .status
            .code(),
        Some(2)
    );
    let path = fixture.write(
        "input.spec",
        "Name: x\nLicense: MIT\nRequires: x = %{version}-%{release}\n%autochangelog\n",
    );
    let result = fixture.run(
        &[
            "--lint",
            "--fix",
            "--diagnostic-format",
            "short",
            "input.spec",
        ],
        None,
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("[use-evr]"));
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("%{version}-%{release}"));
}

#[test]
fn tracing_goes_to_stderr_and_respects_verbosity() {
    let fixture = Fixture::new();
    fixture.write("input.spec", "Name: x\n");
    let result = fixture
        .command()
        .env_remove("RUST_LOG")
        .args(["-vv", "input.spec"])
        .output()
        .unwrap();
    assert!(String::from_utf8(result.stderr).unwrap().contains("DEBUG"));
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "Name: x\n");
    let quiet = fixture
        .command()
        .env_remove("RUST_LOG")
        .args(["-q", "input.spec"])
        .output()
        .unwrap();
    assert_eq!(quiet.stderr, b"");
    let lint = fixture
        .command()
        .env_remove("RUST_LOG")
        .args(["-q", "--lint", "--diagnostic-format", "short", "input.spec"])
        .output()
        .unwrap();
    assert_eq!(lint.status.code(), Some(1));
    assert!(String::from_utf8(lint.stdout)
        .unwrap()
        .contains("[missing-license]"));
}

#[test]
fn repeated_alignment_warnings_render_as_one_multiline_report() {
    let fixture = Fixture::new();
    let source = "%bcond feature 1\nName:      日本語\nVersion:   1\nRelease:   1\nLicense:   MIT\nSummary:   example\n%autochangelog\n";
    let rich = fixture.run(&["--lint", "--color", "never"], Some(source));
    assert_eq!(rich.status.code(), Some(1));
    let output = String::from_utf8(rich.stderr).unwrap();
    assert_eq!(output.matches("[tag-alignment]").count(), 1, "{output}");
    assert!(
        output.contains("日本語") && output.contains("Summary:"),
        "{output}"
    );
    let short = fixture.run(&["--lint", "--diagnostic-format", "short"], Some(source));
    let output = String::from_utf8(short.stdout).unwrap();
    assert_eq!(output.matches("[tag-alignment]").count(), 1, "{output}");
    assert!(
        output
            .lines()
            .any(|line| line.starts_with("<stdin>:2:1:") && line.ends_with("[tag-alignment]")),
        "{output}"
    );
}

#[test]
fn sarif_collects_multiple_files_and_grouped_ranges_in_one_document() {
    let fixture = Fixture::new();
    let source =
        "%bcond feature 1\nName:      日本😀\nVersion:   1\nLicense:   MIT\n%autochangelog\n";
    let path = fixture.write("日本 #?.spec", source);
    fixture.write("second.spec", "Name: x\n");
    let output = fixture.run(
        &[
            "--lint",
            "--diagnostic-format",
            "sarif",
            "--color",
            "always",
            "日本 #?.spec",
            "second.spec",
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(output.stderr, b"");
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(document["version"], "2.1.0");
    let runs = document["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run["tool"]["driver"]["name"], "rpmspecfmt");
    assert_eq!(run["columnKind"], "unicodeCodePoints");
    let rules = run["tool"]["driver"]["rules"].as_array().unwrap();
    assert_eq!(rules.len(), rpmspecfmt::rules::builtins().len());
    let results = run["results"].as_array().unwrap();
    for result in results {
        assert_eq!(
            rules[usize::try_from(result["ruleIndex"].as_u64().unwrap()).unwrap()]["id"],
            result["ruleId"]
        );
        assert_eq!(result["level"], "warning");
    }
    let alignment = results
        .iter()
        .find(|result| result["ruleId"] == "tag-alignment")
        .unwrap();
    let physical = &alignment["locations"][0]["physicalLocation"];
    assert_eq!(
        physical["artifactLocation"]["uri"],
        "%E6%97%A5%E6%9C%AC%20%23%3F.spec"
    );
    assert_eq!(
        physical["region"],
        serde_json::json!({ "startLine": 2, "startColumn": 1, "endLine": 4, "endColumn": 15 })
    );
    assert!(results
        .iter()
        .any(
            |result| result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
                == "second.spec"
        ));
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[test]
fn sarif_unicode_crlf_regions_and_fix_replacements_match_the_input() {
    let fixture = Fixture::new();
    let source = "Name: 日本😀\r\nLicense: MIT\r\nSource0: 日本😀/%version\r\n%autochangelog\r\n";
    let output = fixture.run(&["--lint", "--diagnostic-format", "sarif"], Some(source));
    assert_eq!(output.status.code(), Some(1));
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = document["runs"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["ruleId"] == "braced-macros")
        .unwrap();
    assert_eq!(
        result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "%3Cstdin%3E"
    );
    assert_eq!(
        result["locations"][0]["physicalLocation"]["region"],
        serde_json::json!({ "startLine": 3, "startColumn": 14, "endLine": 3, "endColumn": 22 })
    );
    let replacement = &result["fixes"][0]["artifactChanges"][0]["replacements"][0];
    assert_eq!(
        replacement["deletedRegion"],
        serde_json::json!({ "startLine": 1, "startColumn": 1, "endLine": 5, "endColumn": 1 })
    );
    assert!(replacement["insertedContent"]["text"]
        .as_str()
        .unwrap()
        .contains("%{version}"));
}

#[test]
fn sarif_empty_results_are_emitted_for_clean_files_and_empty_stdin() {
    let fixture = Fixture::new();
    let clean = rpmspecfmt::engine::Engine::new(rpmspecfmt::Config::default())
        .apply(
            "Name: x\nLicense: MIT\n%autochangelog\n",
            rpmspecfmt::engine::FixMode::FormatOnly,
        )
        .unwrap();
    fixture.write("clean.spec", &clean);
    for output in [
        fixture.run(
            &["--lint", "--diagnostic-format", "sarif", "clean.spec"],
            None,
        ),
        fixture.run(&["--lint", "--diagnostic-format", "sarif"], Some("")),
    ] {
        assert!(output.status.success(), "{output:?}");
        let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(document["runs"][0]["results"], serde_json::json!([]));
    }
}

#[test]
fn sarif_fix_reports_positions_in_the_fixed_revision() {
    let fixture = Fixture::new();
    let source = "Name: x\nLicense: MIT\n%define feature 1\nRequires: x = %{version}-%{release}\n%autochangelog\n";
    let path = fixture.write("input.spec", source);
    let output = fixture.run(
        &[
            "--lint",
            "--fix",
            "--diagnostic-format",
            "sarif",
            "input.spec",
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    let fixed = std::fs::read_to_string(path).unwrap();
    assert_ne!(fixed, source);
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = document["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result["ruleId"], "use-evr");
    let offset = fixed.find("%{version}").unwrap();
    let expected_line = fixed[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    assert_eq!(
        result["locations"][0]["physicalLocation"]["region"]["startLine"],
        expected_line
    );
    assert!(result.get("fixes").is_none());
    let second = fixture.run(
        &[
            "--lint",
            "--fix",
            "--diagnostic-format",
            "sarif",
            "input.spec",
        ],
        None,
    );
    assert_eq!(output.stdout, second.stdout);
}

#[test]
fn sarif_requires_lint_and_does_not_emit_partial_documents_on_input_errors() {
    let fixture = Fixture::new();
    let source = "Name: x\n";
    let path = fixture.write("input.spec", source);
    for args in [
        vec!["--diagnostic-format", "sarif", "-w", "input.spec"],
        vec![
            "--lint",
            "--diagnostic-format",
            "sarif",
            "input.spec",
            "missing.spec",
        ],
    ] {
        let output = fixture.run(&args, None);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert_ne!(output.stderr, b"");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn lint_alias_accepts_explicit_sarif_output() {
    let fixture = Fixture::new();
    fixture.write("input.spec", "Name: x\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rpmspeclint"))
        .current_dir(&fixture.0)
        .args(["--diagnostic-format=sarif", "input.spec"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(document["runs"][0]["tool"]["driver"]["name"], "rpmspecfmt");
}

#[test]
fn lint_alias_matches_unified_cli_defaults_formats_and_fixes() {
    let fixture = Fixture::new();
    let source = "Name: x\nLicense: MIT\n%define feature 1\nRequires: x = %{version}-%{release}\n%autochangelog\n";
    let path = fixture.write("input.spec", source);
    for format in [None, Some("rich"), Some("short"), Some("sarif")] {
        for fix in [false, true] {
            std::fs::write(&path, source).unwrap();
            let mut args = vec!["--lint", "--color", "never"];
            if let Some(format) = format {
                args.extend(["--diagnostic-format", format]);
            }
            if fix {
                args.push("--fix");
            }
            args.push("input.spec");
            let expected = fixture.run(&args, None);
            let expected_source = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, source).unwrap();
            let actual = Command::new(env!("CARGO_BIN_EXE_rpmspeclint"))
                .current_dir(&fixture.0)
                .env("RUST_LOG", "off")
                .env("NO_COLOR", "1")
                .args(&args[1..])
                .output()
                .unwrap();
            assert_eq!(actual.status.code(), expected.status.code());
            assert_eq!(actual.stdout, expected.stdout);
            assert_eq!(actual.stderr, expected.stderr);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected_source);
        }
    }
}

#[test]
fn short_output_reports_bare_evr_messages_with_unicode_columns() {
    let fixture = Fixture::new();
    let source = "Name: x\nLicense: MIT\nRequires: 日本語 = %version-%release\n%autochangelog\n";
    let output = fixture.run(&["--lint", "--diagnostic-format", "short"], Some(source));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.lines()
            .any(|line| line.starts_with("<stdin>:3:17:") && line.ends_with("[use-evr]")),
        "{text}"
    );
    assert!(text.contains("use %{evr} instead of %version-%release"));
}

#[test]
fn sarif_keeps_tracing_and_colors_off_stdout() {
    let fixture = Fixture::new();
    fixture.write("input.spec", "Name: x\n");
    let output = fixture
        .command()
        .env_remove("RUST_LOG")
        .args([
            "--lint",
            "--diagnostic-format",
            "sarif",
            "--color",
            "always",
            "-vv",
            "input.spec",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(document["version"], "2.1.0");
    assert!(!output.stdout.contains(&0x1b));
    assert!(String::from_utf8(output.stderr).unwrap().contains("DEBUG"));
}

#[test]
fn epoch_aware_evr_warning_is_explicit_in_rich_and_sarif_output() {
    let fixture = Fixture::new();
    let expression = "%{?epoch:%{epoch}:}%{version}-%{release}";
    let source = format!("Name: x\nLicense: MIT\nEpoch: 1\nProvides: x = {expression}\nRequires: y = {expression}\n%autochangelog\n");
    let rich = fixture.run(&["--lint", "--color", "never"], Some(&source));
    let text = String::from_utf8(rich.stderr).unwrap();
    assert!(text.contains("already includes the epoch"), "{text}");
    assert!(
        text.contains(&format!("entire {expression} expression")),
        "{text}"
    );
    let sarif = fixture.run(&["--lint", "--diagnostic-format", "sarif"], Some(&source));
    let document: serde_json::Value = serde_json::from_slice(&sarif.stdout).unwrap();
    let result = document["runs"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["ruleId"] == "use-evr")
        .unwrap();
    assert!(result["message"]["text"]
        .as_str()
        .unwrap()
        .contains(&format!("entire {expression} expression")));
    assert!(result.get("fixes").is_none());
}
