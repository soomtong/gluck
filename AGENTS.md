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
├── debug.rs                    # tracing-subscriber init
├── lang.rs                     # Language enum + Path-based extension detection
├── git/                        # repo, commit, store (lazy 200-batch), tree, diff, cache (LRU)
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
└── ui/                         # pick, view, diff, search_modal, layout
```

## Commands

```bash
cargo run --bin glc -- [PATH]                    # run TUI
cargo run --bin glc -- index [--force]           # headless index build
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

- **Pick**: commit list + inline diff preview. `/` opens prefix search (CommitIndex tree). Lazy-loads 200-batches; prefetches when selection within 50 of end.
- **View**: file tree + syntax-highlighted content. `.` toggles gitignore filter.
- **Diff**: side-by-side default. `v` toggles unified. `h`/`l` and ←/→ navigate files.
- **Ctrl+N/P** crosses commits while preserving selected file path.
- **Ctrl+T** cycles theme and persists to config.

## Search systems (two separate)

| | Inline (`/`) | Semantic (`s`) |
|---|---|---|
| Backend | `CommitIndex` prefix tree in `CommitStore` | `SearchEngine` (BM25 + vector + RRF) |
| Storage | in-memory, built on commit load | on-disk `.glc-index/`, requires `glc index` |
| Scope | commit message/author/short_id | commits + file content + symbols |

Semantic-search threading: indexing and engine-load each run on their own thread, talk via `mpsc::channel` (`IndexMessage`, `EngineMessage`). `EngineMessage::Ready(Box<SearchEngine>)` hands the heap-allocated engine to the main thread. `with_silenced_stdio()` redirects stderr during model load to keep hf-hub progress bars out of the alternate screen — **Unix-only** (`libc::dup2`).

Index dir `.glc-index/` has `meta.toml` with `INDEX_VERSION` (currently 5), `head_oid`, per-component metadata. Mismatched version forces full rebuild. Mismatched `head_oid` triggers incremental update (BM25 `delete_term` + turbovec `remove` for stale docs, embed only the delta) when the old `head_oid` is still reachable; otherwise falls back to full rebuild.

## Architecture gotchas

### Syntax highlighting

- `HighlightConfiguration::new()` signature in `tree-sitter-highlight` 0.22 is `(language, name, highlights, injections, locals)`. A past bug passed query strings into the `name` slot.
- One shared `HIGHLIGHT_NAMES` array drives all language configs via `.configure(HIGHLIGHT_NAMES)`. Adding a language: append names here AND add palette entries in `theme.rs::to_highlight_map()`.
- `tree-sitter-markdown-fork` 0.7.3 doesn't export its highlight query — we embed `MARKDOWN_HIGHLIGHTS_QUERY` inline.
- Currently only **Rust + Markdown** are highlighted. Extension detection goes through `lang::Language::from_path`; non-matching files fall back to plain text.

### Chunking

- `src/search/chunk/` is a module dir: `mod.rs` (Chunk enum), `symbol.rs` (tree-sitter queries), `file.rs` (split policy), `commit.rs` (commit-message split).
- Symbol extraction supported for Rust, Python, JavaScript, TypeScript, TSX, Go. `Language` and `Query` are cached per language via `OnceLock` — never recompiled per file.
- Rust impl/trait containers are NOT chunked; only their `function_item` children become `SymbolKind::Method`.
- WholeFile threshold 8KB. UTF-8 safe slicing via `source.get(range)` to dodge char-boundary panics.

### View / rendering

- `FileContent::Text` carries `raw: String` + `highlighted: Vec<Line<'static>>`. Setting only `raw` makes view fall back to plain text. `load_view_file()` must populate `highlighted` via `self.highlight.highlight(...)`.
- `FileContent::line_count()` prefers `highlighted.len()` over `raw.lines().count()`.
- `ui/view.rs` prepends line numbers when `highlighted` is populated — keep that consistent if you change construction.
- Search modal is a 70%×60% centered overlay using `Clear` widget. Debug overlay (`Ctrl+D`) renders top-right.

### CommitStore batching

- Loads 200 commits at a time via `revwalk` (TOPOLOGICAL). `prefetch_if_near_end()` triggers next batch when selection nears tail.
- After batch load: active filter re-runs `update_filter()`, `selected` remaps to keep cursor stable.

### Pick mode scroll

- Delegates to ratatui `List::scroll_padding(3)` (`ui/pick.rs:201`).
- ratatui keeps padding symmetrically on both sides — directional padding is not supported by the widget.
- 2026-05-24: custom asymmetric margin attempted, rolled back due to interaction complexity with d/u and Ctrl+F/B batch moves. If retrying: hold scroll as a `PickState.scroll` field, render with `skip().take()`, recompute via `normalize_pick_scroll(prev_selected)` after every move, handle `Event::Resize`.

### Config

- `~/.config/gluck/config.toml` (`dirs::config_dir()`). Sections: `[theme]`, `[ui]` (scroll_lines), `[search]` (index_dir, batch_size, max_file_bytes, result_limit).
- Missing file → `Config::default()`; does NOT auto-create. Saved on theme cycle.

## Notable dependencies

Authoritative list lives in `Cargo.toml`. Non-obvious points:

- `tree-sitter` 0.22 paired with language crates at 0.23 — bridged via `LANGUAGE.into_raw()` + `Language::from_raw(ptr as *const _)`.
- `model2vec-rs` 0.2 with `hf-hub` feature pulls the embedding model at runtime on first index build.
- `blas-src` is Accelerate on macOS, OpenBLAS on Linux (required by turbovec/model2vec). CI installs `libopenblas-dev`.
- `silence.rs` uses `libc::dup2` — porting to Windows requires a no-op fallback.

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
