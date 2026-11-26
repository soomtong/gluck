# Upstream

- Source: https://github.com/soomtong/cyber/tree/d4dedbb4/exts/tree-sitter
- Synced commit: `d4dedbb4` (2026-11-25, "Let for take an iterator that can fail, with a required else block") — the last commit touching `exts/tree-sitter`.
- No crates.io release, and upstream gitignores the generated parser, so the grammar is vendored here with a generated `src/`.

## Local differences

- `src/parser.c`, `src/node-types.json`, `src/tree_sitter/*.h` are generated from upstream `grammar.js` with tree-sitter CLI 0.27 (ABI 15, matching the 0.27 runtime).
- `Cargo.toml`, `build.rs`, `bindings/rust/lib.rs` are ours. `build.rs` turns off cc's default warning flags (`warnings(false)`) for the generated/upstream C, and adds `-utf-8` on MSVC.
- `queries/highlights.scm`, `src/scanner.c`, `LICENSE` are verbatim upstream.

## Sync

```bash
set -euo pipefail
V=<repo>/vendor/tree-sitter-cyber
G=<cyber repo>/exts/tree-sitter
git -C "$G" log --oneline d4dedbb4..HEAD -- .   # anything new?
(cd "$G" && tree-sitter generate)

cp "$G/queries/highlights.scm" "$V/queries/"
cp "$G/src/scanner.c" "$G/src/parser.c" "$G/src/node-types.json" "$V/src/"
cp "$G/src/tree_sitter/"*.h "$V/src/tree_sitter/"
```

Then run `cargo test` in the repo and update the synced commit above (here and in `bindings/rust/lib.rs`).
