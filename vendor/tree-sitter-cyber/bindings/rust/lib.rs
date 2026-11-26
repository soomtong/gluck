//! Cyber language support for the tree-sitter parsing library.
//!
//! Vendored from <https://github.com/soomtong/cyber> `exts/tree-sitter`
//! (rev d4dedbb4, see `UPSTREAM.md` for the sync steps). Upstream doesn't
//! commit the generated parser, so `src/parser.c` and friends are generated
//! here with tree-sitter CLI 0.27 (ABI 15). `scanner.c` and `queries/` are
//! verbatim upstream copies.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_cyber() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_cyber) };

/// The content of the [`node-types.json`] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers#static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// The syntax highlighting query for this grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");
