//! Lisette language support for the tree-sitter parsing library.
//!
//! Vendored from <https://github.com/ivov/lisette> `editors/tree-sitter-lisette`
//! (rev 33edb94a613d0fe9060973c77ebc01a1e6809130). Upstream ships a parser
//! generated at tree-sitter ABI 15, which this project's tree-sitter 0.23
//! runtime cannot load — `src/parser.c` here is regenerated from the upstream
//! `grammar.json` with tree-sitter CLI 0.23 (ABI 14; the grammar's `reserved`
//! field is an empty stub upstream, so nothing is lost). `scanner.c` and
//! `queries/` are verbatim upstream copies.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_lisette() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_lisette) };

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
            .expect("Error loading lisette grammar");
    }
}
