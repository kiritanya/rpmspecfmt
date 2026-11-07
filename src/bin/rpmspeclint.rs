use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::exit;

use anyhow::{Context, Result};
use clap::Parser;

use rpmspecfmt::lint::lint;

#[derive(Debug, Parser)]
#[command(
    name = "rpmspeclint",
    about = "Lint RPM spec files",
    version,
    disable_help_subcommand = true
)]
struct Args {
    /// Spec files to lint. Reads stdin when omitted.
    files: Vec<PathBuf>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("rpmspeclint: {err:#}");
        exit(2);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    if args.files.is_empty() {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .context("failed to read stdin")?;
        if lint_file(Path::new("<stdin>"), &source) {
            exit(1);
        }
        return Ok(());
    }

    let mut found = false;
    for path in &args.files {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        found |= lint_file(path, &source);
    }

    if found {
        exit(1);
    }
    Ok(())
}

/// Print findings for one file and report whether anything was found.
fn lint_file(path: &Path, source: &str) -> bool {
    let diagnostics = lint(source);

    for diagnostic in &diagnostics {
        println!(
            "{}:{}:{}: warning: {} [{}]",
            path.display(),
            diagnostic.line,
            diagnostic.column,
            diagnostic.message,
            diagnostic.rule
        );
    }

    !diagnostics.is_empty()
}
