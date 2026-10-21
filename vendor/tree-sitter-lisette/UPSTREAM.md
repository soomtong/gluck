# Upstream

- Source: https://github.com/ivov/lisette/tree/0c74a0f/editors/tree-sitter-lisette
- Synced commit: `0c74a0f` (2026-08-19, "feat: parse write permission in editor grammar") — the last commit touching `editors/tree-sitter-lisette`. The repo HEAD at sync time was `33edb94`; its grammar directory is identical. `0c74a0f` is the reference to update.
- No crates.io release, so the grammar is vendored here.

## Local differences

- `src/parser.c` and `src/tree_sitter/*.h` are regenerated at ABI 14 (upstream ships ABI 15) from the upstream `src/grammar.json` with tree-sitter CLI 0.23, to match the 0.23 runtime. Re-running the sync below reproduces them byte for byte.
- `src/node-types.json` comes from the same regeneration, so it lacks newer CLI metadata fields (`root`, `extra`).
- `Cargo.toml`, `build.rs`, `bindings/rust/lib.rs` are ours. `build.rs` turns off cc's default warning flags (`warnings(false)`) for the generated/upstream C, and adds `-utf-8` on MSVC because `scanner.c` has non-ASCII comments.
- `queries/highlights.scm`, `src/scanner.c` are verbatim upstream.

## Sync

Generating from `grammar.json` (not `grammar.js`) keeps the result deterministic: the CLI doesn't evaluate the JS grammar DSL, so a newer upstream CLI pin doesn't matter.

```bash
set -euo pipefail
V=<repo>/vendor/tree-sitter-lisette
T=$(mktemp -d)
git clone --filter=blob:none --sparse https://github.com/ivov/lisette.git "$T/lisette"
git -C "$T/lisette" sparse-checkout set editors/tree-sitter-lisette
git -C "$T/lisette" log --oneline 0c74a0f..HEAD -- editors/tree-sitter-lisette   # anything new?
G="$T/lisette/editors/tree-sitter-lisette"

OUT=$(mktemp -d)
(cd "$OUT" && npx tree-sitter-cli@0.23 generate "$G/src/grammar.json" --abi 14)

cp "$G/queries/highlights.scm" "$V/queries/"
cp "$G/src/scanner.c" "$V/src/"
cp "$OUT/src/parser.c" "$OUT/src/node-types.json" "$V/src/"
cp "$OUT/src/tree_sitter/"*.h "$V/src/tree_sitter/"
```

Then run `cargo test` in the repo and update the synced commit above (here and in `bindings/rust/lib.rs`).
