# Upstream

- Source: https://github.com/tree-sitter-grammars/tree-sitter-linkerscript/tree/f99011a
- Synced commit: `f99011a` (2023-11-19, release 1.0.0)
- The crates.io release pins tree-sitter 0.20 / cc 1.0, which conflicts with this repo, so the grammar is vendored here.

## Local differences

- `Cargo.toml`, `build.rs`, `bindings/rust/lib.rs` are ours (`tree-sitter-language` style bindings). `build.rs` turns off cc's default warning flags (`warnings(false)`) for the generated parser, so it builds quietly on GCC, Clang and MSVC.
- `src/` and `queries/highlights.scm` are verbatim upstream (already ABI 14, no regeneration needed; verified byte-identical to `f99011a`).

## Sync

```bash
set -euo pipefail
V=<repo>/vendor/tree-sitter-linkerscript
T=$(mktemp -d)
git clone --filter=blob:none https://github.com/tree-sitter-grammars/tree-sitter-linkerscript.git "$T/ts-ld"
git -C "$T/ts-ld" log --oneline f99011a..HEAD -- grammar.js src queries   # anything new?

cp "$T/ts-ld/queries/highlights.scm" "$V/queries/"
cp "$T/ts-ld/src/parser.c" "$T/ts-ld/src/node-types.json" "$V/src/"
cp "$T/ts-ld/src/tree_sitter/parser.h" "$V/src/tree_sitter/"
```

If upstream `parser.c` moves past ABI 14, regenerate it instead of copying: in an empty directory run `npx tree-sitter-cli@0.23 generate "$T/ts-ld/src/grammar.json" --abi 14` and copy `src/parser.c`, `src/node-types.json`, `src/tree_sitter/*.h` from there. Then run `cargo test` in the repo and update the synced commit above.
