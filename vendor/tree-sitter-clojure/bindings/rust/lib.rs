//! Clojure language support for the tree-sitter parsing library.
//!
//! Vendored from <https://github.com/sogaiu/tree-sitter-clojure> (rev e43eff8, see `UPSTREAM.md` for the
//! sync steps). Upstream has no Rust bindings or crates.io release. The C
//! sources and `queries/` are verbatim upstream copies; the upstream query
//! only covers literals and comments, so gluck layers its own patterns on top.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_clojure() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_clojure) };

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
            .expect("Error loading Clojure grammar");
    }
}
