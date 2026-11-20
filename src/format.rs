use crate::path_macros::DEFAULT_PATH_MACROS;
use crate::rpm::{DEPENDENCY_TAGS, SECTION_KINDS};
use crate::style;
use stable_eyre::eyre::Result;
use tree_sitter::{Node, Tree};

/// Formatting options.
#[derive(Debug, Clone)]
pub struct Config {
    /// Column (0-based) where tag values start.
    pub tag_column: usize,
    /// Group and align preamble tags automatically.
    pub group_tags: bool,
    /// Use one dependency expression per `BuildRequires` line.
    pub split_buildrequires: bool,
    /// Prefer braced macro references.
    pub normalize_macros: bool,
    /// Hoist safe literal definitions and prefer %global.
    pub global_macros: bool,
    /// Maximum line width used for wrapping.
    pub max_width: usize,
    /// Sort values of consecutive tags with the same name.
    pub sort: bool,
    /// Wrap long dependency tag lines.
    pub wrap: bool,
    /// Replace leading tabs in scriptlet bodies with four spaces.
    pub reindent: bool,
    /// Re-wrap text in `%description` sections.
    pub wrap_description: bool,
    /// Rewrite paths in `%files` sections with macros.
    pub convert_paths: bool,
    /// Path prefixes and their macro replacements, longest prefix first.
    pub path_macros: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Self {
        let mut path_macros: Vec<(String, String)> = DEFAULT_PATH_MACROS
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect();
        path_macros.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));

        Self {
            tag_column: 0,
            group_tags: true,
            split_buildrequires: true,
            normalize_macros: true,
            global_macros: true,
            max_width: 100,
            sort: true,
            wrap: true,
            reindent: true,
            wrap_description: false,
            convert_paths: false,
            path_macros,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Other = 0,
    Body = 1,
    DescriptionBody = 2,
    FilesBody = 3,
    SectionHeader = 4,
    Tag = 5,
    Skip = 6,
}

const fn is_body(kind: Kind) -> bool {
    matches!(kind, Kind::Body | Kind::DescriptionBody | Kind::FilesBody)
}

/// Format a spec file.
/// Compatibility API: apply all explicitly enabled formatting and safe-fix
/// options. The CLI uses the engine's separate `FormatOnly` and `SafeFix` modes.
///
/// # Errors
///
/// Returns an error when the source cannot be parsed.
pub fn format(source: &str, cfg: &Config) -> Result<String> {
    crate::engine::Engine::new(cfg.clone()).apply(source, crate::engine::FixMode::SafeFix)
}

pub(crate) fn classify(source: &str, tree: &Tree) -> Vec<Kind> {
    if source.is_empty() {
        return Vec::new();
    }
    let lines: Vec<String> = source.lines().map(str::to_string).collect();
    let mut kinds = vec![Kind::Other; lines.len()];
    walk(tree.root_node(), &mut kinds);
    mark_heredocs(&lines, &mut kinds);
    let mut continued = false;
    for (line, kind) in lines.iter().zip(&mut kinds) {
        let next = line.trim_end().ends_with('\\');
        if continued || next {
            *kind = Kind::Skip;
        }
        continued = next;
    }
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    if style::unsafe_preamble(&refs).is_some() {
        kinds.fill(Kind::Skip);
    } else {
        let end = style::preamble_end(&refs);
        if let Some(start) = refs[..end]
            .iter()
            .position(|line| style::executable_macro(line))
        {
            kinds[start..end].fill(Kind::Skip);
        }
    }
    kinds
}

pub(crate) fn render_definitions(source: &str, tree: &Tree) -> String {
    if source.is_empty() {
        return String::new();
    }
    let mut lines: Vec<_> = source.split('\n').map(str::to_string).collect();
    if source.ends_with('\n') {
        lines.pop();
    }
    let rows = classify(source, tree).into_iter().zip(lines).collect();
    let mut out = hoist_definitions(rows)
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n");
    if source.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub(crate) fn render_paths(source: &str, tree: &Tree, cfg: &Config) -> String {
    if source.is_empty() {
        return String::new();
    }
    let mut lines: Vec<_> = source.split('\n').collect();
    if source.ends_with('\n') {
        lines.pop();
    }
    let mut out = classify(source, tree)
        .into_iter()
        .zip(lines)
        .map(|(kind, line)| {
            if kind == Kind::FilesBody {
                convert_paths_in_line(line, &cfg.path_macros)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if source.ends_with('\n') {
        out.push('\n');
    }
    out
}

// Kept as one linear pipeline so the transformation order stays visible.
#[allow(clippy::too_many_lines)]
pub(crate) fn render(source: &str, cfg: &Config, tree: &Tree) -> String {
    if source.is_empty() {
        return String::new();
    }

    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    if source.ends_with('\n') {
        lines.pop();
    }

    let kinds = classify(source, tree);

    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let unsafe_preamble = style::unsafe_preamble(&refs).is_some();
    if unsafe_preamble {
        return source.to_string();
    }
    if cfg.normalize_macros {
        let names: Vec<String> = style::macro_names(&refs)
            .into_iter()
            .map(str::to_string)
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        for (line, kind) in lines.iter_mut().zip(&kinds) {
            if *kind != Kind::Skip {
                *line = style::brace_macros_with_names(line, &names);
            }
        }
    }

    // Group before dependency planning too: tags brought together by grouping
    // must be sorted/wrapped on the first pass, not only on the second pass.
    let mut initial_rows: Vec<_> = kinds.into_iter().zip(lines).collect();
    if cfg.global_macros {
        initial_rows = hoist_definitions(initial_rows);
    }
    if cfg.group_tags {
        initial_rows = group_tags(&initial_rows, cfg);
    }
    let (kinds, lines): (Vec<_>, Vec<_>) = initial_rows.into_iter().unzip();

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
            Kind::FilesBody => {
                let line = if cfg.convert_paths {
                    convert_paths_in_line(line, &cfg.path_macros)
                } else {
                    line.clone()
                };
                line.trim_end().to_string()
            }
            _ => line.trim_end().to_string(),
        };
        rows.push((kind, text));
    }

    let rows = if cfg.wrap_description {
        wrap_descriptions(rows, cfg.max_width)
    } else {
        rows
    };
    let rows = if cfg.group_tags {
        group_tags(&rows, cfg)
    } else {
        rows
    };
    let rows = apply_spacing(rows);
    let mut out: Vec<&str> = rows.iter().map(|(_, text)| text.as_str()).collect();
    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn walk(node: Node<'_>, kinds: &mut [Kind]) {
    if node.is_error() || node.is_missing() {
        mark_skip(kinds, node.start_position().row, node.end_position().row);
        return;
    }

    let kind = node.kind();

    if kind == "macro_definition" {
        let end = node.end_position();
        let last = if end.column == 0 {
            end.row.saturating_sub(1)
        } else {
            end.row
        };
        if last > node.start_position().row {
            // Definitions embedded in an expansion have a different scope.
            // Protect the parent definition and its entire multiline body.
            mark_skip(kinds, node.start_position().row, last);
            return;
        }
    }

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
        let body_kind = match kind {
            "description" => Kind::DescriptionBody,
            "files" => Kind::FilesBody,
            _ => Kind::Body,
        };
        for row in (start + 1)..=last {
            set_kind(kinds, row, body_kind);
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
        if is_body(kinds[i]) {
            if let Some(delim) = heredoc_delim(&lines[i]) {
                let mut j = i + 1;
                while j < lines.len() && is_body(kinds[j]) {
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

pub(crate) fn split_tag(line: &str) -> Option<(String, String)> {
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
    tag.find('(')
        .map_or_else(|| tag.trim_end_matches(':'), |idx| &tag[..idx])
}

fn render_tag(tag: &str, value: &str, cfg: &Config) -> String {
    let tag = tag.trim_end();
    if value.is_empty() {
        return tag.to_string();
    }

    let padding = tag_padding(tag, cfg);
    format!("{tag}{}{value}", " ".repeat(padding))
}

const fn tag_padding(tag: &str, cfg: &Config) -> usize {
    if tag.len() + 1 >= cfg.tag_column {
        1
    } else {
        cfg.tag_column - tag.len()
    }
}

fn token_is_simple(token: &str) -> bool {
    !token.contains(|c: char| "(){}%<>=|'\"#".contains(c))
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
    if !cfg.wrap && !cfg.sort && !cfg.split_buildrequires {
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

        if cfg.split_buildrequires && base_tag(&tag).eq_ignore_ascii_case("BuildRequires") {
            let expressions: Option<Vec<Vec<String>>> =
                values.iter().map(|v| style::dependencies(v)).collect();
            if let Some(expressions) = expressions {
                let mut expressions: Vec<String> = expressions.into_iter().flatten().collect();
                if cfg.sort {
                    expressions.sort_by_key(|v| v.to_lowercase());
                }
                plans[i] = Some(
                    expressions
                        .iter()
                        .map(|v| render_tag(&tag, v, cfg))
                        .collect(),
                );
                for slot in plans.iter_mut().take(j).skip(i + 1) {
                    *slot = Some(Vec::new());
                }
            }
        } else if simple && cfg.wrap && !tokens.is_empty() {
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
        } else if prefix + current.len() + 1 + token.len() <= cfg.max_width {
            current.push(' ');
            current.push_str(token);
        } else {
            out.push(format!("{tag}{gap}{current}"));
            current.clone_from(token);
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

fn hoist_definitions(mut rows: Vec<(Kind, String)>) -> Vec<(Kind, String)> {
    let end = rows
        .iter()
        .position(|(_, line)| style::is_section(line))
        .unwrap_or(rows.len());
    let name = rows[..end]
        .iter()
        .position(|(_, line)| {
            split_tag(line).is_some_and(|(tag, _)| base_tag(&tag).eq_ignore_ascii_case("Name"))
        })
        .unwrap_or(end);
    let mut moved = Vec::new();
    let mut remove = vec![false; rows.len()];
    for i in 0..end {
        if rows[i].0 == Kind::Skip || !style::literal_definition(&rows[i].1) {
            continue;
        }
        let (_, macro_name, _) = style::definition(&rows[i].1).unwrap();
        let duplicate = rows
            .iter()
            .filter(|(_, line)| {
                style::definition(line)
                    .is_some_and(|(_, n, _)| n.split('(').next() == Some(macro_name))
            })
            .count()
            > 1;
        let previously_used = rows[..i].iter().any(|(_, line)| {
            let braced = style::brace_macros_with_names(line, &[macro_name]);
            braced.contains(&format!("%{{{macro_name}}}"))
                || braced.contains(&format!("%{{?{macro_name}"))
                || braced.contains(&format!("%{{!?{macro_name}"))
                || (braced.contains(macro_name)
                    && ["%{defined", "%{undefined", "%{macrobody"]
                        .iter()
                        .any(|prefix| braced.contains(prefix)))
                || braced.contains("%{lua:")
                || braced.contains("%(")
        });
        if duplicate || (i > name && previously_used) {
            continue;
        }
        rows[i].1 = rows[i].1.replacen("%define", "%global", 1);
        if i < name {
            continue;
        }
        let mut start = i;
        while start > name && rows[start - 1].1.trim_start().starts_with('#') {
            start -= 1;
        }
        for j in start..=i {
            remove[j] = true;
            moved.push(rows[j].clone());
        }
    }
    let mut out = Vec::new();
    for (i, row) in rows.into_iter().enumerate() {
        if i == name && !moved.is_empty() {
            out.append(&mut moved);
            out.push((Kind::Other, String::new()));
        }
        if !remove[i] {
            out.push(row);
        }
    }
    out
}

/// Group only plain tag blocks. Comments travel with the following tag;
/// macros, conditionals and parser errors are never crossed.
fn group_tags(rows: &[(Kind, String)], cfg: &Config) -> Vec<(Kind, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        if rows[i].0 != Kind::Tag || split_tag(&rows[i].1).is_none() {
            out.push(rows[i].clone());
            i += 1;
            continue;
        }
        let start = i;
        let mut entries = Vec::new();
        let mut comments = Vec::new();
        let mut last_tag = i;
        while i < rows.len() {
            let (kind, text) = &rows[i];
            if *kind == Kind::Tag {
                let Some((tag, value)) = split_tag(text) else {
                    break;
                };
                entries.push((tag, value, std::mem::take(&mut comments)));
                last_tag = i;
            } else if *kind == Kind::Other && text.trim_start().starts_with('#') {
                comments.push(rows[i].clone());
            } else if *kind != Kind::Other || !text.is_empty() {
                break;
            }
            i += 1;
        }
        // Leave trailing comments and whitespace attached to what follows.
        i = last_tag + 1;
        debug_assert!(i > start);
        entries.sort_by_key(|(tag, _, _)| style::tag_order(tag));
        let mut group = None;
        for (tag, value, comments) in &entries {
            let next_group = style::tag_order(tag).0;
            if group.is_some_and(|g| g != next_group) {
                out.push((Kind::Other, String::new()));
            }
            group = Some(next_group);
            out.extend(comments.iter().cloned());
            let column = if cfg.tag_column == 0 {
                entries
                    .iter()
                    .filter(|(t, _, _)| style::tag_order(t).0 == next_group)
                    .map(|(t, _, _)| t.len() + 1)
                    .max()
                    .unwrap()
            } else {
                cfg.tag_column
            };
            let padding = column.saturating_sub(tag.len()).max(1);
            let text = if value.is_empty() {
                tag.clone()
            } else {
                format!("{tag}{}{value}", " ".repeat(padding))
            };
            out.push((Kind::Tag, text));
        }
    }
    out
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
                if last.0 == Kind::Skip {
                    // Do not insert whitespace inside an opaque macro/error
                    // tail, even if its final rows look like section comments.
                    spaced.push((Kind::Other, String::new()));
                } else if last.1.trim_start().starts_with('#') {
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
        if trimmed[i].1.is_empty() && trimmed[i].0 != Kind::Skip {
            let mut j = i;
            while j < trimmed.len() && trimmed[j].1.is_empty() && trimmed[j].0 != Kind::Skip {
                j += 1;
            }
            let prev_body = out.last().is_some_and(|(kind, _)| is_body(*kind));
            let next_body = trimmed.get(j).is_some_and(|(kind, _)| is_body(*kind));
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

/// Re-flow long lines in `%description` bodies.
///
/// Each line is wrapped on its own, so existing line breaks are never joined.
/// Blank lines, indentation and `*` / `-` / `+` bullets are kept, which also
/// makes the result idempotent.
fn wrap_descriptions(rows: Vec<(Kind, String)>, max_width: usize) -> Vec<(Kind, String)> {
    if max_width == 0 {
        return rows;
    }

    let mut out = Vec::with_capacity(rows.len());
    for (kind, text) in rows {
        if kind != Kind::DescriptionBody || text.trim().is_empty() {
            out.push((kind, text));
            continue;
        }
        for line in wrap_description_line(&text, max_width) {
            out.push((Kind::DescriptionBody, line));
        }
    }
    out
}

fn split_list_bullet(text: &str) -> (&'static str, &str) {
    for bullet in ["* ", "- ", "+ "] {
        if let Some(rest) = text.strip_prefix(bullet) {
            return (bullet, rest);
        }
    }
    ("", text)
}

fn wrap_description_line(line: &str, max_width: usize) -> Vec<String> {
    if line.chars().count() <= max_width {
        return vec![line.to_string()];
    }

    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let (bullet, body) = split_list_bullet(trimmed);

    let first_prefix = format!("{indent}{bullet}");
    let cont_prefix = " ".repeat(first_prefix.chars().count());

    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_prefix = first_prefix;

    for word in body.split_whitespace() {
        let separator = usize::from(!current.is_empty());
        let projected = current_prefix.chars().count()
            + current.chars().count()
            + separator
            + word.chars().count();

        if !current.is_empty() {
            if projected <= max_width {
                current.push(' ');
            } else {
                out.push(format!("{current_prefix}{current}"));
                current_prefix.clone_from(&cont_prefix);
                current.clear();
            }
        }
        current.push_str(word);
    }

    if !current.is_empty() {
        out.push(format!("{current_prefix}{current}"));
    }
    if out.is_empty() {
        vec![line.to_string()]
    } else {
        out
    }
}

/// Replace leading path prefixes in a `%files` line with macros.
fn convert_paths_in_line(line: &str, macros: &[(String, String)]) -> String {
    if line.trim_start().starts_with('#') {
        return line.to_string();
    }

    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find(|c: char| !c.is_whitespace()) {
        let (space, tail) = rest.split_at(start);
        out.push_str(space);

        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let (token, tail) = tail.split_at(end);
        out.push_str(&convert_path_token(token, macros));
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn convert_path_token(token: &str, macros: &[(String, String)]) -> String {
    // Quoted paths may contain spaces, so leave them alone.
    if token.starts_with('"') || token.starts_with('\'') {
        return token.to_string();
    }

    for (from, to) in macros {
        if token == from {
            return to.clone();
        }
        if let Some(suffix) = token.strip_prefix(from) {
            if suffix.starts_with('/') {
                return format!("{to}{suffix}");
            }
        }
    }
    token.to_string()
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
        assert!(out.contains("Packager: Alice <alice@example.com>"), "{out}");
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
            out.contains("BuildRequires: cmake\nBuildRequires: gcc\nBuildRequires: openssl\nBuildRequires: zlib"),
            "{out}"
        );
    }

    #[test]
    fn wraps_long_dependency_lines() {
        let src = "Name: x\n\nBuildRequires: alpha bravo charlie delta echo foxtrot golf hotel india juliet\n";
        let cfg = Config {
            max_width: 40,
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
    fn does_not_sort_or_wrap_inside_braced_dependency_expressions() {
        let source = "Name: x\nRequires: %{?feature:zz aa} 0dep b\n";
        let config = Config {
            max_width: 20,
            ..Config::default()
        };
        let out = format(source, &config).unwrap();
        assert!(
            out.contains("Requires: %{?feature:zz aa} 0dep b\n"),
            "{out}"
        );
        assert_eq!(format(&out, &config).unwrap(), out);
    }

    #[test]
    fn safe_mode_keeps_dependency_order() {
        let src = "Name: x\n\nBuildRequires:  zlib\nBuildRequires:  cmake\n";
        let cfg = Config {
            group_tags: false,
            split_buildrequires: false,
            normalize_macros: false,
            global_macros: false,
            sort: false,
            wrap: false,
            reindent: false,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert_eq!(
            out,
            "Name: x\n\nBuildRequires: zlib\nBuildRequires: cmake\n"
        );
    }

    #[test]
    fn adds_blank_line_before_sections_only() {
        let src = "Name: x\n%description\nText\n%files\n/foo\n";
        let out = format(src, &Config::default()).unwrap();
        assert_eq!(out, "Name: x\n\n%description\nText\n\n%files\n/foo\n");
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
        assert_eq!(out, "Name: x\n");
    }

    #[test]
    fn wraps_description_lines() {
        let src = "Name: x\n\n%description\nThis is a very long line of text that should be wrapped at a narrow width.\n\n* A bullet item that is also long enough to wrap onto a second line.\n";
        let cfg = Config {
            max_width: 40,
            wrap_description: true,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        for line in out.lines() {
            assert!(line.chars().count() <= 40, "{line}");
        }
        assert!(out.contains("This is a very long line of text that"));
        assert!(out.contains("\n\n* A bullet item"), "{out}");
        assert_eq!(format(&out, &cfg).unwrap(), out);
    }

    #[test]
    fn description_wrapping_is_off_by_default() {
        let src = "Name: x\n\n%description\nThis is a very long line of text that should stay on one line.\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(out.contains("This is a very long line of text that should stay on one line."));
    }

    #[test]
    fn converts_paths_in_files_only() {
        let src = "Name: x\n\n%description\n/usr/share/keep\n\n%files\n/usr/bin/foo\n%{_datadir}/already\n/usr/lib/systemd/system/foo.service\n/etc/foo.conf\n/usr/share/doc/pkg/README\n/usr/share/man/man1/foo.1\n/usr/lib64/pkgconfig/foo.pc\n/var/lib/foo\n/usr/local/foo\n/usr/sharefoo/keep\n\n%changelog\n* Sat Nov 07 2026 Tester <t@example.com> - 1\n- Initial\n";
        let cfg = Config {
            convert_paths: true,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert!(out.contains("%{_bindir}/foo"), "{out}");
        assert!(out.contains("%{_datadir}/already"), "{out}");
        assert!(out.contains("%{_unitdir}/foo.service"), "{out}");
        assert!(out.contains("%{_sysconfdir}/foo.conf"), "{out}");
        assert!(out.contains("%{_docdir}/pkg/README"), "{out}");
        assert!(out.contains("%{_mandir}/man1/foo.1"), "{out}");
        assert!(out.contains("%{_pkgconfigdir}/foo.pc"), "{out}");
        assert!(out.contains("%{_sharedstatedir}/foo"), "{out}");
        assert!(out.contains("%{_prefix}/local/foo"), "{out}");
        // The /usr/share prefix must not match /usr/sharefoo.
        assert!(out.contains("%{_prefix}/sharefoo/keep"), "{out}");
        assert!(!out.contains("%{_datadir}sharefoo"), "{out}");
        assert!(out.contains("%description\n/usr/share/keep"), "{out}");
        assert_eq!(format(&out, &cfg).unwrap(), out);
    }

    #[test]
    fn keeps_comments_and_quoted_paths_in_files() {
        let src =
            "Name: x\n\n%files\n# /usr/share/comment\n\"/usr/share/with space\"\n/usr/bin/foo\n";
        let cfg = Config {
            convert_paths: true,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert!(out.contains("# /usr/share/comment"), "{out}");
        assert!(out.contains("\"/usr/share/with space\""), "{out}");
        assert!(out.contains("%{_bindir}/foo"), "{out}");
    }

    #[test]
    fn groups_and_aligns_preamble() {
        let src = "BuildOption(conf): a\nRequires: a\nPatch0: a\nName: 1\nURL: https://example.com\nVersion: 1\nRelease: 1\nSummary: a\nLicense: a\nPackager: a\nSource0: a\nProvides: a\nSupplements: a\nEnhances: a\nRecommends: a\nObsoletes: a\nConflicts: a\nBuildRequires: a\nBuildSystem: a\n";
        let out = format(src, &Config::default()).unwrap();
        assert_eq!(out, "Name:     1\nVersion:  1\nRelease:  1\nSummary:  a\nLicense:  a\nURL:      https://example.com\nPackager: a\n\nSource0: a\nPatch0:  a\n\nProvides:    a\nSupplements: a\nEnhances:    a\nRecommends:  a\nObsoletes:   a\nConflicts:   a\nRequires:    a\n\nBuildRequires: a\n\nBuildSystem:       a\nBuildOption(conf): a\n");
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn splits_versioned_and_rich_buildrequires() {
        let src = "Name: x\nBuildRequires: zlib >= 1.2, pkgconfig(foo) (bar if baz) gcc\n";
        let out = format(src, &Config::default()).unwrap();
        assert_eq!(out, "Name: x\n\nBuildRequires: (bar if baz)\nBuildRequires: gcc\nBuildRequires: pkgconfig(foo)\nBuildRequires: zlib >= 1.2\n");
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn grouping_dependencies_is_idempotent_when_interleaved() {
        let src = "Name: x\nRequires: zlib\nBuildRequires: zlib >= 1\nProvides: y\nRequires: abc\nBuildRequires: gcc\nSource10: x\nSource2: y\n";
        let cfg = Config::default();
        let out = format(src, &cfg).unwrap();
        assert!(out.find("Source2:").unwrap() < out.find("Source10:").unwrap());
        assert_eq!(format(&out, &cfg).unwrap(), out);
    }

    #[test]
    fn unsafe_preambles_are_not_changed() {
        for directive in ["%gometa", "%electron_meta", "%if 0%{?fedora}"] {
            let src = format!("Name:  x  \n\n\n{directive}\nBuildRequires: gcc cmake\n\n%build\n\techo %_bindir  \n");
            assert_eq!(format(&src, &Config::default()).unwrap(), src);
        }
    }

    #[test]
    fn formats_tags_before_trailing_macros_without_changing_the_tail() {
        let tail = "%electronmeta -D  \n%another_macro %_bindir\n%define feature 1\n\n";
        let src = format!("Name:           discord\nVersion:        1\nSource0:        https://example.com/discord.tar.gz\nLicense:        MIT\nRequires:       zenity\n\n{tail}%description\nExample\n");
        let out = format(&src, &Config::default()).unwrap();
        assert!(out.starts_with("Name:    discord\nVersion: 1\nLicense: MIT\n\nSource0: https://example.com/discord.tar.gz\n\nRequires: zenity\n"), "{out}");
        assert!(out.contains(&format!("{tail}%description")), "{out}");
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn conditionals_remain_unsafe_even_after_the_last_tag() {
        let src = "Name: x\nLicense: MIT\n%if 0%{?fedora}\n%electronmeta -D\n%endif\n\n%description\nExample\n";
        assert_eq!(format(src, &Config::default()).unwrap(), src);
    }

    #[test]
    fn preserves_comments_inside_a_trailing_macro_block() {
        let source = "Name: x\nLicense: MIT\n%electronmeta -D\n# opaque tail comment\n%description\nExample\n";
        let out = format(source, &Config::default()).unwrap();
        assert!(
            out.contains("%electronmeta -D\n# opaque tail comment\n"),
            "{out}"
        );
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn braces_macro_references_without_changing_calls_or_escapes() {
        let src = "Name: x\nRelease: 1%?dist\n\n%install\nmkdir -p %buildroot%_bindir\n%make_install\nprintf '%%s %s' foo\n\n%files\n%_bindir/x\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(out.contains("1%{?dist}"), "{out}");
        assert!(out.contains("%{buildroot}%{_bindir}"), "{out}");
        assert!(out.contains("%make_install\nprintf '%%s %s' foo"), "{out}");
        assert!(out.contains("%{_bindir}/x"), "{out}");
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn hoists_literal_definitions_and_keeps_their_comments() {
        let src = "Name: x\nVersion: 1\n# feature switch\n%define feature 1\n%global other 2\nRelease: 1\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.starts_with("# feature switch\n%global feature 1\n%global other 2\n\nName:"),
            "{out}"
        );
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn leaves_definitions_with_expansion_or_parameters_in_place() {
        let src = "Name: x\n%define delayed %{version}\n%define helper() echo %1\n%global based %{name}\nVersion: 1\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.find("Name:").unwrap() < out.find("%define delayed").unwrap(),
            "{out}"
        );
        assert!(out.contains("%define helper() echo %1"), "{out}");
        assert!(out.contains("%global based %{name}"), "{out}");
    }

    #[test]
    fn parameterized_redefinitions_are_not_braced_or_hoisted() {
        let source = "Name: x\nLicense: MIT\n%define foo 1\n%define foo() echo %1\n%define _bindir() echo %1\n\n%build\n%foo arg\n%_bindir arg\n";
        let out = format(source, &Config::default()).unwrap();
        assert!(
            out.contains("%define foo 1\n%define foo() echo %1"),
            "{out}"
        );
        assert!(out.contains("%foo arg\n%_bindir arg"), "{out}");
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn does_not_hoist_across_macro_introspection_or_rewrite_nested_definitions() {
        let source = "Name: x\nVersion: %{defined feature}\n%define feature 1\n";
        let out = format(source, &Config::default()).unwrap();
        assert!(
            out.find("Version:").unwrap() < out.find("%define feature").unwrap(),
            "{out}"
        );
        let nested = "%global body %{expand:\n%define local 1\n}\nName: x\n";
        let out = format(nested, &Config::default()).unwrap();
        assert!(
            out.contains("%global body %{expand:\n%define local 1\n}"),
            "{out}"
        );
    }

    #[test]
    fn safe_mode_does_not_apply_new_semantic_changes() {
        let src = "Name: x\nBuildRequires: zlib gcc\n%define feature 1\nRelease: 1%?dist\n";
        let cfg = Config {
            group_tags: false,
            split_buildrequires: false,
            normalize_macros: false,
            global_macros: false,
            sort: false,
            wrap: false,
            reindent: false,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert!(out.contains("BuildRequires: zlib gcc"));
        assert!(out.contains("%define feature 1"));
        assert!(out.contains("1%?dist"));
        assert_eq!(strip_ws(src), strip_ws(&out));
    }

    #[test]
    fn keeps_comments_with_their_tags() {
        let src = "Name: x\n# build tool\nBuildRequires: gcc\n# archive\nSource0: x\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.contains("# archive\nSource0: x\n\n# build tool\nBuildRequires: gcc"),
            "{out}"
        );
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn leaves_continued_tags_together() {
        let src = "Name: x\nSource0: x\nBuildRequires: gcc \\\n  cmake\nVersion: 1\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(out.contains("BuildRequires: gcc \\\n  cmake\n"), "{out}");
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }

    #[test]
    fn does_not_hoist_previously_used_or_redefined_macros() {
        let src = "Name: x\nVersion: %feature\n%global feature 1\n%define repeated 1\n%define repeated 2\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.find("Version:").unwrap() < out.find("%global feature").unwrap(),
            "{out}"
        );
        assert!(out.contains("%{feature}"), "{out}");
        assert!(
            out.contains("%define repeated 1\n%define repeated 2"),
            "{out}"
        );
        assert_eq!(format(&out, &Config::default()).unwrap(), out);
    }
}
