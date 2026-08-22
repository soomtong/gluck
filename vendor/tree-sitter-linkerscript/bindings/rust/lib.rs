//! GNU linker script (ld) language support for the tree-sitter parsing library.
//!
//! Vendored from <https://github.com/tree-sitter-grammars/tree-sitter-linkerscript>
//! (rev f99011a3554213b654985a4b0a65b3b032ec4621) because the crates.io release
//! pins tree-sitter 0.20 / cc 1.0, which conflicts with the rest of this
//! project's dependency tree. Only the Rust bindings are rewritten here to the
//! modern `tree-sitter-language` interface; `src/` and `queries/` are verbatim
//! upstream copies.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_linkerscript() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_linkerscript) };

/// The content of the [`node-types.json`] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers#static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// The syntax highlighting query for this grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

#[cfg(test)]
mod tests {
    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading linkerscript grammar");
    }
}
