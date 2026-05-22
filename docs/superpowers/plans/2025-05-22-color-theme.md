# Color Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 6 color themes (plain, catppuccin, tokyo-night, nord, gruvbox, one-light) to gluck with Ctrl+T cycling and TOML config persistence.

**Architecture:** New `src/theme.rs` defines `Palette` struct (16 semantic color fields) + 6 factory functions. New `src/config.rs` handles XDG `config.toml` load/save. `App` gains `palette`, `theme_name`, `config` fields. All UI render functions reference `app.palette` instead of inline styles. `HighlightEngine` receives its highlight map from `Palette::to_highlight_map()` via `set_theme()`.

**Tech Stack:** ratatui Color, serde, toml, dirs

---

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Append new dependencies**

After line 22 (`thiserror = "2.0.18"`), add:

```toml
serde = { version = "1", features = ["derive"] }
toml = "0.8"
dirs = "6"
```

- [ ] **Step 2: Run cargo check**

```bash
cargo check
```

Expected: dependencies resolve, no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "Add serde, toml, dirs dependencies for theme config"
```

---

### Task 2: Create theme module

**Files:**
- Create: `src/theme.rs`
- Modify: `src/highlight/engine.rs` (make HIGHLIGHT_NAMES public)

- [ ] **Step 1: Write src/theme.rs**

```rust
use ratatui::style::{Color, Modifier, Style};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Palette {
    pub fg: Color,
    pub bg: Color,
    pub dim: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_fg: Color,
    pub added: Color,
    pub removed: Color,
    pub warning: Color,
    pub syn_keyword: Color,
    pub syn_string: Color,
    pub syn_comment: Color,
    pub syn_type: Color,
    pub syn_function: Color,
    pub syn_constant: Color,
    pub syn_variable: Color,
    pub syn_operator: Color,
}

impl Palette {
    pub fn plain() -> Self {
        Self {
            fg: Color::White,
            bg: Color::Reset,
            dim: Color::DarkGray,
            border: Color::White,
            accent: Color::White,
            accent_fg: Color::Black,
            added: Color::Green,
            removed: Color::Red,
            warning: Color::Yellow,
            syn_keyword: Color::Magenta,
            syn_string: Color::Green,
            syn_comment: Color::DarkGray,
            syn_type: Color::Cyan,
            syn_function: Color::Blue,
            syn_constant: Color::Yellow,
            syn_variable: Color::White,
            syn_operator: Color::Yellow,
        }
    }

    pub fn catppuccin() -> Self {
        Self {
            fg: Color::Rgb(205, 214, 244),
            bg: Color::Rgb(30, 30, 46),
            dim: Color::Rgb(88, 91, 112),
            border: Color::Rgb(69, 71, 90),
            accent: Color::Rgb(203, 166, 247),
            accent_fg: Color::Rgb(30, 30, 46),
            added: Color::Rgb(166, 227, 161),
            removed: Color::Rgb(243, 139, 168),
            warning: Color::Rgb(249, 226, 175),
            syn_keyword: Color::Rgb(203, 166, 247),
            syn_string: Color::Rgb(166, 227, 161),
            syn_comment: Color::Rgb(88, 91, 112),
            syn_type: Color::Rgb(137, 220, 235),
            syn_function: Color::Rgb(137, 180, 250),
            syn_constant: Color::Rgb(250, 179, 135),
            syn_variable: Color::Rgb(205, 214, 244),
            syn_operator: Color::Rgb(148, 226, 213),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            fg: Color::Rgb(192, 202, 245),
            bg: Color::Rgb(26, 27, 38),
            dim: Color::Rgb(68, 75, 106),
            border: Color::Rgb(41, 46, 66),
            accent: Color::Rgb(122, 162, 247),
            accent_fg: Color::Rgb(26, 27, 38),
            added: Color::Rgb(158, 206, 106),
            removed: Color::Rgb(247, 118, 142),
            warning: Color::Rgb(224, 175, 104),
            syn_keyword: Color::Rgb(187, 154, 247),
            syn_string: Color::Rgb(158, 206, 106),
            syn_comment: Color::Rgb(68, 75, 106),
            syn_type: Color::Rgb(122, 162, 247),
            syn_function: Color::Rgb(122, 162, 247),
            syn_constant: Color::Rgb(255, 158, 100),
            syn_variable: Color::Rgb(192, 202, 245),
            syn_operator: Color::Rgb(137, 221, 255),
        }
    }

    pub fn nord() -> Self {
        Self {
            fg: Color::Rgb(216, 222, 233),
            bg: Color::Rgb(46, 52, 64),
            dim: Color::Rgb(76, 86, 106),
            border: Color::Rgb(59, 66, 82),
            accent: Color::Rgb(136, 192, 208),
            accent_fg: Color::Rgb(46, 52, 64),
            added: Color::Rgb(163, 190, 140),
            removed: Color::Rgb(191, 97, 106),
            warning: Color::Rgb(235, 203, 139),
            syn_keyword: Color::Rgb(180, 142, 173),
            syn_string: Color::Rgb(163, 190, 140),
            syn_comment: Color::Rgb(76, 86, 106),
            syn_type: Color::Rgb(136, 192, 208),
            syn_function: Color::Rgb(136, 192, 208),
            syn_constant: Color::Rgb(208, 135, 112),
            syn_variable: Color::Rgb(216, 222, 233),
            syn_operator: Color::Rgb(143, 188, 187),
        }
    }

    pub fn gruvbox() -> Self {
        Self {
            fg: Color::Rgb(235, 219, 178),
            bg: Color::Rgb(40, 40, 40),
            dim: Color::Rgb(146, 131, 116),
            border: Color::Rgb(60, 56, 54),
            accent: Color::Rgb(250, 189, 47),
            accent_fg: Color::Rgb(40, 40, 40),
            added: Color::Rgb(184, 187, 38),
            removed: Color::Rgb(251, 73, 52),
            warning: Color::Rgb(250, 189, 47),
            syn_keyword: Color::Rgb(211, 134, 155),
            syn_string: Color::Rgb(184, 187, 38),
            syn_comment: Color::Rgb(146, 131, 116),
            syn_type: Color::Rgb(142, 192, 124),
            syn_function: Color::Rgb(142, 192, 124),
            syn_constant: Color::Rgb(254, 128, 25),
            syn_variable: Color::Rgb(235, 219, 178),
            syn_operator: Color::Rgb(131, 165, 152),
        }
    }

    pub fn one_light() -> Self {
        Self {
            fg: Color::Rgb(56, 58, 66),
            bg: Color::Rgb(250, 250, 250),
            dim: Color::Rgb(160, 161, 167),
            border: Color::Rgb(220, 221, 225),
            accent: Color::Rgb(64, 120, 242),
            accent_fg: Color::Rgb(250, 250, 250),
            added: Color::Rgb(80, 161, 79),
            removed: Color::Rgb(228, 86, 73),
            warning: Color::Rgb(193, 132, 1),
            syn_keyword: Color::Rgb(166, 38, 164),
            syn_string: Color::Rgb(80, 161, 79),
            syn_comment: Color::Rgb(160, 161, 167),
            syn_type: Color::Rgb(64, 120, 242),
            syn_function: Color::Rgb(64, 120, 242),
            syn_constant: Color::Rgb(152, 104, 1),
            syn_variable: Color::Rgb(56, 58, 66),
            syn_operator: Color::Rgb(1, 132, 188),
        }
    }

    pub fn highlight_style(&self) -> Style {
        Style::new().fg(self.accent_fg).bg(self.accent)
    }

    pub fn to_highlight_map(&self) -> HashMap<String, Style> {
        let mut m = HashMap::new();
        m.insert("keyword".into(), Style::new().fg(self.syn_keyword).add_modifier(Modifier::BOLD));
        m.insert("function".into(), Style::new().fg(self.syn_function));
        m.insert("function.builtin".into(), Style::new().fg(self.syn_type));
        m.insert("string".into(), Style::new().fg(self.syn_string));
        m.insert("string.special".into(), Style::new().fg(self.syn_type));
        m.insert("comment".into(), Style::new().fg(self.syn_comment));
        m.insert("type".into(), Style::new().fg(self.syn_type));
        m.insert("type.builtin".into(), Style::new().fg(self.syn_type));
        m.insert("constant".into(), Style::new().fg(self.syn_constant));
        m.insert("variable".into(), Style::new().fg(self.syn_variable));
        m.insert("variable.builtin".into(), Style::new().fg(self.syn_type));
        m.insert("variable.parameter".into(), Style::new().fg(self.syn_variable));
        m.insert("operator".into(), Style::new().fg(self.syn_operator));
        m.insert("punctuation".into(), Style::new().fg(self.dim));
        m.insert("punctuation.bracket".into(), Style::new().fg(self.dim));
        m.insert("punctuation.delimiter".into(), Style::new().fg(self.dim));
        m.insert("property".into(), Style::new().fg(self.syn_variable));
        m.insert("attribute".into(), Style::new().fg(self.syn_constant));
        m.insert("tag".into(), Style::new().fg(self.syn_type));
        m.insert("text.title".into(), Style::new().fg(self.syn_keyword).add_modifier(Modifier::BOLD));
        m.insert("text.literal".into(), Style::new().fg(self.syn_string));
        m.insert("text.emphasis".into(), Style::new().fg(self.accent));
        m.insert("text.strong".into(), Style::new().fg(self.accent).add_modifier(Modifier::BOLD));
        m.insert("text.uri".into(), Style::new().fg(self.syn_type).add_modifier(Modifier::UNDERLINED));
        m.insert("text.reference".into(), Style::new().fg(self.syn_type));
        m.insert("punctuation.special".into(), Style::new().fg(self.dim));
        m.insert("string.escape".into(), Style::new().fg(self.syn_constant));
        m
    }
}

pub static THEMES: &[(&str, fn() -> Palette)] = &[
    ("plain", Palette::plain),
    ("catppuccin", Palette::catppuccin),
    ("tokyo-night", Palette::tokyo_night),
    ("nord", Palette::nord),
    ("gruvbox", Palette::gruvbox),
    ("one-light", Palette::one_light),
];

pub fn find_theme(name: &str) -> Option<fn() -> Palette> {
    THEMES.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
}

pub fn default_theme_name() -> &'static str {
    "plain"
}

pub fn resolve_palette(name: Option<&str>) -> Palette {
    name.and_then(|n| find_theme(n))
        .unwrap_or_else(Palette::plain)()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_themes_have_non_default_colors() {
        for (name, factory) in THEMES {
            let p = factory();
            assert_ne!(p.fg, Color::Reset, "{name}: fg must be set");
            assert_ne!(p.accent, Color::Reset, "{name}: accent must be set");
        }
    }

    #[test]
    fn test_plain_theme_matches_expected() {
        let p = Palette::plain();
        assert_eq!(p.fg, Color::White);
        assert_eq!(p.dim, Color::DarkGray);
        assert_eq!(p.accent, Color::White);
        assert_eq!(p.accent_fg, Color::Black);
    }

    #[test]
    fn test_find_theme_case_sensitive() {
        assert!(find_theme("catppuccin").is_some());
        assert!(find_theme("nonexistent").is_none());
    }

    #[test]
    fn test_resolve_palette_falls_back_to_plain() {
        let p = resolve_palette(Some("bogus"));
        assert_eq!(p.fg, Color::White);
    }

    #[test]
    fn test_resolve_palette_loads_named_theme() {
        let p = resolve_palette(Some("nord"));
        assert_eq!(p.fg, Color::Rgb(216, 222, 233));
    }

    #[test]
    fn test_highlight_map_is_non_empty() {
        let p = Palette::plain();
        let map = p.to_highlight_map();
        assert!(!map.is_empty());
        assert!(map.contains_key("keyword"));
        assert!(map.contains_key("comment"));
        assert!(map.contains_key("string"));
    }

    #[test]
    fn test_highlight_style_is_fg_on_accent() {
        let p = Palette::plain();
        let s = p.highlight_style();
        assert_eq!(s.fg, Some(Color::Black));
        assert_eq!(s.bg, Some(Color::White));
    }
}
```

- [ ] **Step 2: Make HIGHLIGHT_NAMES public in engine.rs**

In `src/highlight/engine.rs` line 166, change:

```rust
const HIGHLIGHT_NAMES: &[&str] = &[
```

to:

```rust
pub const HIGHLIGHT_NAMES: &[&str] = &[
```

- [ ] **Step 3: Run tests**

```bash
cargo test theme
```

Expected: 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/theme.rs src/highlight/engine.rs
git commit -m "Add Palette struct and 6 color themes"
```

---

### Task 3: Create config module

**Files:**
- Create: `src/config.rs`

- [ ] **Step 1: Write src/config.rs**

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: crate::theme::default_theme_name().to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self)
            .context("failed to serialize config")?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write config: {}", path.display()))?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("gluck");
    path.push("config.toml");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_plain_theme() {
        let config = Config::default();
        assert_eq!(config.theme.name, "plain");
    }

    #[test]
    fn test_config_roundtrip() {
        let config = Config {
            theme: ThemeConfig {
                name: "nord".to_string(),
            },
        };
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.theme.name, "nord");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test config
```

Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/config.rs
git commit -m "Add Config module with XDG config.toml persistence"
```

---

### Task 4: Wire theme into App and main

**Files:**
- Modify: `src/lib.rs`, `src/app.rs`, `src/main.rs`

- [ ] **Step 1: Register new modules in lib.rs**

In `src/lib.rs`, add after `pub mod mode;`:

```rust
pub mod config;
pub mod theme;
```

- [ ] **Step 2: Update App struct and imports in app.rs**

In `src/app.rs`, add imports after line 7 (`use crate::ui;`):

```rust
use crate::config::Config;
use crate::theme::Palette;
```

Change the `App` struct (lines 13-21) to:

```rust
pub struct App {
    pub mode: Mode,
    pub repo: GitRepo,
    pub commits: Vec<CommitInfo>,
    pub keybindings: KeyBindings,
    pub should_quit: bool,
    pub debug_overlay: bool,
    pub highlight: HighlightEngine,
    pub palette: Palette,
    pub theme_name: String,
    pub config: Config,
}
```

- [ ] **Step 3: Update App::new()**

Replace the existing `App::new()` (lines 24-38) with:

```rust
pub fn new(repo: GitRepo, config: Config) -> Result<Self> {
    let commits = list_commits(&repo)?;
    let pick_state = PickState::new(commits.clone());
    let theme_name = config.theme.name.clone();
    let palette = crate::theme::resolve_palette(Some(&theme_name));
    let mut app = Self {
        mode: Mode::Pick(pick_state),
        repo,
        commits,
        keybindings: KeyBindings::default_bindings(),
        should_quit: false,
        debug_overlay: false,
        highlight: HighlightEngine::new(),
        palette,
        theme_name,
        config,
    };
    app.highlight.set_theme(app.palette.to_highlight_map());
    app.update_pick_diff();
    Ok(app)
}
```

- [ ] **Step 4: Add next_theme() method and Ctrl+T handler**

Add the following method inside `impl App` (before the test module):

```rust
fn next_theme(&mut self) {
    let names: Vec<&str> = crate::theme::THEMES.iter().map(|(n, _)| *n).collect();
    let current_idx = names.iter().position(|&n| n == self.theme_name).unwrap_or(0);
    let next_idx = (current_idx + 1) % names.len();
    self.theme_name = names[next_idx].to_string();
    self.palette = crate::theme::resolve_palette(Some(&self.theme_name));
    self.highlight.set_theme(self.palette.to_highlight_map());
    self.config.theme.name = self.theme_name.clone();
    let _ = self.config.save();
}
```

In `handle_ctrl_key` (lines 132-139), add the Ctrl+T case:

```rust
pub fn handle_ctrl_key(&mut self, code: KeyCode) {
    match code {
        KeyCode::Char('c') => self.should_quit = true,
        KeyCode::Char('d') => self.debug_overlay = !self.debug_overlay,
        KeyCode::Char('p') => self.prev_commit(),
        KeyCode::Char('n') => self.next_commit(),
        KeyCode::Char('t') => self.next_theme(),
        _ => {}
    }
}
```

- [ ] **Step 5: Update test_app() helper and all test call sites**

In `src/app.rs`, update the `test_app()` helper (line 611):

```rust
fn test_app() -> (tempfile::TempDir, App) {
    let (dir, repo) = init_test_repo();
    add_file_commit(&repo, "a.txt", b"first", "First commit");
    add_file_commit(&repo, "b.txt", b"second", "Second commit");
    add_file_commit(&repo, "a.txt", b"third", "Third commit");
    let git_repo = GitRepo::open(dir.path()).unwrap();
    let app = App::new(git_repo, Config::default()).unwrap();
    (dir, app)
}
```

Find every remaining `App::new(git_repo).unwrap()` or `App::new(git_repo,` call in `app.rs` tests (there are ~5 in tests that don't use `test_app()`). Replace with `App::new(git_repo, Config::default()).unwrap()`.

- [ ] **Step 6: Update main.rs**

In `src/main.rs`, add the import after line 7:

```rust
use gluck::config::Config;
```

Change line 28 from:

```rust
let mut app = App::new(repo)?;
```

to:

```rust
let config = Config::load().unwrap_or_default();
let mut app = App::new(repo, config)?;
```

- [ ] **Step 7: Run tests**

```bash
cargo test
```

Expected: all tests pass (including existing app tests).

- [ ] **Step 8: Commit**

```bash
git add src/lib.rs src/app.rs src/main.rs
git commit -m "Wire theme into App with Ctrl+T cycling and config persistence"
```

---

### Task 5: Update HighlightEngine

**Files:**
- Modify: `src/highlight/engine.rs`

- [ ] **Step 1: Add set_theme(), remove default_theme(), update test**

Remove the `default_theme()` function (current lines 222-275, the entire fn).

In `impl HighlightEngine`, change `new()` to:

```rust
pub fn new() -> Self {
    let mut engine = Self {
        configs: HashMap::new(),
        theme: HashMap::new(),
    };
    engine.register_languages();
    engine
}

pub fn set_theme(&mut self, theme: HashMap<String, Style>) {
    self.theme = theme;
}
```

Replace the test module (lines 277-292) with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight("# Title\n**bold** text\n", "readme.md");
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in markdown highlight output");
    }

    #[test]
    fn test_highlight_without_theme_produces_no_colors() {
        let mut engine = HighlightEngine::new();
        let lines = engine.highlight("fn main() {}", "main.rs");
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(!has_color, "expected no colors without theme set");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test highlight
```

Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/highlight/engine.rs
git commit -m "Refactor HighlightEngine to accept external theme via set_theme"
```

---

### Task 6: Update layout.rs with palette

**Files:**
- Modify: `src/ui/layout.rs`

- [ ] **Step 1: Rewrite render_header with palette and theme params**

Replace the entire `render_header` function (lines 22-54) with:

```rust
use crate::theme::Palette;

pub fn render_header(
    frame: &mut ratatui::Frame,
    area: Rect,
    palette: &Palette,
    mode: &str,
    theme: &str,
    message: Option<&str>,
) {
    let logo = Span::styled("◆ ", Style::new().fg(palette.accent));
    let name = Span::styled("glc", Style::new().fg(palette.fg).add_modifier(Modifier::BOLD));
    let version = Span::styled(
        format!(" v{}", env!("CARGO_PKG_VERSION")),
        Style::new().fg(palette.dim),
    );
    let sep = Span::styled(" · ", Style::new().fg(palette.dim));
    let mode_span = Span::styled(mode, Style::new().fg(palette.accent).add_modifier(Modifier::BOLD));
    let theme_span = Span::styled(format!(" · {}", theme), Style::new().fg(palette.dim));

    let line = if let Some(msg) = message {
        let prefix_width =
            2 + 3 + 2 + env!("CARGO_PKG_VERSION").len() + mode.len() + theme.len() + 10;
        let available = (area.width as usize).saturating_sub(prefix_width + 2);
        let truncated: String = if msg.len() > available && available > 0 {
            msg.chars()
                .take(available.saturating_sub(1))
                .chain(['…'])
                .collect()
        } else {
            msg.to_string()
        };
        let sep2 = Span::styled(" · ", Style::new().fg(palette.dim));
        let msg_span = Span::styled(truncated, Style::new().fg(palette.dim));
        Line::from(vec![
            logo, name, version, sep, mode_span, theme_span, sep2, msg_span,
        ])
    } else {
        let project = Span::styled(" GLUCK", Style::new().fg(palette.fg));
        let tagline = Span::styled(
            " git log unfolds code into knowledge",
            Style::new().fg(palette.dim).add_modifier(Modifier::ITALIC),
        );
        Line::from(vec![
            logo, name, version, sep, mode_span, theme_span, project, tagline,
        ])
    };

    let header = Paragraph::new(line)
        .block(Block::bordered().border_style(Style::new().fg(palette.border)));
    frame.render_widget(header, area);
}
```

- [ ] **Step 2: Rewrite render_footer with palette param**

Replace `render_footer` (lines 57-71) with:

```rust
pub fn render_footer(
    frame: &mut ratatui::Frame,
    area: Rect,
    palette: &Palette,
    hints: &[(&str, &str)],
) {
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    format!("[{}]", key),
                    Style::new().fg(palette.warning).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {} ", desc)),
            ]
        })
        .collect();
    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}
```

- [ ] **Step 3: Rewrite render_search_bar with palette param**

Replace `render_search_bar` (lines 74-78) with:

```rust
pub fn render_search_bar(
    frame: &mut ratatui::Frame,
    area: Rect,
    palette: &Palette,
    query: &str,
) {
    let search = Paragraph::new(format!("/ {}", query))
        .style(Style::new().fg(palette.warning))
        .block(Block::bordered().border_style(Style::new().fg(palette.border)));
    frame.render_widget(search, area);
}
```

- [ ] **Step 4: Update callers in pick.rs, view.rs, diff.rs**

In `src/ui/pick.rs`, update these calls in `render_pick`:

Find `layout::render_search_bar(frame, header, input)` → change to `layout::render_search_bar(frame, header, &app.palette, input)`

Find `layout::render_header(frame, header, "PICK", None)` (appears twice, lines ~167 and ~170) → change to `layout::render_header(frame, header, &app.palette, "PICK", &app.theme_name, None)`

Find `layout::render_footer(frame, footer, &hints)` (line ~204) → change to `layout::render_footer(frame, footer, &app.palette, &hints)`

In `src/ui/view.rs`, update in `render_view`:

Find `layout::render_header(frame, header, "VIEW", Some(&state.commit.message))` (line ~19) → change to `layout::render_header(frame, header, &app.palette, "VIEW", &app.theme_name, Some(&state.commit.message))`

Find `layout::render_footer(frame, footer, &hints)` (line ~134) → change to `layout::render_footer(frame, footer, &app.palette, &hints)`

In `src/ui/diff.rs`, update in `render_diff`:

Find `layout::render_header(frame, header, &title, Some(&state.to.message))` (line ~15) → change to `layout::render_header(frame, header, &app.palette, &title, &app.theme_name, Some(&state.to.message))`

Find `layout::render_footer(frame, footer, &hints)` (line ~61) → change to `layout::render_footer(frame, footer, &app.palette, &hints)`

- [ ] **Step 5: Verify compile**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/ui/layout.rs src/ui/pick.rs src/ui/view.rs src/ui/diff.rs
git commit -m "Update layout to use palette for all header/footer/search colors"
```

---

### Task 7: Update pick.rs with palette

**Files:**
- Modify: `src/ui/pick.rs`

- [ ] **Step 1: Update format_commit_line signature and body**

Change `format_commit_line` (line 32) to accept palette:

```rust
fn format_commit_line(
    commit: &crate::git::commit::CommitInfo,
    palette: &crate::theme::Palette,
) -> Line<'static> {
    let date_str = format_date(commit.date);
    Line::from(vec![
        Span::styled(
            format!(" {} ", commit.short_id),
            Style::new().fg(palette.warning).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{:<12} ", date_str), Style::new().fg(palette.dim)),
        Span::raw(commit.message.lines().next().unwrap_or("").to_string()),
    ])
}
```

Update the call site in `render_pick` (line ~180):

```rust
.map(|c| ListItem::new(format_commit_line(c, &app.palette)))
```

- [ ] **Step 2: Update render_commit_detail styles**

In `render_commit_detail` (line 68), access palette via `let palette = &app.palette;` at the function top.

Change line 92 (`Style::new().white().add_modifier(Modifier::BOLD)`) to:

```rust
Style::new().fg(palette.fg).add_modifier(Modifier::BOLD)
```

Change line 96 (`Style::new().dark_gray()`) to:

```rust
Style::new().fg(palette.dim)
```

Change lines 107-109 (the desc `Block::bordered().title(...).style(Style::new().white())`) to:

```rust
Block::bordered()
    .title(" Description ")
    .border_style(Style::new().fg(palette.border))
```

Change line 124 (`Style::new().green()`) to:

```rust
Style::new().fg(palette.added)
```

Change line 130 (`Style::new().red()`) to:

```rust
Style::new().fg(palette.removed)
```

Change lines 141-143 (files_list block `.style(Style::new().white())`) to:

```rust
Block::bordered()
    .title(format!(" Files ({}) ", diff.files.len()))
    .border_style(Style::new().fg(palette.border))
```

Change lines 148-153 (no_diff block `.style(Style::new().white())` and `.style(Style::new().dark_gray())`) to:

```rust
let no_diff = Paragraph::new(" (root commit) ")
    .block(
        Block::bordered()
            .title(" Files ")
            .border_style(Style::new().fg(palette.border)),
    )
    .style(Style::new().fg(palette.dim));
```

- [ ] **Step 3: Update render_pick block styles**

Change line 184-188 (the commit list block `.style(Style::new().white())`) to:

```rust
.block(
    Block::bordered()
        .title(format!(" {} commits ", visible.len()))
        .border_style(Style::new().fg(palette.border)),
)
```

Change line 189 (`.highlight_style(Style::new().black().on_white())`) to:

```rust
.highlight_style(palette.highlight_style())
```

- [ ] **Step 4: Verify compile**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/ui/pick.rs
git commit -m "Update pick UI to use palette colors"
```

---

### Task 8: Update view.rs with palette

**Files:**
- Modify: `src/ui/view.rs`

- [ ] **Step 1: Replace all inline colors in render_view**

At the top of `render_view`, add `let palette = &app.palette;`.

Change line 28 (`Style::new().yellow()`) to:

```rust
Span::styled("*", Style::new().fg(palette.warning))
```

Change line 46 (`Style::new().fg(Color::Green)`) to:

```rust
Style::new().fg(palette.added)
```

Change line 53 (`Style::new().fg(Color::Red)`) to:

```rust
Style::new().fg(palette.removed)
```

Change lines 66-68 (tree_list block `.style(Style::new().white())`) to:

```rust
.block(
    Block::bordered()
        .title(format!(" {} ", state.commit.short_id))
        .border_style(Style::new().fg(palette.border)),
)
```

Change line 69 (`Style::new().black().on_white()`) to:

```rust
.highlight_style(palette.highlight_style())
```

Change line 85 (`Style::new().dark_gray()`) — "(select a file to view)" — to:

```rust
Span::styled(
    "(select a file to view)",
    Style::new().fg(palette.dim),
)
```

Change line 91 (`Style::new().dark_gray()`) — "(binary file)" — to:

```rust
Span::styled(
    "(binary file)",
    Style::new().fg(palette.dim),
)
```

Change line 101-102 (`Style::new().dark_gray()`) — line numbers — to:

```rust
Span::styled(
    format!("{:>4} ", i + 1),
    Style::new().fg(palette.dim),
)
```

Change lines 116-118 (content block `.style(Style::new().white())`) to:

```rust
.block(
    Block::bordered()
        .title(format!(" {} ", file_name))
        .border_style(Style::new().fg(palette.border)),
)
```

- [ ] **Step 2: Verify compile**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/ui/view.rs
git commit -m "Update view UI to use palette colors"
```

---

### Task 9: Update diff.rs with palette

**Files:**
- Modify: `src/ui/diff.rs`

- [ ] **Step 1: Update style_for_line to accept palette**

Change `style_for_line` (line 64) to:

```rust
fn style_for_line(line: &DiffLine, palette: &crate::theme::Palette) -> Style {
    match line {
        DiffLine::Added { .. } => Style::new().fg(palette.added),
        DiffLine::Removed { .. } => Style::new().fg(palette.removed),
        DiffLine::Context { .. } => Style::new(),
    }
}
```

Update all call sites of `style_for_line(dl)` to `style_for_line(dl, palette)` in `render_unified` (line 88, 93, 101) and `render_side_by_side` (line 136, 160).

- [ ] **Step 2: Replace inline colors in render_diff**

In `render_diff`, add `let palette = &app.palette;` at the top after the mode check.

Change line 39 (tab highlight `.white().bold()`) to:

```rust
.highlight_style(Style::new().fg(palette.fg).add_modifier(Modifier::BOLD))
```

- [ ] **Step 3: Replace inline colors in render_unified**

Change line 104 (`Style::new().dark_gray()`) — line number in unified — to:

```rust
Span::styled(line_no, Style::new().fg(palette.dim))
```

Change line 112 (block `.style(Style::new().white())`) to:

```rust
.block(Block::bordered().border_style(Style::new().fg(palette.border)))
```

- [ ] **Step 4: Replace inline colors in render_side_by_side**

Change line 139 (`Style::new().dark_gray()`) — old line number — to:

```rust
Span::styled(line_no, Style::new().fg(palette.dim))
```

Change line 163 (`Style::new().dark_gray()`) — new line number — to:

```rust
Span::styled(line_no, Style::new().fg(palette.dim))
```

Change line 170 (old widget block `.style(Style::new().white())`) to:

```rust
.block(Block::bordered().title(" old ").border_style(Style::new().fg(palette.border)))
```

Change line 173 (new widget block `.style(Style::new().white())`) to:

```rust
.block(Block::bordered().title(" new ").border_style(Style::new().fg(palette.border)))
```

- [ ] **Step 5: Verify compile**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/ui/diff.rs
git commit -m "Update diff UI to use palette colors"
```

---

### Task 10: Final verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run all tests**

```bash
cargo test
```

Expected: ALL tests pass.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```

Expected: no warnings or errors.

- [ ] **Step 3: Smoke test**

```bash
cargo run --bin glc
```

Verify:
- App starts with header showing `◆ glc v0.3.1 · PICK · plain ·` plus tagline
- Footer shows yellow key hints
- Press `Ctrl+T`: theme name in header changes, colors change across all UI
- Press `Enter` to view a file: syntax highlighting uses theme colors
- Press `Tab` to see diff: added/removed lines use theme colors
- Check `cat ~/.config/gluck/config.toml` contains `[theme] name = "..."` matching last selected theme

- [ ] **Step 4: Commit any fixes**

Only if issues found during verification.
