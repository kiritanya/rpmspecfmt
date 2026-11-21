use std::io::Write;
use std::path::Path;

use ariadne::{IndexType, Label, Report, ReportKind, Source};
use stable_eyre::eyre::{Result, WrapErr};

use super::{flush, validate_span, DiagnosticOutput};
use crate::diagnostic::Finding;
use crate::rules::RuleKind;

/// Source excerpts rendered by Ariadne, normally written to stderr.
pub struct RichOutput<W> {
    writer: W,
    color: bool,
}

impl<W: Write> RichOutput<W> {
    #[must_use]
    pub const fn new(writer: W, color: bool) -> Self {
        Self { writer, color }
    }
}

impl<W: Write> DiagnosticOutput for RichOutput<W> {
    fn write_file(&mut self, path: &Path, source: &str, findings: &[Finding]) -> Result<()> {
        let path = path.display().to_string();
        for finding in findings {
            validate_span(source, &finding.span)?;
            let note = match (finding.kind, finding.fix.is_some()) {
                (RuleKind::CheckOnly, _) => "manual review required",
                (RuleKind::Format, _) => "auto-formatting available",
                (_, true) => "safe fix available: rpmspecfmt --lint --fix FILE",
                (_, false) => "automatic fix unavailable for this source; manual review required",
            };
            Report::build(ReportKind::Warning, (path.as_str(), finding.span.clone()))
                .with_code(finding.rule)
                .with_message(&finding.message)
                .with_config(
                    ariadne::Config::default()
                        .with_color(self.color)
                        .with_index_type(IndexType::Byte),
                )
                .with_label(
                    Label::new((path.as_str(), finding.span.clone()))
                        .with_message(&finding.message),
                )
                .with_note(note)
                .finish()
                .write((path.as_str(), Source::from(source)), &mut self.writer)
                .wrap_err("failed to write diagnostic")?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        flush(&mut self.writer)
    }
}
