# AGENTS.md — gluck

Compact repo guide for coding agents. When in doubt, prefer executable truth over prose.

## Identity

- Crate `gluck`, binary `glc`. Single crate, edition 2021.
- Terminal TUI git history viewer: Pick (commit list) → View (file tree) → Diff (compare). Semantic-search overlay modal layered on top.
- Release tags `v*`. Version in `Cargo.toml`.

## Layout

```
src/
├── main.rs / lib.rs / app.rs   # entry, modules, central App orchestrator
├── cli.rs / config.rs          # clap CLI + ~/.config/gluck/config.toml
├── mode.rs                     # Mode enum, key bindings, action dispatch
├── theme.rs                    # 6 built-in palettes + highlight-map builder
├── selection.rs                # line-granular mouse selection + per-frame content row map
├── debug.rs                    # tracing-subscriber init
├── lang.rs                     # Language enum + Path-based extension detection
├── git/                        # repo, commit, store (lazy 200-batch), tree, diff, cache (LRU)
├── ask/                        # LLM explain: provider (config→preset→env), client (ureq SSE), prompt, context, panel
├── highlight/engine.rs         # tree-sitter-highlight for Rust + Markdown
├── search/
│   ├── mod.rs                  # SearchEngine = BM25 + Vector + RRF fusion
│   ├── bm25.rs                 # tantivy index
│   ├── vector.rs               # turbovec ANN
│   ├── embedding.rs            # model2vec-rs (potion-multilingual-128M, 256-dim)
│   ├── rrf.rs                  # reciprocal rank fusion
│   ├── chunk/                  # Chunk variants + tree-sitter query symbol extraction
│   ├── indexer.rs              # build_index(): walks HEAD, chunks, embeds, writes BM25+vectors
│   ├── modal.rs                # Closed/Typing/Loading/Results state machine
│   └── silence.rs              # Unix-only stderr redirect (suppress hf-hub progress bars)
└── ui/                         # pick, view, diff, search_modal, ask_panel, layout
```

## Commands

```bash
cargo run --bin glc -- [PATH]                    # run TUI
cargo run --bin glc -- index [--force]           # headless index build
cargo run --bin glc -- ignore [--force]          # write default .glcignore (glcignore::DEFAULT_TEMPLATE)
cargo run --bin glc -- report [--out FILE.md]    # search quality + perf report
cargo test [name]
cargo clippy                                     # CI: --all-targets -D warnings
rustfmt src/<changed>.rs                         # repo has formatting debt; format only what you touch
```

## Test conventions

- Helpers live in `src/git/repo.rs` under `#[cfg(test)] pub mod tests`: `init_test_repo()` → `(TempDir, Repository)`, `add_file_commit(&repo, path, content, msg)`.
- No external services, no snapshots, no expensive suites.

## Mode state machine

```
Pick ──Enter──→ View ──Tab──→ Diff
  ↑              ↑            │
  │              └────Tab─────┘
  └─── Esc/h ─────────────────┘
```

- **Pick**: commit list + inline diff preview. `/` opens prefix search (CommitIndex tree). Lazy-loads 200-batches; prefetches when selection within 50 of end. `J`/`K` move `PickState.selected_diff_file` through the right-lower Files panel (reset on commit change); `Tab` carries that cursor into `DiffState.selected_file`, and `Esc` from Diff brings `DiffState.selected_file` back (clamped via `PickState::set_diff_file`).
- **View**: file tree + syntax-highlighted content. `.` toggles gitignore filter. On a directory `Enter` toggles fold, `h` collapses (then jumps to parent), `l` expands (then steps into first child); on files `h`/`l` keep their Back/open bindings. `H`/`L` and ←/→ jump to the previous/next changed file; ↑/↓ move the tree selection.
- **Diff**: side-by-side default. `v` toggles unified. `h`/`l` and ←/→ navigate files; `j`/`k` scroll the diff 1 line, `J`/`K` and ↑/↓ page, `u`/`d` `scroll_lines`.
- **Advanced diff color** (`c` in Diff, `config.ui.advanced_diff`, default on, persisted): rows keep syntax colors over a tinted bg (`Palette::diff_bg`, blended from `bg` + added/removed; None on `plain` → legacy fg-colored text). Highlights come from the whole old/new blobs (`App::ensure_diff_highlight`, called at the top of `render_diff`, keyed (from, to, file), through `content_cache`), indexed by line number; a text mismatch falls back to plain. Paired removed/added lines get word emphasis (`ui/diff_words.rs`, token LCS, skipped under 40% overlap). `Paragraph` doesn't paint a `Line` style past its text — `fill_rows` pads tinted rows.
- **Word wrap**: `w` toggles `config.ui.word_wrap` (default on, persisted like theme) in View/Diff. Wrapping is done in `ui/wrap.rs::wrap_spans` (span-level, whitespace-preferring, unicode-width aware) so continuation rows get a blank gutter; ratatui `Wrap` is not used. `scroll` stays in logical lines. Side-by-side pads each aligned pair to the taller side's row count so panes stay in step. Diff panes now render only the `scroll..` window instead of `Paragraph::scroll`.
- **Mouse**: capture enabled in `main.rs`. Wheel moves list selection (Pick, View tree) or scrolls content (View pane, Diff); click selects tree/commit rows (dirs toggle fold, Pick double-click opens View). Hit-testing uses panel `Rect`s + `ListState` offsets captured into `App` fields at render time — `App::render` takes `&mut self` for this.
- **Ctrl+N/P** crosses commits while preserving selected file path.
- **Ctrl+T** cycles theme and persists to config.
- Ask (`a` / `A` deep, View/Diff): explains the visible page, or the mouse-dragged line selection, via an OpenAI-compatible `/chat/completions` stream on a worker thread (`ask::spawn_ask`, one channel per request — dropping the receiver cancels). Answer shows in a bottom drawer (`ui/ask_panel.rs`, 60/40 body split); `[`/`]` page-scroll the answer (panel height − 1; in Diff, `j`/`k` scroll it 1 line instead of the diff while the panel is open; View keeps `j`/`k` for the tree), `r` retry (bypasses + refreshes the cache), `Esc` closes. Finished answers go into `ask::cache::AnswerCache` (LRU 16, keyed by provider + base_url + exact request JSON, not the API key); identical re-asks show `cached` with no request. Failed/abandoned streams are never cached. `Esc` precedence: panel → selection → mode Back. Ctrl+J is avoided (arrives as Enter in legacy terminals).
- Selection: renderers record `App.content_rows` (screen row → logical line). Logical lines are file lines (View), `file.lines` indices (unified), aligned-pair indices (side-by-side; `ui::diff::align_indices` / `aligned_to_file_range` convert). `ContentTarget` pins commit/file/view kind; `App::render` drops a stale selection. A press without drag clears it.

## Search systems (two separate)

| | Inline (`/`) | Semantic (`s`) |
|---|---|---|
| Backend | `CommitIndex` prefix tree in `CommitStore` | `SearchEngine` (BM25 + vector + RRF) |
| Storage | in-memory, built on commit load | on-disk `.glc-index/`, requires `glc index` |
| Scope | commit message/author/short_id | commits + file content + symbols |

Semantic-search threading: indexing and engine-load each run on their own thread, talk via `mpsc::channel` (`IndexMessage`, `EngineMessage`). `EngineMessage::Ready(Box<SearchEngine>)` hands the heap-allocated engine to the main thread. `with_silenced_stdio()` redirects stderr during model load to keep hf-hub progress bars out of the alternate screen — **Unix-only** (`libc::dup2`).

Index dir `.glc-index/` has `meta.toml` with `INDEX_VERSION` (currently 11), `head_oid`, per-component metadata. Mismatched version forces full rebuild. Mismatched `head_oid` triggers incremental update (BM25 `delete_term` + turbovec `remove` for stale docs, embed only the delta) when the old `head_oid` is still reachable; otherwise falls back to full rebuild.

`.glcignore` (repo work-tree root, gitignore syntax via the `ignore` crate, `src/search/glcignore.rs`) excludes HEAD files from indexing — full and incremental builds; commit docs are never filtered. Its blob oid is stored as `meta.toml` `ignore_hash`; a mismatch forces a full rebuild on the next `build_index`. This repo's `.glcignore` excludes search-eval artifacts (reports, fixtures, docs quoting the query set, `HANDOFF.md`) plus the `glc ignore` default list, so `glc report` isn't scored against its own outputs. Any tracked file quoting fixture queries must be added there. Commit chunks embed changed paths (first-parent name-only diff, max 20) after the message.

Typo correction (`src/search/typo.rs`, `SearchParams.typo_mode`, default 1): non-Korean query words (4+ ASCII letters) missing from the BM25 word vocabulary (title + path_terms + the query-unused `body_words` field, cached as `SearchEngine.vocab`) are mapped to a term one OSA edit away with the same first letter — short words (<8) only via dropped letter / adjacent swap / doubled key, never plural forms — and the corrections are appended to the BM25 query. The embedded text is left as is. `examples/typo_probe.rs` prints the corrections per fixture query.

Low-confidence flag: `SearchEngine::search_scored` returns `SearchOutcome { results, weak }` (`search` is a wrapper returning only results). `weak` = no query word (incl. typo corrections) hits title/path_terms AND vector top1 − top10 mean < `SearchParams.weak_gap` (0.055); skipped for `path:` queries. Results are never dropped — the modal shows `· no strong match`, and `glc report` counts a flagged negative as PASS and prints how many positives were flagged (and how many of those had the answer in the top 10 = false alarms).

검색 품질 회귀 추적은 `glc report`가 `tests/fixtures/search_queries.toml`의 쿼리/정답으로 MRR/Recall@k/NDCG@10 + latency p50/p95/p99를 계산해 stdout(및 `--out` markdown)에 출력한다.

## Architecture gotchas

### Syntax highlighting

- `HighlightConfiguration::new()` signature in `tree-sitter-highlight` 0.27 is `(language, name, highlights, injections, locals)`. A past bug passed query strings into the `name` slot.
- One shared `HIGHLIGHT_NAMES` array drives all language configs via `.configure(HIGHLIGHT_NAMES)`. Adding a language: append names here AND add palette entries in `theme.rs::to_highlight_map()`.
- `tree-sitter-markdown-fork` 0.7.3 doesn't export its highlight query — we embed `MARKDOWN_HIGHLIGHTS_QUERY` inline.
- Highlighted languages: **Rust, Markdown, JSON, TypeScript, TSX, JavaScript, YAML, Swift, Bash (sh/bash/zsh), Zig, C, Asm (s/S/asm), linker scripts (ld/lds), Go, Lisette (lis), Gleam (gleam)**. Extension detection goes through `lang::Language::from_path`; non-matching files fall back to plain text.
- `tree-sitter-linkerscript` is vendored at `vendor/tree-sitter-linkerscript/` (crates.io release pins tree-sitter 0.20/cc 1.0 — incompatible); grammar sources are verbatim upstream, only the Rust bindings are modernized. Synced commit + sync steps: `vendor/tree-sitter-linkerscript/UPSTREAM.md`.
- `tree-sitter-lisette` is vendored at `vendor/tree-sitter-lisette/` from ivov/lisette `editors/tree-sitter-lisette` (no crates.io release; upstream parser.c is ABI 15, ours was regenerated at ABI 14 with tree-sitter CLI 0.23 back when the runtime was 0.23 — the 0.27 runtime loads ABI 15, so the next sync can copy upstream parser.c verbatim and drop the regeneration step). Both vendored `build.rs` use `cc` `warnings(false)` (not per-flag `-Wno-*`, which MSVC drops) and watch `src/tree_sitter/`; lisette adds `-utf-8` on MSVC. Synced commit + sync steps: `vendor/tree-sitter-lisette/UPSTREAM.md`.
- `tree-sitter-gleam` is a git dependency pinned by `rev` on gleam-lang/tree-sitter-gleam (crates.io 1.0.0 is stale and its bundled query ends with an `(identifier) @variable` catch-all that overrides fn/const/module captures under tree-sitter-highlight). Config passes `LOCALS_QUERY` (needed by the `#is-not? local` predicates) and skips the doc-comment markdown injection. gluck is not published to crates.io, so git deps are fine.

### Chunking

- `src/search/chunk/` is a module dir: `mod.rs` (Chunk enum), `symbol.rs` (tree-sitter queries), `file.rs` (split policy), `commit.rs` (commit-message split).
- Symbol extraction supported for Rust, Python, JavaScript, TypeScript, TSX, Go. `Language` and `Query` are cached per language via `OnceLock` — never recompiled per file.
- Rust impl/trait containers are NOT chunked; only their `function_item` children become `SymbolKind::Method`.
- WholeFile threshold 8KB. UTF-8 safe slicing via `source.get(range)` to dodge char-boundary panics.

### View / rendering

- `FileContent::Text` carries `raw: String` + `highlighted: Vec<Line<'static>>`. Setting only `raw` makes view fall back to plain text. `load_view_file()` must populate `highlighted` via `self.highlight.highlight(...)`.
- `FileContent::line_count()` prefers `highlighted.len()` over `raw.lines().count()`.
- `ui/view.rs` prepends line numbers when `highlighted` is populated — keep that consistent if you change construction.
- `ViewState.selected_file` indexes `visible` (fold-filtered indices into `tree`), NOT `tree` — go through `selected_entry()`/`select_path()`; call `rebuild_visible()` after mutating `tree` or `collapsed`.
- j/k navigation defers loading: `request_view_file_load()` shows `FileContent::Loading` and arms `pending_view_load`; the main loop's `tick_pending_view_load()` loads after `VIEW_LOAD_DEBOUNCE` (60ms) of quiet, so held keys skip intermediate files. `Enter`/commit-switch paths still call `load_view_file()` directly.
- Loaded content is LRU-cached in `App.content_cache` keyed `(commit oid, path)` — cleared on theme cycle. Files over `MAX_HIGHLIGHT_BYTES` (256KB) skip tree-sitter and render via `HighlightEngine::plain_lines`.
- The content pane renders only the `scroll..scroll+height` window (no `Paragraph::scroll`) — cloning every line per frame stalls large files.
- Wide (CJK) graphemes: ratatui keeps the trailing cell blank and its diff skips it while it stays blank, but terminals erase that half themselves (keeping the old bg) when the lead cell is overwritten → one-cell ghosts. `main.rs` keeps the previous frame and `ui/repaint.rs::force_wide_trails` marks those trailing cells `CellDiffOption::AlwaysUpdate`.
- Search modal is a 70%×60% centered overlay using `Clear` widget. Debug overlay (`Ctrl+D`) renders top-right.

### CommitStore batching

- Loads 200 commits at a time via `revwalk` (TOPOLOGICAL). `prefetch_if_near_end()` triggers next batch when selection nears tail.
- After batch load: active filter re-runs `update_filter()`, `selected` remaps to keep cursor stable.

### Pick mode scroll

- Delegates to ratatui `List::scroll_padding(3)` (`ui/pick.rs:201`).
- ratatui keeps padding symmetrically on both sides — directional padding is not supported by the widget.
- 2026-05-24: custom asymmetric margin attempted, rolled back due to interaction complexity with d/u and Ctrl+F/B batch moves. If retrying: hold scroll as a `PickState.scroll` field, render with `skip().take()`, recompute via `normalize_pick_scroll(prev_selected)` after every move, handle `Event::Resize`.

### Config

- `~/.config/gluck/config.toml` (`dirs::config_dir()`). Sections: `[theme]`, `[ui]` (scroll_lines, word_wrap), `[search]` (index_dir, batch_size, max_file_bytes, result_limit).
- Missing file → `Config::default()`; does NOT auto-create. Saved on theme cycle.
- `[ask]` + `[ask.providers.<name>]`: config values win, empty ones fall back to the preset (`ask/provider.rs::PRESETS`) and its env var; empty `provider` auto-detects the first preset whose env key is set. `save()` writes the file 0600 from creation (keys may live there). `Config::load_or_default()` keeps a parse error in `load_error`; `save()` then refuses, so a typo (e.g. `detail = "Deep"`) can't wipe stored keys on the next theme save; `ProviderConfig`/`Endpoint` mask keys in `Debug`. App tests resolve with an empty env so they never hit a provider.
- Diff line `content` keeps its trailing newline — strip it when serializing (`ask/context.rs::strip_eol`).

### Repo watch

- Main loop is poll-based (`event::poll(1s)`; 80ms while indexing). `App::poll_repo_watch()` checks `GitRepo::head_info()` (oid + ref shorthand) every `HEAD_POLL_INTERVAL` (5s).
- On change: Pick mode rebuilds `CommitStore` immediately (filter re-applied, selection restored by oid); View/Diff only set `repo_changed` and show a footer notice — the refresh applies in `back()`.
- Diff/tree caches are oid-keyed and immutable — never cleared on refresh.

## Notable dependencies

Authoritative list lives in `Cargo.toml`. Non-obvious points:

- `tree-sitter` / `tree-sitter-highlight` 0.27 (runtime ABI 13–15). Language crates connect through `tree-sitter-language`, so their versions are independent of the runtime. `Highlighter::highlight` takes `(config, source, encoding, cancellation_flag, injection_cb)`; `QueryCursor::matches` returns a `StreamingIterator` (`while let Some(m) = matches.next()`, `m.captures()`).
- `model2vec-rs` 0.2 with `hf-hub` feature pulls the embedding model at runtime on first index build.
- `turbovec` 1.0 is pure Rust (no BLAS). Its TQ+ calibration is explicit: `VectorIndex::calibrate()` must run before the first `add` on a full build, or the index silently loses recall. Incremental updates reuse the calibration stored in the file and persist via `sync` (append-only). Warnings go through `turbovec::set_warning_hook` → `tracing` (`main.rs`).
- `silence.rs` uses `libc::dup2` — porting to Windows requires a no-op fallback.
- `ureq` 3 (Ask client) is configured with `http_status_as_error(false)` so provider error bodies survive. ureq has no per-chunk idle timeout, so the stream is bounded by `timeout_recv_response` (60s) + `timeout_recv_body` (300s); `parse_sse` also calls back with `""` on every non-content line so a closed panel cancels before the first token, and treats EOF without `[DONE]`/`finish_reason` as an error (never cached). `hf-hub` still pulls `ureq` 2 — two versions coexist.

## Planning artifacts

- Design specs → `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`
- Implementation plans → `docs/superpowers/plans/YYYY-MM-DD-<feature>.md`
- V2 / current-cycle plans → `docs/plans/`
- NEVER save to `.opencode/plans/` — transient only.

## CI

- `.github/workflows/ci.yml`: check + test + clippy + rustfmt on push/PR to main.
- `.github/workflows/release.yml`: `v*` tag → builds for aarch64-apple-darwin + x86_64-pc-windows-msvc → GitHub Release → updates Homebrew tap.

## Conventions

- Commit messages in Korean, code identifiers/comments in English.
- `cargo fmt --check` fails on legacy files — format only files you touch.
- `.gitignore` covers `target/`, `*.log`, `.DS_Store`, `.glc-index/`.
