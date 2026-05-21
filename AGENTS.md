# AGENTS.md — gluck

Compact repo guide for coding agents. When in doubt, prefer executable truth over prose.

## Project identity

- **Name**: `gluck` (crate), **binary**: `glc`
- Single Rust crate, no workspace. Edition 2021.
- A terminal TUI git history file viewer. Three modes: Pick (commit list) → View (file tree + content) → Diff (commit comparison).

## Entrypoints & layout

- `src/main.rs` → parses CLI with `clap` → opens `GitRepo` → runs `App` event loop → `ratatui` render.
- `src/lib.rs` exposes modules: `app`, `cli`, `debug`, `git`, `highlight`, `mode`, `ui`.
- `src/app.rs` is the heart: `App::render()` dispatches to `ui::{pick,view,diff}` based on `Mode`.
- `src/highlight/engine.rs` drives syntax highlighting via `tree-sitter-highlight` 0.22.

## Developer commands

```bash
# Build & run binary
cargo run --bin glc -- [PATH]

# Run all tests (unit tests only; no integration tests)
cargo test

# Run a single test
cargo test test_view_loads_syntax_highlighted_content

# Format (repo has pre-existing formatting debt on unmodified files; only format changed files)
rustfmt src/path/to/changed.rs
```

## Testing conventions

- Tests use ephemeral git repos via helpers in `src/git/repo.rs` under `#[cfg(test)] pub mod tests`:
  - `init_test_repo()` → `(TempDir, git2::Repository)`
  - `add_file_commit(&repo, "path", b"content", "msg")`
- Many modules import these helpers (`use crate::git::repo::tests::{init_test_repo, add_file_commit}`).
- No external services, no snapshot tests, no expensive suites.

## Architecture gotchas

### Syntax highlighting engine

- `HighlightEngine` registers one `HighlightConfiguration` per language in `register_languages()`.
- **Critical**: `HighlightConfiguration::new()` signature in `tree-sitter-highlight` 0.22 is `(language, name, highlights_query, injection_query, locals_query)`. The previous bug was passing the query string into the `name` slot.
- There is a **single shared** `HIGHLIGHT_NAMES` array (static slice) used by *all* language configs. Each config calls `.configure(HIGHLIGHT_NAMES)` and events emit indices into this unified array. When adding a new language, append its capture names to this array and add matching theme entries.
- `tree-sitter-markdown-fork` 0.7.3 does **not** ship query strings in the crate (they are commented out in `lib.rs`). We embed markdown highlight queries as a raw `&str` constant (`MARKDOWN_HIGHLIGHTS_QUERY`).

### Mode transitions

- `Mode::View` holds `content: Option<String>` and `highlighted: Vec<Line>`.
- `load_view_file()` in `App` reads blob content AND populates `highlighted` by calling `self.highlight.highlight(&content, &path)`. If you only set `content`, the view falls back to plain text.

### Rendering

- `src/ui/view.rs` prepends line numbers when `highlighted` is populated. If you change how `highlighted` is constructed, ensure line numbers remain consistent.

## Dependencies to know

- `ratatui` 0.29 — TUI framework. `Paragraph`, `List`, `Block` are the main widgets.
- `crossterm` 0.28 — input handling. Key events map to `Action` via `KeyBindings` in `src/mode.rs`.
- `git2` 0.20 — libgit2 Rust bindings. All git operations go through `GitRepo` wrapper.
- `tree-sitter` 0.22, `tree-sitter-highlight` 0.22 — syntax highlighting.

## Notes

- `cargo fmt --check` fails on many pre-existing files. Only format files you touch.
- `.gitignore` ignores `target/`, `*.log`, `.DS_Store`.
- No CI, no pre-commit hooks, no custom toolchain config.
