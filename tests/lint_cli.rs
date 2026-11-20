use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct SpecDir(PathBuf);

impl SpecDir {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let root = if Path::new("/tmp/opencode").is_dir() {
            PathBuf::from("/tmp/opencode")
        } else {
            std::env::temp_dir()
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = root.join(format!(
            "rpmspecfmt-lint-test-{}-{stamp}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn spec(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, source).unwrap();
        path
    }
}

impl Drop for SpecDir {
    fn drop(&mut self) {
        // Only remove files and the unique directory created by this test.
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

fn run(fix: bool, paths: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rpmspeclint"));
    if fix {
        command.arg("--fix");
    }
    command.args(paths).output().unwrap()
}

#[test]
fn fixes_safe_findings_and_is_idempotent() {
    let dir = SpecDir::new();
    let path = dir.spec("safe.spec", "Name: x\nLicense: MIT\nRelease: 1%?dist\nBuildRequires: zlib >= 1.2, pkgconfig(foo)\n%define feature 1\n\n%autochangelog\n");
    let result = run(true, &[&path]);
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    assert!(result.stdout.is_empty(), "{result:?}");
    let fixed = std::fs::read_to_string(&path).unwrap();
    assert!(fixed.starts_with("%global feature 1\n\nName:"), "{fixed}");
    assert!(fixed.contains("1%{?dist}"), "{fixed}");
    assert!(
        fixed.contains("BuildRequires: zlib >= 1.2\nBuildRequires: pkgconfig(foo)"),
        "{fixed}"
    );
    assert_eq!(run(true, &[&path]).status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(path).unwrap(), fixed);
}

#[test]
fn plain_lint_does_not_rewrite_files() {
    let dir = SpecDir::new();
    let source = "Name: x\nLicense: MIT\nRelease: 1%?dist\n%autochangelog\n";
    let path = dir.spec("readonly.spec", source);
    let result = run(false, &[&path]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("[braced-macros]"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[test]
fn unsafe_preamble_stays_unchanged_and_reports_findings() {
    let dir = SpecDir::new();
    let source = "Name:  x  \nLicense: MIT\n%gometa\nBuildRequires: gcc cmake\n%autochangelog\n";
    let path = dir.spec("unsafe.spec", source);
    let result = run(true, &[&path]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("[unsafe-preamble]"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[test]
fn trailing_electronmeta_does_not_prevent_fixing_tags() {
    let dir = SpecDir::new();
    let source = "Name:           discord\nVersion:        1\nRelease:        1%?dist\nSource0:        https://example.com/discord.tar.gz\nLicense:        MIT\nRequires:       zenity\n\n%electronmeta -D  \n\n%description\nExample\n\n%autochangelog\n";
    let path = dir.spec("discord.spec", source);
    let result = run(true, &[&path]);
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    assert!(result.stdout.is_empty(), "{result:?}");
    let fixed = std::fs::read_to_string(&path).unwrap();
    assert_ne!(fixed, source);
    assert!(fixed.contains("Release: 1%{?dist}"), "{fixed}");
    assert!(fixed.contains("License: MIT\n\nSource0:"), "{fixed}");
    assert!(
        fixed.contains("%electronmeta -D  \n\n%description"),
        "{fixed}"
    );
    assert_eq!(run(true, &[&path]).status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(path).unwrap(), fixed);
}

#[test]
fn reports_remaining_findings_at_fixed_line_numbers() {
    let dir = SpecDir::new();
    let path = dir.spec("manual.spec", "Name: x\nLicense: MIT\nBuildRequires: gcc cmake\n%define delayed %{version}\nRequires: x = %{version}-%{release}\n\n%prep\n%setup -q\n\n%autochangelog\n");
    let result = run(true, &[&path]);
    assert_eq!(result.status.code(), Some(1));
    let fixed = std::fs::read_to_string(&path).unwrap();
    let diagnostics = String::from_utf8(result.stdout).unwrap();
    let line = fixed
        .lines()
        .position(|line| line.starts_with("%define delayed"))
        .unwrap()
        + 1;
    assert!(
        diagnostics.contains(&format!("{}:{line}:1:", path.display())),
        "{diagnostics}"
    );
    for rule in ["use-global", "macro-placement", "use-evr", "use-autosetup"] {
        assert!(diagnostics.contains(&format!("[{rule}]")), "{diagnostics}");
    }
    assert!(
        !diagnostics.contains("[buildrequires-one-per-line]"),
        "{diagnostics}"
    );
    assert!(fixed.contains("%setup -q"));
    assert!(fixed.contains("%{version}-%{release}"));
}

#[test]
fn fixes_all_requested_files_even_if_findings_remain() {
    let dir = SpecDir::new();
    let unsafe_path = dir.spec("unsafe.spec", "Name: x\n%gometa\n");
    let safe_path = dir.spec(
        "safe.spec",
        "Name: x\nLicense: MIT\nRelease: 1%?dist\n%autochangelog\n",
    );
    assert_eq!(
        run(true, &[&unsafe_path, &safe_path]).status.code(),
        Some(1)
    );
    assert!(std::fs::read_to_string(safe_path)
        .unwrap()
        .contains("1%{?dist}"));
}

#[test]
fn fix_requires_files() {
    let result = run(true, &[]);
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("--fix requires at least one spec file"));
}

#[test]
fn missing_file_is_an_io_error() {
    let dir = SpecDir::new();
    let path = dir.0.join("missing.spec");
    assert_eq!(run(true, &[&path]).status.code(), Some(2));
    assert!(!path.exists());
}

#[test]
fn fixes_do_not_sort_or_reindent_scriptlets() {
    let source = "Name: x\nLicense: MIT\nRequires: zlib\nRequires: abc\nBuildRequires: zlib gcc\n\n%build\n\techo hello\n\n%autochangelog\n";
    let fixed = rpmspecfmt::lint::fix(source).unwrap();
    assert!(fixed.contains("Requires: zlib\nRequires: abc"), "{fixed}");
    assert!(
        fixed.contains("BuildRequires: zlib\nBuildRequires: gcc"),
        "{fixed}"
    );
    assert!(fixed.contains("\techo hello"), "{fixed}");
    assert_eq!(rpmspecfmt::lint::fix(&fixed).unwrap(), fixed);
}
