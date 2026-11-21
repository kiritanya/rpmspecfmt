use std::io::Write;
use std::path::Path;

use serde_json::{json, Value};
use stable_eyre::eyre::{eyre, Result, WrapErr};

use super::{flush, location, validate_span, DiagnosticOutput};
use crate::diagnostic::Finding;
use crate::rules::{self, Rule, RuleKind};

/// SARIF 2.1.0: one run containing results from every file in an invocation.
/// Nothing is written until `finish`, including when there are no findings.
pub struct SarifOutput<W> {
    writer: W,
    results: Vec<Value>,
}

impl<W: Write> SarifOutput<W> {
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            results: Vec::new(),
        }
    }
}

impl<W: Write> DiagnosticOutput for SarifOutput<W> {
    fn write_file(&mut self, path: &Path, source: &str, findings: &[Finding]) -> Result<()> {
        let uri = artifact_uri(path)?;
        let catalog = rules::builtins();
        // Collect atomically so invalid ranges cannot leave a partial file batch.
        let mut results = Vec::with_capacity(findings.len());
        for finding in findings {
            let index = catalog
                .iter()
                .position(|rule| rule.id() == finding.rule)
                .ok_or_else(|| eyre!("unknown SARIF rule: {}", finding.rule))?;
            let mut result = json!({
                "ruleId": finding.rule,
                "ruleIndex": index,
                "level": "warning",
                "message": { "text": finding.message },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": uri },
                    "region": region(source, &finding.span)?
                }}],
                "properties": {
                    "ruleKind": kind_name(finding.kind),
                    "fixAvailable": finding.kind != RuleKind::CheckOnly && finding.fix.is_some()
                }
            });
            if finding.kind != RuleKind::CheckOnly {
                if let Some(fix) = &finding.fix {
                    let replacements = fix
                        .edits
                        .iter()
                        .map(|edit| {
                            Ok(json!({
                                "deletedRegion": region(source, &edit.range)?,
                                "insertedContent": { "text": edit.replacement.as_ref() }
                            }))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    result["fixes"] = json!([{
                        "description": { "text": finding.message },
                        "artifactChanges": [{
                            "artifactLocation": { "uri": uri },
                            "replacements": replacements
                        }]
                    }]);
                }
            }
            results.push(result);
        }
        self.results.extend(results);
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        let rules: Vec<_> = rules::builtins()
            .iter()
            .map(|rule| {
                json!({
                    "id": rule.id(),
                    "shortDescription": { "text": rule.message() },
                    "defaultConfiguration": { "level": "warning" },
                    "properties": { "ruleKind": kind_name(rule.kind()) }
                })
            })
            .collect();
        let document = json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": { "driver": {
                    "name": "rpmspecfmt",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": env!("CARGO_PKG_REPOSITORY"),
                    "rules": rules
                }},
                "columnKind": "unicodeCodePoints",
                "results": self.results
            }]
        });
        serde_json::to_writer_pretty(&mut self.writer, &document)
            .wrap_err("failed to serialize SARIF diagnostics")?;
        writeln!(self.writer).wrap_err("failed to write SARIF diagnostics")?;
        flush(&mut self.writer)
    }
}

fn region(source: &str, span: &std::ops::Range<usize>) -> Result<Value> {
    validate_span(source, span)?;
    let (start_line, start_column) = location(source, span.start);
    let (end_line, end_column) = location(source, span.end);
    Ok(json!({
        "startLine": start_line, "startColumn": start_column,
        "endLine": end_line, "endColumn": end_column
    }))
}

const fn kind_name(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::Format => "format",
        RuleKind::Fixable => "fixable",
        RuleKind::CheckOnly => "check-only",
    }
}

/// Paths are URI references, not raw filesystem strings. Encode UTF-8 bytes
/// (including spaces, percent signs, and fragment/query delimiters).
fn artifact_uri(path: &Path) -> Result<String> {
    use std::fmt::Write as _;
    let text = path
        .to_str()
        .ok_or_else(|| eyre!("SARIF requires a UTF-8 file path"))?;
    let mut uri = if path.is_absolute() {
        "file://".to_string()
    } else {
        String::new()
    };
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            write!(uri, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    Ok(uri)
}
