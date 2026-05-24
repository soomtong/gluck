# AGENTS.md — gluck

Compact repo guide for coding agents. When in doubt, prefer executable truth over prose.

## Project identity

- **Name**: `gluck` (crate), **binary**: `glc`
- Single Rust crate, no workspace. Edition 2021.
- A terminal TUI git history file viewer. Three modes: Pick (commit list) → View (file tree + content) → Diff (commit comparison). Plus a semantic search overlay modal.
- Crate version in Cargo.toml: `0.6.0`. Release tags are `v*` format.

## Entrypoints & layout

```
src/
├── main.rs          # CLI parse → GitRepo open → App event loop → ratatui render
├── lib.rs           # Module declarations
├── app.rs           # Central orchestrator: App struct, mode transitions, search/indexing pipeline
├── cli.rs           # clap derive: Cli (path, --log-level, --debug) + Commands::Index subcommand
├── config.rs        # TOML config (~/.config/gluck/config.toml): theme, ui, search settings
├── debug.rs         # tracing-subscriber init (env-filter)
├── mode.rs          # Mode enum, state structs (Pick/View/Diff), Action enum, KeyBindings
├── theme.rs         # Palette struct, 6 built-in themes, highlight map construction
├── git/
│   ├── mod.rs       # Re-exports
│   ├── repo.rs      # GitRepo wrapper around git2::Repository (open, workdir)
│   ├── commit.rs    # CommitInfo struct, list_commits()
│   ├── store.rs     # CommitStore: lazy-loading batches of 200 commits + CommitIndex prefix-tree
│   ├── tree.rs      # FileEntry, list_tree(), read_blob(), is_binary_blob()
│   ├── diff.rs      # DiffResult/DiffFile/DiffLine, compute_diff() via git2 diff_tree_to_tree
│   └── cache.rs     # LRU caches: DiffCache (64 entries), TreeCache (32 entries)
├── highlight/
│   ├── mod.rs       # Re-exports
│   └── engine.rs    # HighlightEngine: tree-sitter-highlight for Rust + Markdown
├── search/
│   ├── mod.rs       # SearchEngine (BM25 + Vector + RRF fusion), DocMeta/DocKind/SearchResult
│   ├── bm25.rs      # Bm25Index: tantivy full-text index
│   ├── vector.rs    # VectorIndex: turbovec ANN for embedding similarity
│   ├── embedding.rs # EmbeddingModel: model2vec-rs StaticModel (potion-multilingual-128M, 256-dim)
│   ├── rrf.rs       # Reciprocal Rank Fusion: merge BM25 + vector result lists
│   ├── chunk.rs     # Split files into Chunk variants (CommitMessage, FileSection)
│   ├── indexer.rs   # build_index(): walks HEAD tree, chunks files, embeds, writes BM25+vector+turbovec
│   ├── modal.rs     # SemanticSearchModal: Closed/Typing/Loading/Results state machine
│   └── silence.rs   # Unix-only: redirect stderr to /dev/null to suppress hf-hub progress bars
└── ui/
    ├── mod.rs       # Re-exports
    ├── pick.rs      # Commit list rendering
    ├── view.rs      # File tree + syntax-highlighted content rendering
    ├── diff.rs      # Side-by-side / unified diff rendering
    ├── search_modal.rs # Centered overlay: loading/input/results rendering
    └── layout.rs    # Shared layout helpers
```

## Developer commands

```bash
# Build & run binary
cargo run --bin glc -- [PATH]

# Run all tests
cargo test

# Run a single test
cargo test test_view_loads_syntax_highlighted_content

# Format (repo has pre-existing formatting debt on unmodified files; only format changed files)
rustfmt src/path/to/changed.rs

# Build search index for a repo (headless, exits after indexing)
cargo run --bin glc -- index [--force] [--batch-size N] [--max-file-bytes N]

# Lint locally (CI runs clippy --all-targets -- -D warnings)
cargo clippy
```

## Testing conventions

- Tests use ephemeral git repos via helpers in `src/git/repo.rs` under `#[cfg(test)] pub mod tests`:
  - `init_test_repo()` → `(TempDir, git2::Repository)`
  - `add_file_commit(&repo, "path", b"content", "msg")`
- Many modules import these helpers (`use crate::git::repo::tests::{init_test_repo, add_file_commit}`).
- Test helpers in individual modules create `CommitInfo` via `make_commit()` helper (uses `Oid::zero()`).
- No external services, no snapshot tests, no expensive suites.

## Application architecture

### Event loop (`src/main.rs`)

1. Parse CLI → open `GitRepo` → handle `Commands::Index` subcommand (headless, exits after indexing).
2. Load `Config` from `~/.config/gluck/config.toml`.
3. Create `App::new()` → `ratatui::init()` → loop: `terminal.draw(|f| app.render(f))` → `event::read()` → dispatch.
4. During indexing: `is_indexing()` returns true → poll with 80ms timeout, drain channel messages each tick.

### Mode state machine

```
Pick ──Enter──→ View ──Tab──→ Diff
  ↑              ↑            │
  │              └────Tab─────┘
  └────Back (Esc/h)───────────┘
```

- **Pick**: Shows commit list with inline diff preview for selected commit. Supports prefix-based text search (`/`). Lazy-loads commits in batches of 200 with prefetch when near end.
- **View**: Shows file tree + syntax-highlighted content for a commit. `.` key toggles gitignore filtering.
- **Diff**: Side-by-side or unified diff between parent and commit. `v` toggles side-by-side mode. In diff mode, `h`/`l` and Left/Right navigate files (not up/down).

### `App` struct key fields

| Field | Purpose |
|-------|---------|
| `mode: Mode` | Current mode state (enum with Pick/View/Diff variants) |
| `store: CommitStore` | Lazy-loaded commit list with `CommitIndex` prefix-tree for O(1) search |
| `diff_cache: DiffCache` | LRU cache (64 entries) keyed by (parent_oid, child_oid) |
| `tree_cache: TreeCache` | LRU cache (32 entries) keyed by commit oid |
| `highlight: HighlightEngine` | tree-sitter syntax highlighter with theme-aware styles |
| `palette: Palette` / `theme_name: String` | Current theme (6 built-in, Ctrl+T cycles) |
| `search_modal: SemanticSearchModal` | Overlay modal state machine |
| `search_engine: Option<SearchEngine>` | Preloaded BM25+vector search engine |
| `index_rx: Option<mpsc::Receiver<IndexMessage>>` | Background indexing channel |
| `engine_rx: Option<mpsc::Receiver<EngineMessage>>` | Background engine loading channel |
| `saved_search: SearchState` | Preserved text search query when entering/exiting View/Diff |

### Commit navigation (Ctrl+N/P) preserves file selection

When navigating between commits in View or Diff mode, the code preserves the currently selected file path across transitions by matching on `path` string.

### Side-by-side diff

Diff mode defaults to `side_by_side: true`. The `ToggleView` action (`v` key) flips this. Rendering in `ui/diff.rs` handles both layouts.

## Architecture gotchas

### Syntax highlighting engine

- `HighlightEngine` registers one `HighlightConfiguration` per language in `register_languages()`.
- **Critical**: `HighlightConfiguration::new()` signature in `tree-sitter-highlight` 0.22 is `(language, name, highlights_query, injection_query, locals_query)`. The previous bug was passing the query string into the `name` slot.
- There is a **single shared** `HIGHLIGHT_NAMES` array (static slice) used by *all* language configs. Each config calls `.configure(HIGHLIGHT_NAMES)` and events emit indices into this unified array. When adding a new language, append its capture names to this array and add matching theme entries in `theme.rs`'s `to_highlight_map()`.
- `tree-sitter-markdown-fork` 0.7.3 does **not** ship query strings in the crate (they are commented out in `lib.rs`). We embed markdown highlight queries as a raw `&str` constant (`MARKDOWN_HIGHLIGHTS_QUERY`).
- Currently only Rust and Markdown are registered. Language detection is extension-based in `detect_language()`.

### Mode transitions

- `Mode::View` uses `FileContent` enum (NotLoaded/Binary/Text with `raw: String` and `highlighted: Vec<Line<'static>>`).
- `load_view_file()` reads blob content AND populates `highlighted` by calling `self.highlight.highlight(&content, &path)`. If you only set `raw`, the view falls back to plain text.
- `FileContent::line_count()` prefers `highlighted.len()` over `raw.lines().count()`.

### Rendering

- `src/ui/view.rs` prepends line numbers when `highlighted` is populated. If you change how `highlighted` is constructed, ensure line numbers remain consistent.
- Search modal renders as a centered overlay (70% width, 60% height) using `Clear` widget to erase background.
- Debug overlay (`Ctrl+D` to toggle) renders at top-right showing mode-specific state.

### CommitStore lazy loading

- `CommitStore` loads commits in batches of 200 via `git2::revwalk` with TOPOLOGICAL sorting.
- `CommitIndex` is a prefix-tree on message tokens, author, and short_id — **not** the same as the search modal's BM25 engine.
- When `prefetch_if_near_end()` detects selection is within 50 of the end, it loads the next batch.
- After batch load: if there's an active filter, `update_filter()` is re-run to rebuild `filtered_indices`, and `selected` is remapped to maintain cursor position.

### Search pipeline (two separate systems)

**Inline text search** (`/` key):
- Uses `CommitIndex` prefix-tree in `CommitStore`.
- Case-insensitive prefix matching on message, author, short_id.
- State tracked in `PickState.search: SearchState` (Idle/Active).

**Semantic search** (`s` key):
- Full hybrid search engine: BM25 (tantivy) + vector embeddings (model2vec-rs/turbovec) with RRF fusion.
- Requires pre-built index (`glc index` or `I` key from TUI).
- Background threading: indexing spawns a thread, engine loading spawns a thread — both communicate via `mpsc::channel`.
- `with_silenced_stdio()` in `search/silence.rs` redirects stderr to `/dev/null` during model loading to prevent hf-hub progress bars from corrupting the TUI alternate screen. **This is Unix-only** (uses `libc::dup2`).
- `EngineMessage::Ready(Box<SearchEngine>)` passes a heap-allocated engine back to the main thread.
- `IndexMessage::Progress` updates the modal's loading state in real-time during indexing.
- Index stored in `.glc-index/` directory at repo root. `meta.toml` tracks `INDEX_VERSION` (currently 3), `head_oid`, and per-component metadata.

### Search modal state machine

```
Closed → Typing → (as you type) → Results
         ↓
       Loading (indexing / model loading)
         ↓
       Typing or Results (after engine ready)
```

- In `Loading` state, `Esc` closes, `I`/`i` triggers reindex.
- In `Typing`/`Results`, `Esc` closes, `Backspace` deletes, `Up`/`Down` navigates, `Enter` selects and transitions to View/Diff mode.
- When modal is open, all other key handling is bypassed in `App::handle_key()`.
- Selected results navigate to: `DocKind::Commit` → Diff mode (or View if initial commit), `DocKind::File | Symbol` → View mode with file scrolled to line.

### Config

- Path: `~/.config/gluck/config.toml` (uses `dirs::config_dir()`).
- Sections: `[theme]` (name), `[ui]` (scroll_lines), `[search]` (index_dir, batch_size, max_file_bytes, result_limit).
- Saving config is done on theme change (Ctrl+T).
- If config file is missing, `Config::load()` returns `Config::default()` — does not auto-create the file.

### Theme system

- 6 built-in themes: `plain`, `catppuccin`, `tokyo-night`, `nord`, `gruvbox`, `one-light`.
- `Palette::to_highlight_map()` builds a `HashMap<String, Style>` from palette colors using capture names matching `HIGHLIGHT_NAMES`.
- Theme is applied to both UI widgets (via `app.palette` fields) and syntax highlighting (via `highlight.set_theme()`).
- `Ctrl+T` cycles through themes and saves to config.

### CommitStore search vs. semantic search

These are **completely separate** systems:
- `CommitStore::search()` → `CommitIndex` prefix-tree (in-memory, built during load).
- `SearchEngine::search()` → BM25 + vector + RRF (disk-backed, requires `.glc-index/`).

`PickState::update_filter()` uses the former; `App::run_semantic_search()` uses the latter.

## Dependencies to know

- `ratatui` 0.29 — TUI framework. `Paragraph`, `List`, `Block`, `Clear` are the main widgets.
- `crossterm` 0.28 — input handling. Key events map to `Action` via `KeyBindings` in `src/mode.rs`.
- `git2` 0.20 — libgit2 Rust bindings. All git operations go through `GitRepo` wrapper.
- `tree-sitter` 0.22, `tree-sitter-highlight` 0.22 — syntax highlighting.
- `tree-sitter-rust` 0.23, `tree-sitter-markdown-fork` 0.7.3 — language grammars.
- `tantivy` 0.22 — full-text search (BM25 index backend).
- `model2vec-rs` 0.2 (feature `hf-hub`) — embedding model (`minishlab/potion-multilingual-128M`, 256-dim).
- `turbovec` 0.5 — approximate nearest neighbor vector index.
- `clap` 4 (derive) — CLI parsing.
- `serde` / `serde_json` / `toml` 0.8 — config and index metadata serialization.
- `dirs` 6 — platform config directory resolution.
- `libc` 0.2 — stderr redirection during model loading (Unix only).
- `blas-src` — Accelerate on macOS, OpenBLAS on Linux (required by turbovec/model2vec).
- `tracing` / `tracing-subscriber` — logging (env-filter feature, default level: warn).
- `anyhow` / `thiserror` 2 — error handling.
- `tempfile` 3 (dev) — test ephemeral directories.

## CI

- `.github/workflows/ci.yml`: check, test, lint (clippy + rustfmt) on push/PR to main.
- `.github/workflows/release.yml`: triggers on `v*` tags → builds for aarch64-apple-darwin and x86_64-pc-windows-msvc → creates GitHub Release → updates Homebrew tap.
- Build requires `libopenblas-dev` on Linux (CI installs it).

## Notes

- `cargo fmt --check` fails on many pre-existing files. Only format files you touch.
- `.gitignore` ignores `target/`, `*.log`, `.DS_Store`, `.glc-index/`.
- The `silence.rs` module is Unix-only. CI runs on Ubuntu. If targeting Windows, this module would need a no-op fallback.
- Commit messages are in Korean but code identifiers and comments are in English.

### Pick mode scroll behavior

- Pick 모드 커밋 리스트는 ratatui `List` 위젯의 `scroll_padding(3)` (src/ui/pick.rs:201)에 스크롤을 위임한다.
- ratatui는 선택 아이템 기준 **상하 양쪽**에 동시에 N개 패딩을 유지한다. 방향별 비대칭 마진(아래로 내려갈 땐 하단만, 위로 올라갈 땐 상단만)은 ratatui에서 지원하지 않는다.
- `2026-05-24`: 커스텀 스크롤 마진 구현을 시도했으나 d/u, Ctrl+F/B 등 배치 이동과의 상호작용이 까다로워 롤백. `normalize_pick_scroll(prev)` + 수동 window slicing 접근이 작동했지만, ratatui 기본 동작의 단순함을 유지하기로 결정.
- 향후 재시도 시 참고할 접근법:
  - `PickState.scroll` 필드를 수동 오프셋으로 활용
  - 렌더링: `skip(scroll).take(visible_height)` 로 직접 윈도우 슬라이싱
  - 이동: `normalize_pick_scroll(prev_selected)` 하나로 모든 move 연산 후 scroll 재계산
  - `term_height` 추적용 `Event::Resize` 핸들링, `scroll_margin` config 필드 필요

## Planning artifacts

- **Design docs** → `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`
- **Implementation plans** → `docs/superpowers/plans/YYYY-MM-DD-<feature-name>.md`
- **V2 plans** → `docs/plans/` (e.g., semantic-search-design-v2.md)
- Do NOT save to `.opencode/plans/` — that directory is for transient plans only.
