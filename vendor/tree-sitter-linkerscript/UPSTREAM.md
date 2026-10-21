# Upstream

- Source: https://github.com/tree-sitter-grammars/tree-sitter-linkerscript/tree/f99011a
- Synced commit: `f99011a` (2023-11-19, release 1.0.0)
- The crates.io release pins tree-sitter 0.20 / cc 1.0, which conflicts with this repo, so the grammar is vendored here.

## Local differences

- `Cargo.toml`, `build.rs`, `bindings/rust/lib.rs` are ours (`tree-sitter-language` style bindings). `build.rs` silences unused-variable warnings from the generated parser.
- `src/` and `queries/highlights.scm` are verbatim upstream (already ABI 14, no regeneration needed).

## Sync

```bash
git clone --filter=blob:none https://github.com/tree-sitter-grammars/tree-sitter-linkerscript.git /tmp/ts-ld
cd /tmp/ts-ld && git log --oneline f99011a..HEAD -- grammar.js src queries   # anything new?

V=<repo>/vendor/tree-sitter-linkerscript
cp queries/highlights.scm "$V/queries/"
cp src/parser.c src/node-types.json "$V/src/"
cp src/tree_sitter/parser.h "$V/src/tree_sitter/"
```

If upstream `parser.c` moves past ABI 14, regenerate it with `npx tree-sitter-cli@0.23 generate --abi 14` instead of copying. Then run `cargo test` in the repo and update the synced commit above.
