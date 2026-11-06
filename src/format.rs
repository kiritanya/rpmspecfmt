use crate::parse;
use anyhow::Result;
use tree_sitter::Node;

/// Formatting options.
#[derive(Debug, Clone)]
pub struct Config {
    /// Column (0-based) where tag values start.
    pub tag_column: usize,
    /// Maximum line width used when wrapping dependency tags.
    pub wrap_width: usize,
    /// Sort values of consecutive tags with the same name.
    pub sort: bool,
    /// Wrap long dependency tag lines.
    pub wrap: bool,
    /// Replace leading tabs in scriptlet bodies with four spaces.
    pub reindent: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tag_column: 16,
            wrap_width: 100,
            sort: true,
            wrap: true,
            reindent: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Other = 0,
    Body = 1,
    SectionHeader = 2,
    Tag = 3,
    Skip = 4,
}

const SECTION_KINDS: &[&str] = &[
    "description",
    "package",
    "files",
    "changelog",
    "prep_scriptlet",
    "build_scriptlet",
    "install_scriptlet",
    "check_scriptlet",
    "clean_scriptlet",
    "conf_scriptlet",
    "runtime_scriptlet",
    "runtime_scriptlet_interpreter",
    "trigger",
    "file_trigger",
    "generate_buildrequires",
    "sourcelist",
    "patchlist",
];

const DEPENDENCY_TAGS: &[&str] = &[
    "BuildRequires",
    "Requires",
    "Provides",
    "Conflicts",
    "Obsoletes",
    "Recommends",
    "Suggests",
    "Supplements",
    "Enhances",
    "BuildConflicts",
    "OrderWithRequires",
];

/// Format a spec file.
pub fn format(source: &str, cfg: &Config) -> Result<String> {
    if source.is_empty() {
        return Ok(String::new());
    }

    // A missing final newline makes the grammar report a MISSING token at EOF,
    // which would mark the last section as unformattable. Parse a normalised
    // copy instead; node rows still map one-to-one onto the source lines.
    let parse_source = if source.ends_with('\n') {
        std::borrow::Cow::Borrowed(source)
    } else {
        std::borrow::Cow::Owned(format!("{source}\n"))
    };
    let tree = parse::parse(&parse_source)?;
    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    if source.ends_with('\n') {
        lines.pop();
    }

    let mut kinds = vec![Kind::Other; lines.len()];
    walk(tree.root_node(), &mut kinds);

    mark_heredocs(&lines, &mut kinds);

    // Split tags into (tag, value) pairs.
    let mut parts: Vec<Option<(String, String)>> = lines
        .iter()
        .zip(&kinds)
        .map(|(line, kind)| {
            if *kind == Kind::Tag {
                split_tag(line)
            } else {
                None
            }
        })
        .collect();

    if cfg.sort {
        sort_tag_runs(&kinds, &mut parts);
    }

    // Rows replaced by a pre-rendered block (dependency wrapping/sorting).
    let plans = build_dependency_plans(&kinds, &mut parts, cfg);

    let mut rows: Vec<(Kind, String)> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if let Some(block) = &plans[i] {
            for part in block {
                rows.push((Kind::Tag, part.clone()));
            }
            continue;
        }

        let kind = kinds[i];
        let text = match kind {
            Kind::Skip => line.clone(),
            Kind::Tag => match &parts[i] {
                Some((tag, value)) => render_tag(tag, value, cfg),
                None => line.trim_end().to_string(),
            },
            Kind::Body => {
                let line = if cfg.reindent {
                    expand_leading_tabs(line)
                } else {
                    line.clone()
                };
                line.trim_end().to_string()
            }
            _ => line.trim_end().to_string(),
        };
        rows.push((kind, text));
    }

    let rows = apply_spacing(rows);
    let mut out: Vec<&str> = rows.iter().map(|(_, text)| text.as_str()).collect();
    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    let mut text = out.join("\n");
    text.push('\n');
    Ok(text)
}

fn walk(node: Node<'_>, kinds: &mut [Kind]) {
    if node.is_error() || node.is_missing() {
        mark_skip(kinds, node.start_position().row, node.end_position().row);
        return;
    }

    let kind = node.kind();

    if kind == "preamble_tag" || kind == "packager_tag" {
        set_kind(kinds, node.start_position().row, Kind::Tag);
    } else if SECTION_KINDS.contains(&kind) {
        let start = node.start_position().row;
        let end = node.end_position();
        let last = if end.column == 0 {
            end.row.saturating_sub(1)
        } else {
            end.row
        };

        if node.has_error() {
            mark_skip(kinds, start, last);
            return;
        }

        set_kind(kinds, start, Kind::SectionHeader);
        for row in (start + 1)..=last {
            set_kind(kinds, row, Kind::Body);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, kinds);
    }
}

fn set_kind(kinds: &mut [Kind], row: usize, kind: Kind) {
    if row < kinds.len() && kinds[row] < kind {
        kinds[row] = kind;
    }
}

fn mark_skip(kinds: &mut [Kind], start: usize, end: usize) {
    for row in start..=end.min(kinds.len().saturating_sub(1)) {
        kinds[row] = Kind::Skip;
    }
}

/// Mark heredoc bodies as skipped so tabs and trailing spaces are preserved.
fn mark_heredocs(lines: &[String], kinds: &mut [Kind]) {
    let mut i = 0;
    while i < lines.len() {
        if kinds[i] == Kind::Body {
            if let Some(delim) = heredoc_delim(&lines[i]) {
                let mut j = i + 1;
                while j < lines.len() && kinds[j] == Kind::Body {
                    kinds[j] = Kind::Skip;
                    if lines[j].trim() == delim {
                        break;
                    }
                    j += 1;
                }
                i = j;
            }
        }
        i += 1;
    }
}

fn heredoc_delim(line: &str) -> Option<String> {
    let idx = line.find("<<")?;
    let rest = &line[idx + 2..];
    let rest = rest.trim_start_matches('-').trim_start();
    let rest = rest.trim_start_matches(['"', '\'']);
    let delim: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if delim.is_empty() {
        None
    } else {
        Some(delim)
    }
}

fn expand_leading_tabs(line: &str) -> String {
    let tabs = line.len() - line.trim_start_matches('\t').len();
    if tabs == 0 {
        return line.to_string();
    }
    format!("{}{}", "    ".repeat(tabs), &line[tabs..])
}

fn split_tag(line: &str) -> Option<(String, String)> {
    let s = line.trim_start();
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }

    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }

    if i < bytes.len() && bytes[i] == b'(' {
        let mut j = i + 1;
        while j < bytes.len() && bytes[j] != b')' {
            j += 1;
        }
        if j < bytes.len() {
            i = j + 1;
        }
    }

    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }

    Some((s[..=i].to_string(), s[i + 1..].trim().to_string()))
}

fn base_tag(tag: &str) -> &str {
    match tag.find('(') {
        Some(idx) => &tag[..idx],
        None => tag.trim_end_matches(':'),
    }
}

fn render_tag(tag: &str, value: &str, cfg: &Config) -> String {
    let tag = tag.trim_end();
    if value.is_empty() {
        return tag.to_string();
    }

    let padding = tag_padding(tag, cfg);
    format!("{tag}{}{value}", " ".repeat(padding))
}

fn tag_padding(tag: &str, cfg: &Config) -> usize {
    if tag.len() + 1 >= cfg.tag_column {
        1
    } else {
        cfg.tag_column - tag.len()
    }
}

fn token_is_simple(token: &str) -> bool {
    !token.contains(|c: char| "()<>=|'\"#".contains(c)) && !token.contains("%(")
}

/// Dependency tags are handled as a block so that sorting and wrapping are
/// stable under repeated formatting: the tokens are sorted once and then
/// wrapped deterministically.
fn build_dependency_plans(
    kinds: &[Kind],
    parts: &mut [Option<(String, String)>],
    cfg: &Config,
) -> Vec<Option<Vec<String>>> {
    let mut plans = vec![None; parts.len()];
    if !cfg.wrap && !cfg.sort {
        return plans;
    }

    let mut i = 0;
    while i < parts.len() {
        let Some((tag, _)) = &parts[i] else {
            i += 1;
            continue;
        };
        if kinds[i] != Kind::Tag || !DEPENDENCY_TAGS.contains(&base_tag(tag)) {
            i += 1;
            continue;
        }
        let tag = tag.clone();

        let mut j = i + 1;
        while j < parts.len()
            && kinds[j] == Kind::Tag
            && parts[j].as_ref().is_some_and(|(t, _)| *t == tag)
        {
            j += 1;
        }

        let values: Vec<String> = (i..j)
            .filter_map(|k| parts[k].as_ref().map(|(_, v)| v.clone()))
            .collect();
        let tokens: Vec<String> = values
            .iter()
            .flat_map(|value| value.split_whitespace().map(str::to_string))
            .collect();
        let simple = tokens.iter().all(|token| token_is_simple(token));

        if simple && cfg.wrap && !tokens.is_empty() {
            let mut tokens = tokens;
            if cfg.sort {
                tokens.sort_by_key(|token| token.to_lowercase());
            }
            let block = wrap_tokens(&tag, &tokens, cfg);
            plans[i] = Some(block);
            for slot in plans.iter_mut().take(j).skip(i + 1) {
                *slot = Some(Vec::new());
            }
        } else if cfg.sort {
            let mut sorted = values;
            sorted.sort_by_key(|value| value.to_lowercase());
            for (offset, value) in sorted.into_iter().enumerate() {
                parts[i + offset].as_mut().unwrap().1 = value;
            }
        }

        i = j;
    }

    plans
}

fn wrap_tokens(tag: &str, tokens: &[String], cfg: &Config) -> Vec<String> {
    let padding = tag_padding(tag, cfg);
    let gap = " ".repeat(padding);
    let prefix = tag.len() + padding;

    let mut out = Vec::new();
    let mut current = String::new();
    for token in tokens {
        if current.is_empty() {
            current.push_str(token);
        } else if prefix + current.len() + 1 + token.len() <= cfg.wrap_width {
            current.push(' ');
            current.push_str(token);
        } else {
            out.push(format!("{tag}{gap}{current}"));
            current = token.to_string();
        }
    }
    if !current.is_empty() {
        out.push(format!("{tag}{gap}{current}"));
    }
    out
}

fn sort_tag_runs(kinds: &[Kind], parts: &mut [Option<(String, String)>]) {
    let mut i = 0;
    while i < parts.len() {
        let Some((tag, _)) = &parts[i] else {
            i += 1;
            continue;
        };
        if DEPENDENCY_TAGS.contains(&base_tag(tag)) {
            i += 1;
            continue;
        }
        let tag = tag.clone();
        let mut j = i + 1;
        while j < parts.len()
            && kinds[j] == Kind::Tag
            && parts[j].as_ref().is_some_and(|(t, _)| *t == tag)
        {
            j += 1;
        }

        let mut values: Vec<String> = (i..j)
            .map(|k| parts[k].as_ref().unwrap().1.clone())
            .collect();
        values.sort_by_key(|value| value.to_lowercase());
        for (offset, value) in values.into_iter().enumerate() {
            parts[i + offset].as_mut().unwrap().1 = value;
        }

        i = j;
    }
}

fn apply_spacing(rows: Vec<(Kind, String)>) -> Vec<(Kind, String)> {
    // Insert one blank line before section headers. If a comment block sits
    // directly above the header, keep the comment attached to the section and
    // insert the blank line above the comment block instead.
    let mut spaced: Vec<(Kind, String)> = Vec::with_capacity(rows.len() + 8);
    for (kind, text) in rows {
        if kind == Kind::SectionHeader && !spaced.is_empty() {
            let last = spaced.last().unwrap();
            if !last.1.is_empty() && last.0 != Kind::SectionHeader {
                if last.1.trim_start().starts_with('#') {
                    let mut idx = spaced.len();
                    while idx > 0 && spaced[idx - 1].1.trim_start().starts_with('#') {
                        idx -= 1;
                    }
                    if idx > 0 && !spaced[idx - 1].1.is_empty() {
                        spaced.insert(idx, (Kind::Other, String::new()));
                    }
                } else {
                    spaced.push((Kind::Other, String::new()));
                }
            }
        }
        spaced.push((kind, text));
    }

    // Drop blank lines directly after a section header.
    let mut trimmed: Vec<(Kind, String)> = Vec::with_capacity(spaced.len());
    let mut i = 0;
    while i < spaced.len() {
        trimmed.push(spaced[i].clone());
        if spaced[i].0 == Kind::SectionHeader {
            i += 1;
            while i < spaced.len() && spaced[i].1.is_empty() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    // Collapse runs of blank lines outside section bodies.
    let mut out: Vec<(Kind, String)> = Vec::with_capacity(trimmed.len());
    let mut i = 0;
    while i < trimmed.len() {
        if trimmed[i].1.is_empty() {
            let mut j = i;
            while j < trimmed.len() && trimmed[j].1.is_empty() {
                j += 1;
            }
            let prev_body = out.last().is_some_and(|(kind, _)| *kind == Kind::Body);
            let next_body = trimmed.get(j).is_some_and(|(kind, _)| *kind == Kind::Body);
            let keep = if prev_body || next_body { j - i } else { 1 };
            for _ in 0..keep {
                out.push((Kind::Other, String::new()));
            }
            i = j;
        } else {
            out.push(trimmed[i].clone());
            i += 1;
        }
    }

    while out.first().is_some_and(|(_, text)| text.is_empty()) {
        out.remove(0);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ws(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn aligns_packager_with_email() {
        let src = "Name: foo\nVersion: 1\nRelease: 1\nSummary: x\nLicense: MIT\nPackager: Alice <alice@example.com>\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.contains("Packager:       Alice <alice@example.com>"),
            "{out}"
        );
    }

    #[test]
    fn preserves_non_whitespace_bytes() {
        let src = "Name:   foo\nVersion: 1\n\n%description\n some text  \n\n * item\n";
        let out = format(src, &Config::default()).unwrap();
        assert_eq!(strip_ws(src), strip_ws(&out));
    }

    #[test]
    fn is_idempotent() {
        let src =
            "Name:   foo\nVersion: 1\nRelease: 1\n\n%description\nText\n\n%files\n%{_bindir}/foo\n";
        let once = format(src, &Config::default()).unwrap();
        let twice = format(&once, &Config::default()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn sorts_dependency_tokens() {
        let src = "Name: x\n\nBuildRequires:  zlib openssl\nBuildRequires:  cmake gcc\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.contains("BuildRequires:  cmake gcc openssl zlib"),
            "{out}"
        );
    }

    #[test]
    fn wraps_long_dependency_lines() {
        let src = "Name: x\n\nBuildRequires: alpha bravo charlie delta echo foxtrot golf hotel india juliet\n";
        let cfg = Config {
            wrap_width: 40,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert!(out.lines().count() > 2, "{out}");
        for line in out
            .lines()
            .filter(|line| line.starts_with("BuildRequires:"))
        {
            assert!(line.len() <= 40, "{line}");
        }
        assert_eq!(format(&out, &cfg).unwrap(), out);
    }

    #[test]
    fn safe_mode_keeps_dependency_order() {
        let src = "Name: x\n\nBuildRequires:  zlib\nBuildRequires:  cmake\n";
        let cfg = Config {
            sort: false,
            wrap: false,
            reindent: false,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert_eq!(
            out,
            "Name:           x\n\nBuildRequires:  zlib\nBuildRequires:  cmake\n"
        );
    }

    #[test]
    fn adds_blank_line_before_sections_only() {
        let src = "Name: x\n%description\nText\n%files\n/foo\n";
        let out = format(src, &Config::default()).unwrap();
        assert_eq!(
            out,
            "Name:           x\n\n%description\nText\n\n%files\n/foo\n"
        );
    }

    #[test]
    fn keeps_heredoc_content() {
        let src = "Name: x\n\n%build\ncat << 'EOF'\na\t b  \nEOF\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(out.contains("a\t b  \n"), "{out}");
    }

    #[test]
    fn removes_trailing_whitespace_and_adds_final_newline() {
        let out = format("Name:   x   ", &Config::default()).unwrap();
        assert_eq!(out, "Name:           x\n");
    }
}
