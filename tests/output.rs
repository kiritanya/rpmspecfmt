use std::io::{self, Write};
use std::path::Path;

use rpmspecfmt::diagnostic::{Edit, Finding, Fix};
use rpmspecfmt::output::{DiagnosticOutput, RichOutput, SarifOutput, ShortOutput};
use rpmspecfmt::rules::RuleKind;

fn finding(span: std::ops::Range<usize>) -> Finding {
    Finding {
        rule: "braced-macros",
        kind: RuleKind::Format,
        span,
        message: "use braced macros".into(),
        fix: None,
    }
}

#[test]
fn backends_share_the_output_trait_and_validate_utf8_ranges() {
    let mut rich = Vec::new();
    let mut short = Vec::new();
    let mut sarif = Vec::new();
    let mut backends: Vec<Box<dyn DiagnosticOutput + '_>> = vec![
        Box::new(RichOutput::new(&mut rich, false)),
        Box::new(ShortOutput::new(&mut short)),
        Box::new(SarifOutput::new(&mut sarif)),
    ];
    for backend in &mut backends {
        for span in [1..2, 0..4, std::ops::Range { start: 3, end: 0 }] {
            assert!(backend
                .write_file(Path::new("input.spec"), "日", &[finding(span)])
                .is_err());
        }
        backend
            .write_file(Path::new("input.spec"), "日", &[finding(0..3)])
            .unwrap();
        backend.finish().unwrap();
    }
    drop(backends);
    assert!(String::from_utf8(rich).unwrap().contains("日"));
    assert_eq!(
        String::from_utf8(short).unwrap(),
        "input.spec:1:1: warning: use braced macros [braced-macros]\n"
    );
    let document: serde_json::Value = serde_json::from_slice(&sarif).unwrap();
    assert_eq!(document["runs"][0]["results"].as_array().unwrap().len(), 1);
    assert_eq!(
        document["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]
            ["endColumn"],
        2
    );
}

#[test]
fn sarif_preserves_atomic_multi_edit_fixes_and_rejects_invalid_edits() {
    let mut bytes = Vec::new();
    let mut output = SarifOutput::new(&mut bytes);
    let mut finding = finding(0..1);
    finding.fix = Some(Fix {
        edits: vec![
            Edit {
                range: 0..1,
                replacement: "a".into(),
            },
            Edit {
                range: 3..3,
                replacement: "b".into(),
            },
        ],
    });
    output
        .write_file(Path::new("/work/input #?.spec"), "x\ny", &[finding.clone()])
        .unwrap();
    finding.fix.as_mut().unwrap().edits[1].range = 3..4;
    assert!(output
        .write_file(Path::new("bad.spec"), "x\ny", &[finding])
        .is_err());
    output.finish().unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let results = document["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let change = &results[0]["fixes"][0]["artifactChanges"][0];
    assert_eq!(
        change["artifactLocation"]["uri"],
        "file:///work/input%20%23%3F.spec"
    );
    assert_eq!(change["replacements"].as_array().unwrap().len(), 2);
    assert_eq!(
        change["replacements"][1]["deletedRegion"],
        serde_json::json!({
            "startLine": 2, "startColumn": 2, "endLine": 2, "endColumn": 2
        })
    );
}

#[test]
fn sarif_does_not_publish_check_only_fixes() {
    let mut bytes = Vec::new();
    let mut output = SarifOutput::new(&mut bytes);
    let mut finding = finding(0..1);
    finding.rule = "missing-license";
    finding.kind = RuleKind::CheckOnly;
    finding.fix = Some(Fix {
        edits: vec![Edit {
            range: 0..1,
            replacement: "unsafe".into(),
        }],
    });
    output
        .write_file(Path::new("input.spec"), "x", &[finding])
        .unwrap();
    output.finish().unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let result = &document["runs"][0]["results"][0];
    assert_eq!(result["properties"]["fixAvailable"], false);
    assert!(result.get("fixes").is_none());
}

#[test]
fn sarif_rejects_unknown_rules_without_collecting_a_partial_batch() {
    let mut bytes = Vec::new();
    let mut output = SarifOutput::new(&mut bytes);
    let mut unknown = finding(0..1);
    unknown.rule = "unknown";
    assert!(output
        .write_file(Path::new("input.spec"), "x", &[finding(0..1), unknown])
        .is_err());
    output.finish().unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["runs"][0]["results"], serde_json::json!([]));
}

struct BrokenWriter;

impl Write for BrokenWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("write failed"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}

#[test]
fn backends_propagate_output_errors() {
    let path = Path::new("input.spec");
    let findings = [finding(0..1)];
    let mut rich = RichOutput::new(BrokenWriter, false);
    let mut short = ShortOutput::new(BrokenWriter);
    assert!(rich.write_file(path, "x", &findings).is_err());
    assert!(short.write_file(path, "x", &findings).is_err());
    assert!(rich.finish().is_err());
    assert!(short.finish().is_err());
    let mut sarif = SarifOutput::new(BrokenWriter);
    sarif.write_file(path, "x", &findings).unwrap();
    assert!(sarif.finish().is_err());
}
