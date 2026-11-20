use std::ops::Range;
use std::sync::{Arc, OnceLock};

use stable_eyre::eyre::Result;
use tree_sitter::Tree;

use crate::diagnostic::{Edit, Fix};
use crate::format::{self, Kind};
use crate::{parse, rpm, style, Config};

#[derive(Debug, Clone)]
pub struct Line<'src> {
    pub number: usize,
    pub span: Range<usize>,
    pub text: &'src str,
    pub editable: bool,
}

/// A borrowed tag in a preamble/package tag region. These are parsing facts,
/// not lint findings, so they remain usable on partially erroneous trees.
#[derive(Debug, Clone)]
pub struct Tag<'src> {
    pub line: usize,
    pub key: &'src str,
    pub value: &'src str,
    pub value_offset: usize,
}

impl Tag<'_> {
    #[must_use]
    pub fn name(&self) -> &str {
        self.key.split('(').next().unwrap_or(self.key)
    }
}

/// One parse per source revision. Text checks remain available on ERROR trees.
pub struct Context<'src> {
    pub source: &'src str,
    pub tree: Tree,
    pub lines: Vec<Line<'src>>,
    pub config: &'src Config,
    pub unsafe_preamble: bool,
    pub unsafe_preamble_line: Option<usize>,
    pub preamble_end: usize,
    pub name_line: usize,
    pub tags: Vec<Tag<'src>>,
    tags_by_line: Vec<Option<usize>>,
    pub(crate) macro_declarations: Vec<&'src str>,
    formatted: OnceLock<Arc<str>>,
    definitions: OnceLock<Arc<str>>,
    paths: OnceLock<Arc<str>>,
}

impl<'src> Context<'src> {
    /// Parse one source revision and collect structural facts.
    ///
    /// # Errors
    ///
    /// Returns an error when the tree-sitter parser cannot be initialized.
    pub fn new(source: &'src str, config: &'src Config) -> Result<Self> {
        // A missing final newline should not turn an otherwise valid section
        // into a MISSING-at-EOF section. Offsets still refer to the original.
        let parse_source = if source.ends_with('\n') {
            std::borrow::Cow::Borrowed(source)
        } else {
            std::borrow::Cow::Owned(format!("{source}\n"))
        };
        let tree = parse::parse(&parse_source)?;
        let kinds = format::classify(source, &tree);
        let mut offset = 0;
        let lines: Vec<_> = source
            .split_inclusive('\n')
            .enumerate()
            .map(|(index, chunk)| {
                let text = chunk.strip_suffix('\n').unwrap_or(chunk);
                let text = text.strip_suffix('\r').unwrap_or(text);
                let line = Line {
                    number: index + 1,
                    span: offset..offset + text.len(),
                    text,
                    editable: kinds.get(index).is_some_and(|kind| *kind != Kind::Skip),
                };
                offset += chunk.len();
                line
            })
            .collect();
        let refs: Vec<_> = lines.iter().map(|line| line.text).collect();
        let macro_declarations = style::macro_names(&refs);
        let unsafe_preamble_line = style::unsafe_preamble(&refs);
        let unsafe_preamble = unsafe_preamble_line.is_some();
        let preamble_end = style::preamble_end(&refs);
        tracing::debug!(
            lines = lines.len(),
            parse_errors = tree.root_node().has_error(),
            unsafe_preamble,
            "parsed spec"
        );
        let mut tags = Vec::new();
        let mut tags_by_line = vec![None; lines.len()];
        let mut in_tags = true;
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.text.trim_start();
            if trimmed.starts_with('%') {
                let word = style::section_word(trimmed);
                if word == "package" {
                    in_tags = true;
                } else if rpm::BODY_SECTIONS.contains(&word) || word == "autochangelog" {
                    in_tags = false;
                }
                continue;
            }
            if in_tags {
                if let Some((key, value, offset)) = style::split_tag_line(trimmed) {
                    tags_by_line[index] = Some(tags.len());
                    tags.push(Tag {
                        line: index,
                        key,
                        value,
                        value_offset: line.span.start + line.text.len() - trimmed.len() + offset,
                    });
                }
            }
        }
        let name_line = tags
            .iter()
            .find(|tag| tag.line < preamble_end && tag.name() == "Name")
            .map_or(preamble_end, |tag| tag.line);
        Ok(Self {
            source,
            tree,
            lines,
            config,
            unsafe_preamble,
            unsafe_preamble_line,
            preamble_end,
            name_line,
            tags,
            tags_by_line,
            macro_declarations,
            formatted: OnceLock::new(),
            definitions: OnceLock::new(),
            paths: OnceLock::new(),
        })
    }

    pub(crate) fn formatting_target(&self, tree: &Tree) -> &Arc<str> {
        if self.formatted.get().is_none() {
            let mut cfg = self.config.clone();
            cfg.global_macros = false;
            cfg.convert_paths = false;
            let target = format::render(self.source, &cfg, tree);
            let _ = self.formatted.set(target.into());
        }
        self.formatted.get().expect("formatting target initialized")
    }

    pub(crate) fn definition_target(&self, tree: &Tree) -> &Arc<str> {
        self.definitions
            .get_or_init(|| format::render_definitions(self.source, tree).into())
    }

    pub(crate) fn path_target(&self) -> &Arc<str> {
        self.paths
            .get_or_init(|| format::render_paths(self.source, &self.tree, self.config).into())
    }

    pub(crate) fn replacement(&self, target: &Arc<str>) -> Option<Fix> {
        (target.as_ref() != self.source).then(|| Fix {
            edits: vec![Edit {
                range: 0..self.source.len(),
                replacement: Arc::clone(target),
            }],
        })
    }

    pub fn tag_at(&self, number: usize) -> Option<&Tag<'src>> {
        self.tags_by_line
            .get(number.checked_sub(1)?)
            .copied()
            .flatten()
            .map(|index| &self.tags[index])
    }

    pub fn preamble_tags(&self) -> impl Iterator<Item = &Tag<'src>> {
        self.tags
            .iter()
            .take_while(|tag| tag.line < self.preamble_end)
    }

    /// Safely anchor a diagnostic at a source byte offset.
    pub fn point_span(&self, offset: usize) -> Range<usize> {
        let mut start = offset.min(self.source.len());
        while !self.source.is_char_boundary(start) {
            start -= 1;
        }
        let end = start
            + self.source[start..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
        start..end
    }

    pub fn location(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.source.len());
        if offset == self.source.len() && self.source.ends_with('\n') {
            return (self.lines.len() + 1, 1);
        }
        let line = self
            .lines
            .iter()
            .rev()
            .find(|line| line.span.start <= offset);
        line.map_or((1, 1), |line| {
            let mut end = offset.min(line.span.end);
            while !self.source.is_char_boundary(end) {
                end -= 1;
            }
            (
                line.number,
                self.source[line.span.start..end].chars().count() + 1,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_locations_keep_utf8_byte_ranges() {
        let config = Config::default();
        let context = Context::new("Name: 日本語\r\nLicense: MIT\r\n", &config).unwrap();
        assert_eq!(context.lines[0].text, "Name: 日本語");
        assert_eq!(context.location(6), (1, 7));
        assert_eq!(context.location(7), (1, 7));
        assert_eq!(context.location(context.source.len()), (3, 1));
    }

    #[test]
    fn construction_only_prepares_source_facts() {
        let config = Config::default();
        let context =
            Context::new("Name: x\nRelease: 1%?dist\n%define feature 1\n", &config).unwrap();
        assert_eq!(context.tags.len(), 2);
        assert_eq!(context.name_line, 0);
        assert!(context.macro_declarations.contains(&"feature"));
        assert!(context.formatted.get().is_none());
        assert!(context.definitions.get().is_none());
        assert!(context.paths.get().is_none());
    }
}
