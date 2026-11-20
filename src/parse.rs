use stable_eyre::eyre::{eyre, Result, WrapErr};
use tree_sitter::{Parser, Tree};

/// Parse an RPM spec file with the vendored (forked) rpmspec grammar.
pub fn parse(source: &str) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rpmspec::LANGUAGE.into())
        .wrap_err("failed to load the rpmspec grammar")?;
    parser
        .parse(source, None)
        .ok_or_else(|| eyre!("failed to parse the spec file"))
}
