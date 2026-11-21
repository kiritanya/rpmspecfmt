use std::io::Write;
use std::path::Path;

use stable_eyre::eyre::{Result, WrapErr};

use super::{flush, location, validate_span, DiagnosticOutput};
use crate::diagnostic::Finding;

/// One-line diagnostics, normally written to stdout.
pub struct ShortOutput<W> {
    writer: W,
}

impl<W: Write> ShortOutput<W> {
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> DiagnosticOutput for ShortOutput<W> {
    fn write_file(&mut self, path: &Path, source: &str, findings: &[Finding]) -> Result<()> {
        for finding in findings {
            validate_span(source, &finding.span)?;
            let (line, column) = location(source, finding.span.start);
            writeln!(
                self.writer,
                "{}:{line}:{column}: warning: {} [{}]",
                path.display(),
                finding.message,
                finding.rule
            )
            .wrap_err("failed to write diagnostic")?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        flush(&mut self.writer)
    }
}
