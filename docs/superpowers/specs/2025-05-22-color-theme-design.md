# Color Theme Design

## Overview

gluck에 6가지 컬러 테마를 추가하고, `~/.config/gluck/config.toml`에 선택을 영속화한다.
`Ctrl+T`로 실시간 순환 전환. UI 크롬과 Syntax Highlight를 하나의 Palette가 함께 커버한다.

## Themes

| # | Name | Type | Key trait |
|---|------|------|-----------|
| 1 | `plain` | Dark (default) | Minimal, current-like palette (white/gray) |
| 2 | `catppuccin` | Dark | Warm, modern, highest popularity |
| 3 | `tokyo-night` | Dark | Cool blue-toned, clean readability |
| 4 | `nord` | Dark | Calm blue-gray, eye-comfort |
| 5 | `gruvbox` | Dark | Retro warm, distinctive personality |
| 6 | `one-light` | Light | Clean light theme |

## Palette Struct (`src/theme.rs`)

```rust
pub struct Palette {
    // --- Base UI ---
    pub fg: Color,           // Primary text
    pub bg: Color,           // Background (terminal default)
    pub dim: Color,          // Secondary text (line numbers, dates, hints)
    pub border: Color,       // Block borders, separators

    // --- Accent ---
    pub accent: Color,       // Selection highlight background, logo
    pub accent_fg: Color,    // Highlighted text on accent background

    // --- Status colors ---
    pub added: Color,        // Added lines (diff, stat)
    pub removed: Color,      // Removed lines (diff, stat)
    pub warning: Color,      // Search bar, markers

    // --- Syntax Highlight ---
    pub syn_keyword: Color,
    pub syn_string: Color,
    pub syn_comment: Color,
    pub syn_type: Color,
    pub syn_function: Color,
    pub syn_constant: Color,
    pub syn_variable: Color,
    pub syn_operator: Color,
}
```

16 fields total: 9 UI + 7 Syntax.

Each theme is a `pub fn plain() -> Palette` factory function.
Registered in a static `THEMES: &[(&str, fn() -> Palette)]` array.

## Persistence

- **Path**: `~/.config/gluck/config.toml` (XDG standard via `dirs` crate)
  - `Config::load()` creates parent directory if missing
  - `Config::save()` writes atomically (write to temp + rename)
- **Format**:
  ```toml
  [theme]
  name = "catppuccin"
  ```
- **Crates added**: `serde`, `toml`, `dirs`
- `Config` struct in `src/config.rs` with `ThemeConfig { name: Option<String> }`
- Default fallback: `"plain"` if file missing or name unrecognized

## Theme Switching

- **Key**: `Ctrl+T` (add to `KeyBindings` as `Action::NextTheme`)
- **Behavior**: Cycle to next theme in `THEMES` array, update `app.palette`, write to `config.toml`
- **Footer display**: `PICK · catppuccin` — show current theme name next to mode

## Rendering Changes

- `App` gains `palette: Palette` and `config: Config` fields
- All UI render functions reference `app.palette` instead of hardcoded colors
- `HighlightEngine::default_theme()` replaced by `Palette::to_highlight_map()` which
  maps semantic syntax fields to capture names. The 7 syn_* fields map to the 22+
  capture names using grouping (e.g., syn_keyword → keyword + keyword.* patterns,
  syn_comment → comment, syn_type → type + type.builtin, etc.)

## Files to touch

| File | Change |
|------|--------|
| `src/theme.rs` | **New** — Palette struct, 6 theme factories, THEMES array, to_highlight_map() |
| `src/config.rs` | **New** — Config struct, load/save, ThemeConfig |
| `src/app.rs` | Add `palette`, `config` fields; add `next_theme()` method; wire into init |
| `src/mode.rs` | Add `Action::NextTheme`, keybinding `Ctrl+T` |
| `src/main.rs` | Load config before App::new |
| `src/ui/layout.rs` | Use `app.palette` instead of inline styles |
| `src/ui/pick.rs` | Use `app.palette` |
| `src/ui/view.rs` | Use `app.palette` |
| `src/ui/diff.rs` | Use `app.palette` |
| `src/highlight/engine.rs` | Remove `default_theme()`, accept external highlight map |
| `Cargo.toml` | Add `serde`, `toml`, `dirs` |

## Out of scope

- Per-project `.gluck.toml` overrides
- Custom color overrides in config
- Live preview before selecting theme
- Theme-dependent terminal OSC sequences
