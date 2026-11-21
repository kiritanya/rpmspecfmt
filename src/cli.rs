use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use clap::{ColorChoice, Parser, ValueEnum};
use clap_verbosity_flag::{Verbosity, WarnLevel};
use similar::TextDiff;
use stable_eyre::eyre::{bail, eyre, Result, WrapErr};
use tracing_subscriber::EnvFilter;

use crate::config::FileConfig;
use crate::engine::{Engine, FixMode};
use crate::output::{DiagnosticOutput, RichOutput, SarifOutput, ShortOutput};
use crate::Config;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DiagnosticFormat {
    Rich,
    Short,
    Sarif,
}

#[derive(Debug, Parser)]
#[command(
    name = "rpmspecfmt",
    about = "Format and lint RPM spec files",
    version,
    disable_help_subcommand = true
)]
struct Args {
    /// Spec files. Reads stdin when omitted.
    files: Vec<PathBuf>,
    /// Write formatted output back to files.
    #[arg(short = 'w', long, conflicts_with_all = ["check", "diff", "lint"])]
    write: bool,
    /// Exit 1 if formatting would change any input.
    #[arg(long, conflicts_with_all = ["diff", "lint"])]
    check: bool,
    /// Show a unified formatting diff.
    #[arg(long, conflicts_with = "lint")]
    diff: bool,
    /// Check all three kinds of rules without rewriting files.
    #[arg(long)]
    lint: bool,
    /// With --lint, apply formatting and safe lint fixes in place.
    #[arg(long, requires = "lint", conflicts_with_all = ["write", "check", "diff"])]
    fix: bool,
    /// Whitespace-only formatting; disable non-whitespace style rules.
    #[arg(long)]
    safe: bool,
    /// Keep values of identical tags in their original order.
    #[arg(long)]
    no_sort: bool,
    /// Disable dependency wrapping (`BuildRequires` splitting is separate).
    #[arg(long)]
    no_wrap: bool,
    /// Do not expand leading tabs in scriptlets.
    #[arg(long)]
    no_reindent: bool,
    /// Select a TOML configuration file instead of searching upwards.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Fixed tag value column; 0 aligns each group automatically.
    #[arg(long)]
    tag_column: Option<usize>,
    /// Maximum wrapping width.
    #[arg(long, visible_alias = "wrap-width")]
    max_width: Option<usize>,
    /// Diagnostic rendering: source excerpts, one-line diagnostics, or SARIF JSON.
    #[arg(long, value_enum, default_value = "rich")]
    diagnostic_format: DiagnosticFormat,
    /// Diagnostic and log colors.
    #[arg(long, value_enum, default_value = "auto")]
    color: ColorChoice,
    /// Increase/decrease tracing verbosity with -v/-q. Findings are unaffected.
    #[command(flatten)]
    verbosity: Verbosity<WarnLevel>,
}

/// Shared entry point; the lint alias only selects `--lint` mode.
pub fn main(lint_alias: bool) {
    if let Err(error) = stable_eyre::install() {
        eprintln!("rpmspecfmt: {error:?}");
        std::process::exit(2);
    }
    let mut arguments: Vec<_> = std::env::args_os().collect();
    if lint_alias && !arguments.iter().any(|arg| arg == "--lint") {
        arguments.insert(1, "--lint".into());
    }
    let args = Args::parse_from(arguments);
    let color = match args.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        }
    };
    let result = init_logging(&args, color).and_then(|()| run(&args, color));
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("rpmspecfmt: {error:?}");
            std::process::exit(2);
        }
    }
}

fn init_logging(args: &Args, color: bool) -> Result<()> {
    let level = args.verbosity.tracing_level_filter();
    let filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(color)
        .without_time()
        .try_init()
        .map_err(|error| eyre!(error))
}

fn build_config(args: &Args) -> Result<Config> {
    let mut defaults = Config::default();
    // Preserve the conservative --fix profile; file/env/CLI values can override it.
    if args.fix {
        defaults.sort = false;
        defaults.wrap = false;
        defaults.reindent = false;
    }
    let overrides = FileConfig {
        tag_column: args.tag_column,
        max_width: args.max_width,
        sort: args.no_sort.then_some(false),
        wrap: args.no_wrap.then_some(false),
        reindent: args.no_reindent.then_some(false),
        ..FileConfig::default()
    };
    let discovered = if args.config.is_none() {
        FileConfig::find()
    } else {
        None
    };
    let mut cfg = FileConfig::resolve(
        args.config.as_deref().or(discovered.as_deref()),
        &defaults,
        &overrides,
    )?;
    // Safe mode is an invariant, not another provider that env can override.
    if args.safe {
        cfg.group_tags = false;
        cfg.split_buildrequires = false;
        cfg.normalize_macros = false;
        cfg.global_macros = false;
        cfg.sort = false;
        cfg.wrap = false;
        cfg.reindent = false;
        cfg.wrap_description = false;
        cfg.convert_paths = false;
    }
    Ok(cfg)
}

fn run(args: &Args, color: bool) -> Result<i32> {
    if matches!(args.diagnostic_format, DiagnosticFormat::Sarif) && !args.lint {
        bail!("--diagnostic-format sarif requires --lint");
    }
    if args.files.is_empty() && args.fix {
        bail!("--fix requires at least one spec file; stdin is lint-only");
    }
    if args.files.is_empty() && args.write {
        bail!("--write cannot be used with stdin");
    }
    let engine = Engine::new(build_config(args)?);
    let mut output: Box<dyn DiagnosticOutput> = match args.diagnostic_format {
        DiagnosticFormat::Rich => Box::new(RichOutput::new(std::io::stderr(), color)),
        DiagnosticFormat::Short => Box::new(ShortOutput::new(std::io::stdout())),
        DiagnosticFormat::Sarif => Box::new(SarifOutput::new(std::io::stdout())),
    };
    if args.files.is_empty() {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .wrap_err("failed to read stdin")?;
        let found = process(
            Path::new("<stdin>"),
            &source,
            args,
            &engine,
            output.as_mut(),
        )?;
        output.finish()?;
        return Ok(i32::from(found));
    }
    let mut found = false;
    for path in &args.files {
        let source = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("failed to read {}", path.display()))?;
        found |= process(path, &source, args, &engine, output.as_mut())?;
    }
    output.finish()?;
    Ok(i32::from(found))
}

/// Return whether the selected check/lint mode should exit with findings.
fn process(
    path: &Path,
    source: &str,
    args: &Args,
    engine: &Engine,
    output: &mut dyn DiagnosticOutput,
) -> Result<bool> {
    tracing::info!(path = %path.display(), lint = args.lint, fix = args.fix, "processing spec");
    if args.lint {
        let (fixed, findings) = if args.fix {
            let outcome = engine.apply_with_findings(source, FixMode::SafeFix)?;
            (outcome.source, outcome.findings)
        } else {
            (source.to_string(), engine.check(source)?)
        };
        if fixed != source {
            std::fs::write(path, &fixed)
                .wrap_err_with(|| format!("failed to write {}", path.display()))?;
        }
        output.write_file(path, &fixed, &findings)?;
        return Ok(!findings.is_empty());
    }
    let outcome = engine.apply_with_findings(source, FixMode::FormatOnly)?;
    let formatted = outcome.source;
    let changed = formatted != source;
    let unsafe_findings: Vec<_> = outcome
        .findings
        .into_iter()
        .filter(|d| d.rule == "unsafe-preamble")
        .collect();
    output.write_file(path, source, &unsafe_findings)?;
    if args.check {
        if changed {
            eprintln!("would reformat {}", path.display());
        }
    } else if args.diff {
        if changed {
            let name = path.display().to_string();
            print!(
                "{}",
                TextDiff::from_lines(source, &formatted)
                    .unified_diff()
                    .header(&name, &format!("{name} (formatted)"))
            );
        }
    } else if args.write {
        if changed {
            std::fs::write(path, formatted)
                .wrap_err_with(|| format!("failed to write {}", path.display()))?;
        }
    } else {
        print!("{formatted}");
    }
    Ok(args.check && changed)
}
