# Upstream

- Source: https://github.com/ivov/lisette/tree/0c74a0f/editors/tree-sitter-lisette
- Synced commit: `0c74a0f` (2026-08-19, "feat: parse write permission in editor grammar")
- No crates.io release, so the grammar is vendored here.

## Local differences

- `src/parser.c` is regenerated at ABI 14 (upstream ships ABI 15) with tree-sitter CLI 0.23, to match the 0.23 runtime.
- `src/node-types.json` comes from the same regeneration, so it lacks newer CLI metadata fields (`root`, `extra`).
- `Cargo.toml`, `build.rs`, `bindings/rust/lib.rs` are ours. `build.rs` silences unused-parameter warnings from the upstream C sources.
- `queries/highlights.scm`, `src/scanner.c` are verbatim upstream.

## Sync

```bash
git clone --filter=blob:none --sparse https://github.com/ivov/lisette.git /tmp/lisette
cd /tmp/lisette && git sparse-checkout set editors/tree-sitter-lisette
git log --oneline 0c74a0f..HEAD -- editors/tree-sitter-lisette   # anything new?

cd editors/tree-sitter-lisette
npx tree-sitter-cli@0.23 generate --abi 14

V=<repo>/vendor/tree-sitter-lisette
cp queries/highlights.scm "$V/queries/"
cp src/parser.c src/scanner.c src/node-types.json "$V/src/"
cp src/tree_sitter/*.h "$V/src/tree_sitter/"
```

Then run `cargo test` in the repo and update the synced commit above.
