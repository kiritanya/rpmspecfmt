//! Diagnostic output backends. Inspection and fixes remain in the rule engine.
use std::io::Write;
use std::path::Path;

use stable_eyre::eyre::{ensure, Result};

use crate::diagnostic::Finding;

mod rich;
mod sarif;
mod short;

pub use rich::RichOutput;
pub use sarif::SarifOutput;
pub use short::ShortOutput;

/// A backend receives findings for each exact source revision, then finishes
/// once for the entire invocation. Backends may stream or collect a document.
pub trait DiagnosticOutput {
    /// Write or collect findings for one file.
    ///
    /// # Errors
    /// Returns an error for invalid source ranges or output failures.
    fn write_file(&mut self, path: &Path, source: &str, findings: &[Finding]) -> Result<()>;

    /// Complete the output, including documents with no findings.
    ///
    /// # Errors
    /// Returns an error if serialization or output fails.
    fn finish(&mut self) -> Result<()>;
}

pub(super) fn validate_span(source: &str, span: &std::ops::Range<usize>) -> Result<()> {
    ensure!(
        span.start <= span.end && span.end <= source.len(),
        "diagnostic range is outside the source"
    );
    ensure!(
        source.is_char_boundary(span.start) && source.is_char_boundary(span.end),
        "diagnostic range splits a UTF-8 character"
    );
    Ok(())
}

/// One-based Unicode code-point columns, shared by short output and SARIF.
pub(super) fn location(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset];
    (
        prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
        prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
    )
}

pub(super) fn flush(writer: &mut impl Write) -> Result<()> {
    use stable_eyre::eyre::WrapErr;
    writer.flush().wrap_err("failed to flush diagnostics")
}
