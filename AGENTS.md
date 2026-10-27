# AGENTS.md — gluck

Compact repo guide for coding agents. When in doubt, prefer executable truth over prose.

## Identity

- Crate `gluck`, binary `glc`. Single crate, edition 2021. Not published to crates.io (git deps are fine).
- Terminal TUI git history viewer: Pick (commit list) → View (file tree) → Diff (compare). Semantic-search overlay modal and an LLM "Ask" drawer layered on top.
- Release tags `v*`. Version in `Cargo.toml`.

## Layout

```
src/
├── main.rs / lib.rs / app.rs   # entry + event loop, modules, central App orchestrator
├── cli.rs / config.rs          # clap CLI + ~/.config/gluck/config.toml
├── mode.rs                     # Mode enum, key bindings, action dispatch
├── theme.rs                    # 6 built-in palettes + highlight-map builder (+ capture-name aliases)
├── selection.rs                # line-granular mouse selection + per-frame content row map
├── debug.rs                    # tracing-subscriber init
├── lang.rs                     # Language enum, extension / file-name detection
├── git/                        # repo, commit, store (lazy 200-batch), tree, diff, cache (LRU)
├── ask/                        # LLM explain: provider (config→preset→env), client (ureq SSE), prompt, context, panel, cache
├── highlight/engine.rs         # tree-sitter-highlight configs for every language + span builder
├── search/
│   ├── mod.rs                  # SearchEngine = BM25 + vector + RRF fusion, INDEX_VERSION
│   ├── bm25.rs / vector.rs     # tantivy index / turbovec ANN
│   ├── embedding.rs            # model2vec-rs (potion-multilingual-128M, 256-dim)
│   ├── rrf.rs / params.rs      # reciprocal rank fusion / SearchParams tuning knobs
│   ├── typo.rs / text_prep.rs  # BM25 typo correction / query + doc text normalization
│   ├── chunk/                  # Chunk variants + tree-sitter symbol extraction
│   ├── indexer.rs / diff.rs    # build_index() full + incremental (head_oid delta)
│   ├── glcignore.rs            # .glcignore matcher + `glc ignore` template
│   ├── modal_state.rs          # Closed/Typing/Loading/Results state machine
│   ├── report/ / diagnose.rs   # `glc report` quality + perf metrics
│   └── silence.rs              # stderr redirect during model load (no-op off Unix)
└── ui/                         # pick, view, diff, diff_words, wrap, repaint, search_modal, ask_panel, layout
vendor/                         # vendored grammars (see Syntax highlighting)
examples/                       # *_probe.rs diagnostics (typo, ask, commit, negative queries)
site/                           # GitHub Pages landing page
```

## Commands

```bash
cargo run --bin glc -- [PATH]                    # run TUI
cargo run --bin glc -- index [--force]           # headless index build
cargo run --bin glc -- ignore [--force]          # write default .glcignore (glcignore::DEFAULT_TEMPLATE)
cargo run --bin glc -- report [--out FILE.md]    # search quality + perf report
cargo test [name]
cargo clippy --all-targets -- -D warnings        # same as CI
cargo fmt --all                                  # CI runs cargo fmt --all -- --check
```

## Test conventions

- Helpers live in `src/git/repo.rs` under `#[cfg(test)] pub mod tests`: `init_test_repo()` → `(TempDir, Repository)`, `add_file_commit(&repo, path, content, msg)`.
- Highlight tests use `assert_span_colors(path, source, &[(text, color)])` in `highlight/engine.rs` against `Palette::plain()`.
- No external services, no snapshots, no expensive suites. App tests resolve Ask providers with an empty env.

## Mode state machine

```
Pick ──Enter──→ View ──Tab──→ Diff
  ↑              ↑            │
  │              └────Tab─────┘
  └─── Esc/h ─────────────────┘
```

- Pick: commit list + inline diff preview. `/` opens prefix search (`CommitIndex` tree). Lazy-loads 200-batches; prefetches when selection is within 50 of the end. `J`/`K` move `PickState.selected_diff_file` through the right-lower Files panel (reset on commit change); `Tab` carries that cursor into `DiffState.selected_file`, and `Esc` from Diff brings it back (clamped via `PickState::set_diff_file`).
- View: file tree + syntax-highlighted content. `.` toggles gitignore filter. On a directory `Enter` toggles fold, `h` collapses (then jumps to parent), `l` expands (then steps into first child); on files `h`/`l` keep their Back/open bindings. `H`/`L` and ←/→ jump to the previous/next changed file; ↑/↓ move the tree selection.
- Diff: side-by-side default, `v` toggles unified. `h`/`l` and ←/→ navigate files; `j`/`k` scroll 1 line, `J`/`K` and ↑/↓ page, `u`/`d` scroll `scroll_lines`.
- Advanced diff color (`c` in Diff, `config.ui.advanced_diff`, default on, persisted): rows keep syntax colors over a tinted bg (`Palette::diff_bg`, blended from `bg` + added/removed; None on `plain` → fg-colored text). Highlights come from the whole old/new blobs (`App::ensure_diff_highlight`, called at the top of `render_diff`, keyed (from, to, file), through `content_cache`), indexed by line number; a text mismatch falls back to plain. Paired removed/added lines get word emphasis (`ui/diff_words.rs`, token LCS, skipped under 40% overlap). `Paragraph` doesn't paint a `Line` style past its text — `fill_rows` pads tinted rows.
- Word wrap: `w` toggles `config.ui.word_wrap` (default on, persisted) in View/Diff. Wrapping is done in `ui/wrap.rs::wrap_spans` (span-level, whitespace-preferring, unicode-width aware) so continuation rows get a blank gutter; ratatui `Wrap` is not used. `scroll` stays in logical lines. Side-by-side pads each aligned pair to the taller side's row count so panes stay in step.
- Mouse: capture enabled in `main.rs`. Wheel moves list selection (Pick, View tree) or scrolls content (View pane, Diff, Ask panel); left press starts a content selection or else clicks a tree/commit row (`App::mouse_press`; dirs toggle fold, Pick double-click opens View). Hit-testing uses panel `Rect`s + `ListState` offsets captured into `App` fields at render time — `App::render` takes `&mut self` for this.
- Ctrl keys: `Ctrl+N/P` cross commits while preserving the selected file path; `Ctrl+T` cycles theme (persisted); `Ctrl+F/B` page Pick; `Ctrl+D` toggles the debug overlay (top-right); `Ctrl+C` quits. Ctrl+J is avoided (arrives as Enter in legacy terminals).
- Ask (`a` / `A` deep, View/Diff): explains the visible page, or the mouse-dragged line selection, via an OpenAI-compatible `/chat/completions` stream on a worker thread (`ask::spawn_ask`, one channel per request — dropping the receiver cancels). Answer shows in a bottom drawer (`ui/ask_panel.rs`, 60/40 body split); `[`/`]` page-scroll it (panel height − 1; in Diff, `j`/`k` scroll it 1 line while the panel is open; View keeps `j`/`k` for the tree), `r` retries (bypasses + refreshes the cache), `Esc` closes. Finished answers go into `ask::cache::AnswerCache` (LRU 16, keyed by provider + base_url + exact request JSON, not the API key); identical re-asks show `cached` with no request. Failed/abandoned streams are never cached. `Esc` precedence: panel → selection → mode Back.
- Selection: renderers record `App.content_rows` (screen row → logical line). Logical lines are file lines (View), `file.lines` indices (unified), aligned-pair indices (side-by-side; `ui::diff::align_indices` / `aligned_to_file_range` convert). `ContentTarget` pins commit/file/view kind; `App::render` drops a stale selection. A press without drag clears it.

## Search systems (two separate)

| | Inline (`/`) | Semantic (`s`) |
|---|---|---|
| Backend | `CommitIndex` prefix tree in `CommitStore` | `SearchEngine` (BM25 + vector + RRF) |
| Storage | in-memory, built on commit load | on-disk `.glc-index/`, requires `glc index` |
| Scope | commit message/author/short_id | commits + file content + symbols |

- Threading: indexing and engine-load each run on their own thread and talk via `mpsc::channel` (`IndexMessage`, `EngineMessage`). `EngineMessage::Ready(Box<SearchEngine>)` hands the engine to the main thread. `with_silenced_stdio()` redirects stderr during model load to keep hf-hub progress bars out of the alternate screen (`libc::dup2` on Unix, no-op elsewhere).
- Index dir `.glc-index/` has `meta.toml` with `INDEX_VERSION` (currently 11), `head_oid`, per-component metadata. A version mismatch forces a full rebuild. A `head_oid` mismatch triggers an incremental update (BM25 `delete_term` + turbovec `remove` for stale docs, embed only the delta) when the old oid is still reachable; otherwise full rebuild.
- `.glcignore` (work-tree root, gitignore syntax via the `ignore` crate) excludes HEAD files from indexing, in full and incremental builds; commit docs are never filtered. Its blob oid is stored as `meta.toml` `ignore_hash`; a mismatch forces a full rebuild. This repo's `.glcignore` excludes search-eval artifacts (reports, fixtures, docs quoting the query set, `HANDOFF.md`) plus the `glc ignore` default list, so `glc report` isn't scored against its own outputs — any tracked file quoting fixture queries must be added there.
- Commit chunks embed changed paths (first-parent name-only diff, max 20) after the message.
- Typo correction (`search/typo.rs`, `SearchParams.typo_mode`, default 1): non-Korean query words (4+ ASCII letters) missing from the BM25 word vocabulary (title + path_terms + the query-unused `body_words` field, cached as `SearchEngine.vocab`) map to a term one OSA edit away with the same first letter — short words (<8) only via dropped letter / adjacent swap / doubled key, never plural forms — and corrections are appended to the BM25 query. Embedded text is left as is. `examples/typo_probe.rs` prints corrections per fixture query.
- Low-confidence flag: `SearchEngine::search_scored` returns `SearchOutcome { results, weak }` (`search` wraps it, results only). `weak` = no query word (incl. typo corrections) hits title/path_terms AND vector top1 − top10 mean < `SearchParams.weak_gap` (0.055); skipped for `path:` queries. Results are never dropped — the modal shows `· no strong match`, and `glc report` counts a flagged negative as PASS and reports flagged positives (and false alarms among them).
- 검색 품질 회귀 추적은 `glc report`가 `tests/fixtures/search_queries.toml`의 쿼리/정답으로 MRR/Recall@k/NDCG@10 + latency p50/p95/p99를 계산해 stdout(및 `--out` markdown)에 출력한다.

## Architecture gotchas

### Syntax highlighting

- Languages: Rust, Markdown, JSON, YAML, JavaScript, TypeScript, TSX, Swift, Bash (sh/bash/zsh), Zig, C, C++, Asm (s/S/asm), linker scripts (ld/lds), LLVM IR (ll), Go, Odin, Python, Ruby (+ `Rakefile`/`Gemfile`), Lua, PHP, Java, Kotlin, Scala, Clojure (clj/cljs/cljc/edn/bb), Erlang (+ `rebar.config`), Elixir, Gleam, Janet (janet/jdn), Lisette (lis). Detection is `lang::Language::from_path` (file name first, then extension); anything else renders as plain text.
- Every grammar is statically linked; the README explains why (vs Helix's dynamic `.so` loading) and the size cost.
- Before adding a grammar, check `LANGUAGE_VERSION` in its `src/parser.c` against the runtime's ABI 13–15 window, and make sure it depends on `tree-sitter-language`, not `tree-sitter` directly (that links a second runtime). Then assert real colors with `assert_span_colors` — a config that fails to build is silently skipped by `register_languages`.
- `HighlightConfiguration::new()` in 0.27 is `(language, name, highlights, injections, locals)`; don't pass a query into the `name` slot. Injections are never resolved, so injection queries are left empty.
- One shared `HIGHLIGHT_NAMES` drives all configs via `.configure()`. A capture resolves to the matching name with the most dot-parts, ties going to the earlier entry. New capture names that should look like an existing style go in both `HIGHLIGHT_NAMES` and `theme.rs::HIGHLIGHT_ALIASES` (e.g. `keyword.function` → keyword, which otherwise fell to `function`; nvim-style `conditional`/`repeat`/`include`/`namespace`/`parameter`/`method`/`float`/`field`… used by Lua, Zig, Scala, Odin). `theme.rs` tests require a style for every name.
- When two patterns capture the same node the later one wins, and an unstyled capture (`@_name`) still wins the node — capture predicate subjects with a real name instead. `engine.rs::layered()` appends gluck-side patterns to upstream queries: C++ (C query first, as upstream's tree-sitter.json does), Kotlin `package` keyword, LLVM function names over the trailing `@variable`, PHP declared type names, and the Clojure/Janet call-head / special-form / definition patterns.
- Odin's query targets nvim: `odin_highlights()` rewrites `#lua-match?` → `#match?` and cuts `(#not-has-parent? …)` forms. Unknown predicates are ignored, not rejected, so such queries load but over-match.
- Captures nest; `highlight()` keeps a style stack so ending an inner capture restores the outer style.
- Inline queries: `MARKDOWN_HIGHLIGHTS_QUERY` (`tree-sitter-markdown-fork` 0.7.3 exports none), `JSON_HIGHLIGHTS_QUERY`, `SWIFT_HIGHLIGHTS_QUERY`. TS/TSX layer the JS query first (`ts_with_js_keywords`).
- Non-crates.io grammars:
  - git deps pinned by `rev`: `tree-sitter-gleam` (crates.io 1.0.0 is stale; config passes `LOCALS_QUERY` for `#is-not? local` and skips the doc-comment injection), `tree-sitter-kotlin` from fwcd (crates.io 0.3.8 pins tree-sitter < 0.23; `tree-sitter-kotlin-ng` ships no queries).
  - vendored in `vendor/` (each has `UPSTREAM.md` with synced commit + sync steps): `tree-sitter-linkerscript` (crates.io pins tree-sitter 0.20), `tree-sitter-lisette` (no release; parser.c was regenerated at ABI 14 for the old 0.23 runtime — the next sync can copy upstream's ABI 15 parser.c verbatim), `tree-sitter-clojure` / `tree-sitter-janet-simple` from sogaiu (CC0, no Rust bindings; upstream queries cover literals only). Vendored `build.rs` use `cc` `warnings(false)` (per-flag `-Wno-*` is dropped by MSVC) and watch `src/tree_sitter/`; lisette adds `-utf-8` on MSVC. They aren't workspace members, so `cargo test -p` can't run their own tests.

### Chunking

- `search/chunk/`: `mod.rs` (Chunk enum), `symbol.rs` (tree-sitter queries), `file.rs` (split policy), `commit.rs` (commit-message split).
- Symbol extraction: Rust, Python, JavaScript, TypeScript, TSX, Go, Swift (`Language::supports_symbol_chunking`). `Language` and `Query` are cached per language via `OnceLock` — never recompiled per file.
- Rust impl/trait containers are not chunked; only their `function_item` children become `SymbolKind::Method`.
- Files under `WHOLE_FILE_THRESHOLD` (16KB) stay one chunk. UTF-8 safe slicing via `source.get(range)` dodges char-boundary panics.

### View / rendering

- `FileContent::Text` carries `raw: String` + `highlighted: Vec<Line<'static>>`. Setting only `raw` makes the view fall back to plain text — `load_view_file()` must populate `highlighted` via `self.highlight.highlight(...)`. `line_count()` prefers `highlighted.len()`; `ui/view.rs` prepends line numbers when `highlighted` is populated.
- `ViewState.selected_file` indexes `visible` (fold-filtered indices into `tree`), not `tree` — go through `selected_entry()`/`select_path()`; call `rebuild_visible()` after mutating `tree` or `collapsed`.
- j/k navigation defers loading: `request_view_file_load()` shows `FileContent::Loading` and arms `pending_view_load`; `tick_pending_view_load()` loads after `VIEW_LOAD_DEBOUNCE` (60ms) of quiet, so held keys skip intermediate files. `Enter`/commit-switch paths call `load_view_file()` directly.
- Loaded content is LRU-cached in `App.content_cache` keyed `(commit oid, path)` — cleared on theme cycle. Files over `MAX_HIGHLIGHT_BYTES` (256KB) skip tree-sitter and render via `HighlightEngine::plain_lines`.
- View and Diff panes render only the `scroll..scroll+height` window (no `Paragraph::scroll`) — cloning every line per frame stalls large files.
- Wide (CJK) graphemes: ratatui keeps the trailing cell blank and its diff skips it while it stays blank, but terminals erase that half themselves (keeping the old bg) when the lead cell is overwritten → one-cell ghosts. `main.rs` keeps the previous frame and `ui/repaint.rs::force_wide_trails` marks those trailing cells `CellDiffOption::AlwaysUpdate`.
- Search modal is a 70%×60% centered overlay using the `Clear` widget.

### CommitStore batching

- Loads 200 commits at a time via `revwalk` (TOPOLOGICAL). `prefetch_if_near_end()` triggers the next batch when selection nears the tail.
- After a batch load the active filter re-runs `update_filter()`, and `selected` remaps to keep the cursor stable.

### Pick mode scroll

- Delegates to ratatui `List::scroll_padding(3)` in `ui/pick.rs`; ratatui keeps padding symmetric — directional padding isn't supported by the widget.
- 2026-05-24: a custom asymmetric margin was rolled back (interaction complexity with d/u and Ctrl+F/B batch moves). If retrying: hold scroll as a `PickState.scroll` field, render with `skip().take()`, recompute via `normalize_pick_scroll(prev_selected)` after every move, handle `Event::Resize`.

### Config

- `~/.config/gluck/config.toml` (`dirs::config_dir()`). Sections: `[theme]` (name), `[ui]` (scroll_lines, word_wrap, advanced_diff), `[search]` (index_dir, batch_size, max_file_bytes, result_limit), `[ask]` + `[ask.providers.<name>]`.
- Missing file → `Config::default()`; never auto-created. Saved when a persisted toggle changes (theme, wrap, diff color).
- `[ask]`: config values win, empty ones fall back to the preset (`ask/provider.rs::PRESETS`) and its env var; empty `provider` auto-detects the first preset whose env key is set. `save()` writes the file 0600 from creation (keys may live there). `Config::load_or_default()` keeps a parse error in `load_error` and `save()` then refuses, so a typo can't wipe stored keys on the next save; `ProviderConfig`/`Endpoint` mask keys in `Debug`.
- Diff line `content` keeps its trailing newline — strip it when serializing (`ask/context.rs::strip_eol`).

### Event loop / repo watch

- `event::poll` timeout: 80ms while indexing or streaming an Ask answer, 30ms while a debounced view load is pending, else 1s.
- `App::poll_repo_watch()` checks `GitRepo::head_info()` (oid + ref shorthand) every `HEAD_POLL_INTERVAL` (5s). On change, Pick rebuilds `CommitStore` immediately (filter re-applied, selection restored by oid); View/Diff only set `repo_changed` and show a footer notice — the refresh applies in `back()`.
- Diff/tree caches are oid-keyed and immutable — never cleared on refresh.

## Notable dependencies

Authoritative list lives in `Cargo.toml`. Non-obvious points:

- `tree-sitter` / `tree-sitter-highlight` 0.27 (runtime ABI 13–15). Language crates connect through `tree-sitter-language`, so their versions are independent of the runtime. `Highlighter::highlight` takes `(config, source, encoding, cancellation_flag, injection_cb)`; `QueryCursor::matches` returns a `StreamingIterator` (`while let Some(m) = matches.next()`, `m.captures()`).
- `model2vec-rs` 0.2 with `hf-hub` pulls the embedding model at runtime on the first index build.
- `turbovec` 1.0 is pure Rust (no BLAS). TQ+ calibration is explicit: `VectorIndex::calibrate()` must run before the first `add` on a full build, or the index silently loses recall. Incremental updates reuse the stored calibration and persist via `sync` (append-only). Warnings go through `turbovec::set_warning_hook` → `tracing` (`main.rs`).
- `ureq` 3 (Ask client) runs with `http_status_as_error(false)` so provider error bodies survive. There's no per-chunk idle timeout, so the stream is bounded by `timeout_recv_response` (60s) + `timeout_recv_body` (300s); `parse_sse` calls back with `""` on every non-content line so a closed panel cancels before the first token, and treats EOF without `[DONE]`/`finish_reason` as an error (never cached). `hf-hub` still pulls `ureq` 2 — two versions coexist.

## Planning artifacts

- Design specs → `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`
- Implementation plans → `docs/superpowers/plans/YYYY-MM-DD-<feature>.md`
- Current-cycle plans → `docs/plans/`; search reports → `docs/reports/`
- Never save to `.opencode/plans/` — transient only.

## CI

- `ci.yml`: check + test + `cargo fmt --all -- --check` + `clippy --all-targets -D warnings` on push/PR to main. New clippy lints land with toolchain updates — fix them, don't `allow` them.
- `release.yml`: `v*` tag → builds aarch64-apple-darwin + x86_64-pc-windows-msvc → GitHub Release → updates the Homebrew tap.
- `pages.yml`: deploys `site/` to GitHub Pages on changes under `site/`.

## Conventions

- Commit messages in Korean, code identifiers/comments in English.
- The repo is rustfmt-clean; run `cargo fmt --all` before committing.
