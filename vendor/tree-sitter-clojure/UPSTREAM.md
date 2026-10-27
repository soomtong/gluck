# Upstream

- Source: https://github.com/sogaiu/tree-sitter-clojure
- Synced commit: `e43eff8` (2025-08-26, "Update README")
- No crates.io release with Rust bindings, so the grammar is vendored here.

## Local differences

- `Cargo.toml`, `build.rs`, `bindings/rust/lib.rs` are ours. `build.rs` turns off cc's default warning flags (`warnings(false)`) for the generated C.
- `src/`, `queries/highlights.scm` are verbatim upstream (parser generated at ABI 14, inside the runtime's 13–15 window). `LICENSE` is upstream `COPYING.txt` (CC0 1.0).
- The upstream query only highlights literals and comments; special forms, definitions and call heads come from gluck's own patterns in `src/highlight/engine.rs`.

## Sync

```bash
set -euo pipefail
V=<repo>/vendor/tree-sitter-clojure
T=$(mktemp -d)
git clone https://github.com/sogaiu/tree-sitter-clojure.git "$T"
git -C "$T" log --oneline e43eff8..HEAD   # anything new?
grep -o 'LANGUAGE_VERSION [0-9]*' "$T/src/parser.c"   # must stay within 13–15
cp "$T/src/parser.c" "$T/src/node-types.json" "$V/src/"
cp "$T/src/tree_sitter/"*.h "$V/src/tree_sitter/"
cp "$T/queries/highlights.scm" "$V/queries/"
```

Then run `cargo test` in the repo and update the synced commit above (here and in `bindings/rust/lib.rs`).
