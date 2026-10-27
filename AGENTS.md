# AGENTS.md — gluck

Development guide for coding agents: invariants and traps that aren't obvious from the code. Features, key bindings and architecture overview live in `README.md`. When in doubt, prefer executable truth over prose.

## Basics

- Crate `gluck`, binary `glc`. Single crate, edition 2021. Not published to crates.io, so git deps are fine.
- Release tags `v*`; version in `Cargo.toml`.

## Layout

```
src/
├── main.rs / app.rs            # event loop / central App (state, key + mouse dispatch, render)
├── mode.rs                     # Mode enum, key bindings, actions
├── cli.rs / config.rs          # clap CLI / ~/.config/gluck/config.toml
├── theme.rs                    # palettes, highlight map, capture-name aliases
├── lang.rs                     # Language detection (file name, then extension)
├── selection.rs                # mouse line selection + per-frame content row map
├── git/                        # repo, commit, store (200-batch), tree, diff, cache
├── highlight/engine.rs         # tree-sitter-highlight configs + span builder
├── ask/                        # LLM explain: provider, client (SSE), prompt, context, cache
├── search/                     # BM25 + vector + RRF engine, indexer, chunk/, typo, glcignore, report/
└── ui/                         # pick, view, diff, diff_words, wrap, repaint, search_modal, ask_panel
vendor/                         # vendored tree-sitter grammars (each with UPSTREAM.md)
examples/                       # *_probe.rs diagnostics
```

## Commands

```bash
cargo run --bin glc -- [PATH]                    # run TUI (subcommands: index, ignore, report)
cargo test [name]
cargo clippy --all-targets -- -D warnings        # same as CI
cargo fmt --all                                  # CI runs cargo fmt --all -- --check
cargo run --bin glc -- report [--out FILE.md]    # search quality regression check
```

## Tests

- Git helpers in `src/git/repo.rs` `#[cfg(test)] pub mod tests`: `init_test_repo()` → `(TempDir, Repository)`, `add_file_commit(&repo, path, content, msg)`.
- Highlight: `assert_span_colors(path, source, &[(text, color)])` in `highlight/engine.rs`, against `Palette::plain()`.
- No external services, no snapshots, no expensive suites. App tests resolve Ask providers with an empty env.

## Gotchas

### Syntax highlighting

- Adding a grammar: check `LANGUAGE_VERSION` in its `src/parser.c` is within the runtime's ABI 13–15, and that it depends on `tree-sitter-language`, not `tree-sitter` (that links a second runtime). Then assert real colors with `assert_span_colors` — a config that fails to build is silently skipped by `register_languages`. Update the README language list.
- `HighlightConfiguration::new()` is `(language, name, highlights, injections, locals)`; don't put a query in the `name` slot. Injections are never resolved — pass `""`.
- `HIGHLIGHT_NAMES` drives every config. A capture resolves to the matching name with the most dot-parts, ties to the earlier entry. A capture name that should reuse an existing style goes in both `HIGHLIGHT_NAMES` and `theme.rs::HIGHLIGHT_ALIASES` (a test requires a style for every name).
- Same node captured twice → the later pattern wins, even an unstyled `@_name` capture; capture predicate subjects with a real name. `engine.rs::layered()` appends gluck patterns to upstream queries (C under C++, Kotlin, LLVM, PHP, Clojure, Janet).
- Unknown predicates are ignored, not rejected, so nvim-targeted queries load but over-match — see `odin_highlights()`.
- Captures nest; `highlight()` keeps a style stack so ending an inner capture restores the outer style.
- Non-crates.io grammars: git deps pinned by `rev` (`tree-sitter-gleam`, `tree-sitter-kotlin` from fwcd) and vendored crates in `vendor/` (linkerscript, lisette, clojure, janet-simple). Sync steps live in each `UPSTREAM.md`. Vendored `build.rs` use `cc` `warnings(false)` (per-flag `-Wno-*` is dropped by MSVC). They aren't workspace members, so `cargo test -p` can't run them.

### View / rendering

- `FileContent::Text` needs both `raw` and `highlighted`; setting only `raw` silently renders plain text.
- `ViewState.selected_file` indexes `visible` (fold-filtered), not `tree` — use `selected_entry()`/`select_path()`, and call `rebuild_visible()` after mutating `tree` or `collapsed`.
- j/k defer loading via `pending_view_load` + `VIEW_LOAD_DEBOUNCE`; `Enter`/commit-switch paths call `load_view_file()` directly.
- `content_cache` is keyed `(commit oid, path)` and cleared on theme cycle. Files over `MAX_HIGHLIGHT_BYTES` skip tree-sitter.
- Panes render only the `scroll..scroll+height` window; don't switch to `Paragraph::scroll` (cloning every line stalls big files). Word wrap is `ui/wrap.rs::wrap_spans`, not ratatui `Wrap`; `scroll` stays in logical lines.
- `Paragraph` doesn't paint a `Line` style past its text — `fill_rows` pads tinted diff rows.
- Wide (CJK) graphemes leave one-cell ghosts unless trailing cells are forced to repaint: `main.rs` keeps the previous frame and `ui/repaint.rs::force_wide_trails` marks them `AlwaysUpdate`.
- Mouse hit-testing uses `Rect`s and `ListState` offsets captured during render, which is why `App::render` takes `&mut self`.
- Selection: renderers record `App.content_rows`; logical lines differ per view (file lines / unified `file.lines` indices / side-by-side aligned pairs via `ui::diff::align_indices`). `ContentTarget` pins what was selected; `App::render` drops a stale selection.
- Diff advanced color highlights whole old/new blobs (`App::ensure_diff_highlight`) and indexes by line number; a text mismatch must fall back to plain.
- Avoid Ctrl+J bindings (arrives as Enter in legacy terminals).

### Pick

- `PickState.selected_diff_file` is carried into `DiffState.selected_file` on `Tab` and back on `Esc` (clamped via `PickState::set_diff_file`).
- Scroll uses ratatui `List::scroll_padding(3)`, which is symmetric only. A custom asymmetric margin was rolled back on 2026-05-24 (interaction with d/u and Ctrl+F/B batch moves). If retrying: hold `PickState.scroll`, render with `skip().take()`, recompute via `normalize_pick_scroll(prev_selected)` after every move, handle `Event::Resize`.
- `CommitStore` loads 200-commit batches; after a batch the active filter re-runs `update_filter()` and `selected` is remapped to keep the cursor.

### Search

- Bump `INDEX_VERSION` (`search/mod.rs`) whenever stored fields/text, the tokenizer, chunking or the embedded text change — a mismatch forces a full rebuild; a `head_oid` change only does an incremental update.
- turbovec: `VectorIndex::calibrate()` must run before the first `add` on a full build, or recall silently drops. Incremental updates reuse the stored calibration and persist via `sync`.
- `.glcignore` blob oid is stored as `ignore_hash`; a change forces a full rebuild. Any tracked file quoting `tests/fixtures/search_queries.toml` queries must be listed in this repo's `.glcignore`, or `glc report` scores against its own fixtures.
- Tuning knobs (typo correction, weak-match gap, …) live in `search/params.rs::SearchParams`; check `glc report` before and after changing them.
- Indexing and engine load run on their own threads and report through `mpsc` (`IndexMessage`, `EngineMessage`). `with_silenced_stdio()` keeps hf-hub progress bars off the alternate screen (Unix only, no-op elsewhere).
- Symbol chunking `Language`/`Query` are `OnceLock`-cached; Rust impl/trait containers are not chunked, only their methods. Slice source with `source.get(range)` (char-boundary panics).

### Config

- Missing file → `Config::default()`, never auto-created. On a parse error `load_error` is kept and `save()` refuses, so a typo can't wipe stored API keys on the next toggle save. The file is written 0600; `ProviderConfig`/`Endpoint` mask keys in `Debug`.

### Ask

- One channel per request; dropping the receiver cancels. `parse_sse` calls back on non-content lines so a closed panel cancels before the first token. EOF without `[DONE]`/`finish_reason` is an error, and failed or abandoned streams are never cached.
- The answer cache key is provider + base_url + exact request JSON, never the API key.
- ureq 3 runs with `http_status_as_error(false)` so provider error bodies survive; there is no idle timeout, only `timeout_recv_response` + `timeout_recv_body`. hf-hub still pulls ureq 2.
- Diff line `content` keeps its trailing newline — strip it when serializing (`ask/context.rs::strip_eol`).

### Repo watch

- View/Diff only set `repo_changed` on HEAD change; the refresh applies in `back()`. Diff/tree caches are oid-keyed and immutable — never clear them on refresh.

## tree-sitter API (0.27)

- `Highlighter::highlight(config, source, encoding, cancellation_flag, injection_cb)`.
- `QueryCursor::matches` returns a `StreamingIterator`: `while let Some(m) = matches.next()`, `m.captures()`.

## Planning artifacts

- Design specs → `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`
- Implementation plans → `docs/superpowers/plans/YYYY-MM-DD-<feature>.md`
- Current-cycle plans → `docs/plans/`; search reports → `docs/reports/`
- Never save to `.opencode/plans/`.

## CI

- `ci.yml`: check, test, `cargo fmt --all -- --check`, `clippy --all-targets -D warnings`. New clippy lints arrive with toolchain updates — fix them, don't `allow` them.
- `release.yml`: `v*` tag → aarch64-apple-darwin + x86_64-pc-windows-msvc builds → GitHub Release → Homebrew tap. Keep code buildable on MSVC.
- `pages.yml`: deploys `site/`.

## Conventions

- Commit messages in Korean; code identifiers and comments in English.
- Run `cargo fmt --all` before committing.
