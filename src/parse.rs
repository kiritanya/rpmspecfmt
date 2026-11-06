use anyhow::{Context, Result};
use tree_sitter::{Parser, Tree};

/// Parse an RPM spec file with the vendored (forked) rpmspec grammar.
pub fn parse(source: &str) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rpmspec::LANGUAGE.into())
        .context("failed to load the rpmspec grammar")?;
    parser
        .parse(source, None)
        .context("failed to parse the spec file")
}
