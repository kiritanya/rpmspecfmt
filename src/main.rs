use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::exit;

use anyhow::{Context, Result};
use clap::Parser;
use similar::TextDiff;

use rpmspecfmt::config::FileConfig;
use rpmspecfmt::{format, Config};

#[derive(Debug, Parser)]
#[command(
    name = "rpmspecfmt",
    about = "Format RPM spec files",
    version,
    disable_help_subcommand = true
)]
struct Args {
    /// Spec files to format. Reads stdin when omitted.
    files: Vec<PathBuf>,

    /// Write the result back to each file.
    #[arg(short = 'w', long, conflicts_with_all = ["check", "diff"])]
    write: bool,

    /// Exit with status 1 if any file would change.
    #[arg(long, conflicts_with = "diff")]
    check: bool,

    /// Print a unified diff instead of the formatted text.
    #[arg(long)]
    diff: bool,

    /// Only normalise whitespace (no sorting, wrapping or reindent).
    #[arg(long)]
    safe: bool,

    /// Do not sort values of consecutive identical tags.
    #[arg(long)]
    no_sort: bool,

    /// Do not wrap long dependency tags.
    #[arg(long)]
    no_wrap: bool,

    /// Do not replace leading tabs in scriptlet bodies.
    #[arg(long)]
    no_reindent: bool,

    /// Config file to read. Searches upwards for .rpmspecfmt.toml when omitted.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Column where tag values start.
    #[arg(long)]
    tag_column: Option<usize>,

    /// Maximum line width for wrapping.
    #[arg(long, visible_alias = "wrap-width")]
    max_width: Option<usize>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("rpmspecfmt: {err:#}");
        exit(2);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let cfg = build_config(&args)?;

    if args.files.is_empty() {
        let mut src = String::new();
        std::io::stdin()
            .read_to_string(&mut src)
            .context("failed to read stdin")?;
        return process_stdin(&src, &cfg, &args);
    }

    let mut changed = false;
    for path in &args.files {
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let out =
            format(&src, &cfg).with_context(|| format!("failed to format {}", path.display()))?;
        let differs = out != src;
        changed |= differs;

        if args.check {
            if differs {
                eprintln!("would reformat {}", path.display());
            }
        } else if args.diff {
            if differs {
                print_diff(&src, &out, path);
            }
        } else if args.write {
            if differs {
                std::fs::write(path, &out)
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
        } else {
            print!("{out}");
        }
    }

    if args.check && changed {
        exit(1);
    }

    Ok(())
}

fn build_config(args: &Args) -> Result<Config> {
    let mut cfg = Config::default();

    if let Some(path) = &args.config {
        FileConfig::load(path)?.apply(&mut cfg);
    } else if let Some(path) = FileConfig::find() {
        FileConfig::load(&path)?.apply(&mut cfg);
    }

    if let Some(value) = args.tag_column {
        cfg.tag_column = value;
    }
    if let Some(value) = args.max_width {
        cfg.max_width = value;
    }

    if args.safe {
        cfg.sort = false;
        cfg.wrap = false;
        cfg.reindent = false;
        cfg.wrap_description = false;
        cfg.convert_paths = false;
    } else {
        if args.no_sort {
            cfg.sort = false;
        }
        if args.no_wrap {
            cfg.wrap = false;
        }
        if args.no_reindent {
            cfg.reindent = false;
        }
    }

    Ok(cfg)
}

fn process_stdin(src: &str, cfg: &Config, args: &Args) -> Result<()> {
    let out = format(src, cfg).context("failed to format stdin")?;
    let differs = out != src;

    if args.check {
        if differs {
            eprintln!("would reformat stdin");
            exit(1);
        }
    } else if args.diff {
        if differs {
            print_diff(src, &out, Path::new("<stdin>"));
        }
    } else if args.write {
        anyhow::bail!("--write cannot be used with stdin");
    } else {
        print!("{out}");
    }

    Ok(())
}

fn print_diff(old: &str, new: &str, path: &Path) {
    let old_name = path.display().to_string();
    let new_name = format!("{} (formatted)", path.display());
    let diff = TextDiff::from_lines(old, new);
    print!("{}", diff.unified_diff().header(&old_name, &new_name));
}
