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
    assert!(result.stdout.is_empty());
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
    assert!(quiet.stderr.is_empty());
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
