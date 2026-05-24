# SemanticSearchModal Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Simplify the semantic search modal by consolidating its state machine, unifying input dispatch, and cleaning up rendering.

**Architecture:** Six-state ModalState becomes four. `SemanticSearchModal` gains `handle_key(KeyCode) -> bool` so `App` checks it once. `handle_modal_key` is removed. `set_no_index`/`set_indexing` merge into `set_loading`. `ui/search_modal.rs` renders one Loading state instead of three separate branches.

**Tech Stack:** Rust, ratatui, existing gluck codebase

---

### Task 1: Simplify `ModalState` enum and clean up `SemanticSearchModal`

**Files:**
- Modify: `src/search/modal.rs`

- [ ] **Step 1: Rewrite `ModalState` enum — replace 6 variants with 4**

Replace lines 3-20 (the enum):

```rust
#[derive(Debug, Clone)]
pub enum ModalState {
    Closed,
    Typing {
        input: String,
    },
    Loading {
        input: String,
        message: String,
    },
    Results {
        input: String,
        results: Vec<SearchResult>,
    },
}
```

- [ ] **Step 2: Update `ModalState::input()` helper**

Replace lines 22-31:

```rust
impl ModalState {
    pub fn input(&self) -> &str {
        match self {
            ModalState::Typing { input }
            | ModalState::Loading { input, .. }
            | ModalState::Results { input, .. } => input,
            ModalState::Closed => "",
        }
    }
}
```

- [ ] **Step 3: Remove `ModalAction` enum**

Delete lines 33-38 entirely.

- [ ] **Step 4: Update `SemanticSearchModal::new()` — use `Closed` instead of `Idle`**

Replace lines 46-51:

```rust
impl SemanticSearchModal {
    pub fn new() -> Self {
        Self {
            state: ModalState::Closed,
            selected: 0,
        }
    }
```

- [ ] **Step 5: Update `close()` — use `Closed` instead of `Idle`**

Replace lines 60-63:

```rust
    pub fn close(&mut self) {
        self.state = ModalState::Closed;
        self.selected = 0;
    }
```

- [ ] **Step 6: Update `push_char()` — remove unused state patterns**

Replace lines 65-69:

```rust
    pub fn push_char(&mut self, c: char) {
        if let ModalState::Typing { input } | ModalState::Results { input, .. } = &mut self.state {
            input.push(c);
        }
    }
```

Note: `Loading` is intentionally excluded — typing is not allowed while loading.

- [ ] **Step 7: Update `pop_char()` — same pattern as push_char**

Replace lines 71-75:

```rust
    pub fn pop_char(&mut self) {
        if let ModalState::Typing { input } | ModalState::Results { input, .. } = &mut self.state {
            input.pop();
        }
    }
```

- [ ] **Step 8: Replace `set_no_index()` and `set_indexing()` with `set_loading()`**

Delete lines 83-91 and add instead:

```rust
    pub fn set_loading(&mut self, message: impl Into<String>) {
        let input = self.state.input().to_string();
        self.state = ModalState::Loading {
            input,
            message: message.into(),
        };
    }
```

- [ ] **Step 9: Update `set_results()` — preserves input across state change**

No code change needed here — it already reads `self.state.input()`, which now also works for `Loading`. (Verify: line 77-80 is unchanged.)

- [ ] **Step 10: Update `is_open()` — check for `Closed` instead of `Idle`**

Replace lines 104-106:

```rust
    pub fn is_open(&self) -> bool {
        !matches!(self.state, ModalState::Closed)
    }
```

- [ ] **Step 11: Update unit tests — replace `ModalState::Idle` with `ModalState::Closed`**

Replace lines 123-146:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modal_open_close() {
        let mut m = SemanticSearchModal::new();
        assert!(matches!(m.state, ModalState::Closed));
        assert!(!m.is_open());
        m.open();
        assert!(matches!(m.state, ModalState::Typing { .. }));
        assert!(m.is_open());
        m.close();
        assert!(matches!(m.state, ModalState::Closed));
        assert!(!m.is_open());
    }

    #[test]
    fn test_push_pop_char() {
        let mut m = SemanticSearchModal::new();
        m.open();
        m.push_char('a');
        m.push_char('b');
        assert_eq!(m.state.input(), "ab");
        m.pop_char();
        assert_eq!(m.state.input(), "a");
    }

    #[test]
    fn test_set_loading_preserves_input() {
        let mut m = SemanticSearchModal::new();
        m.open();
        m.push_char('t');
        m.push_char('e');
        m.push_char('s');
        m.push_char('t');
        m.set_loading("Indexing...");
        assert!(matches!(m.state, ModalState::Loading { .. }));
        assert_eq!(m.state.input(), "test");
    }
}
```

- [ ] **Step 12: Build and test modal module only**

```bash
cargo test -p glc search::modal
```
Expected: 3 tests pass.

- [ ] **Step 13: Commit**

```bash
git add src/search/modal.rs
git commit -m "Simplify ModalState from 6 to 4 variants, remove unused ModalAction"
```

---

### Task 2: Add `handle_key` method to `SemanticSearchModal`

**Files:**
- Modify: `src/search/modal.rs`

- [ ] **Step 1: Add `handle_key` method after `pop_char` (before `set_results`)**

Insert after `pop_char` (after the closing `}` of `pop_char`):

```rust
    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match &self.state {
            ModalState::Closed => return false,
            ModalState::Loading { .. } => {
                match code {
                    KeyCode::Esc => self.close(),
                    KeyCode::Char('I') | KeyCode::Char('i') => {}
                    _ => return false,
                }
            }
            _ => {}
        }
        match code {
            KeyCode::Esc => self.close(),
            KeyCode::Backspace => {
                self.pop_char();
            }
            KeyCode::Down => {
                self.move_down();
            }
            KeyCode::Up => {
                self.move_up();
            }
            KeyCode::Enter => {}
            KeyCode::Char(c) => {
                self.push_char(c);
            }
            _ => return false,
        }
        true
    }
```

- [ ] **Step 2: Add `KeyCode` import at top of file**

Insert after line 1 (`use crate::search::SearchResult;`):

```rust
use crossterm::event::KeyCode;
```

- [ ] **Step 3: Add unit tests for `handle_key`**

Append to the test module (after `test_set_loading_preserves_input`):

```rust
    #[test]
    fn test_handle_key_closed_returns_false() {
        let mut m = SemanticSearchModal::new();
        assert!(!m.handle_key(KeyCode::Esc));
        assert!(!m.handle_key(KeyCode::Char('a')));
        assert!(!m.handle_key(KeyCode::Enter));
        assert!(!m.handle_key(KeyCode::Down));
    }

    #[test]
    fn test_handle_key_typing() {
        let mut m = SemanticSearchModal::new();
        m.open();
        assert!(m.handle_key(KeyCode::Char('h')));
        assert!(m.handle_key(KeyCode::Char('i')));
        assert_eq!(m.state.input(), "hi");
        assert!(m.handle_key(KeyCode::Backspace));
        assert_eq!(m.state.input(), "h");
        assert!(m.handle_key(KeyCode::Esc));
        assert!(matches!(m.state, ModalState::Closed));
    }

    #[test]
    fn test_handle_key_loading_only_esc_and_i() {
        let mut m = SemanticSearchModal::new();
        m.set_loading("Loading...");
        assert!(m.handle_key(KeyCode::Esc));
        assert!(matches!(m.state, ModalState::Closed));

        m.set_loading("Loading...");
        assert!(m.handle_key(KeyCode::Char('I')));
        assert!(m.handle_key(KeyCode::Char('i')));
        assert!(!m.handle_key(KeyCode::Char('x')));
        assert!(!m.handle_key(KeyCode::Down));
    }
```

- [ ] **Step 4: Run tests — all 6 should pass**

```bash
cargo test -p glc search::modal
```
Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/search/modal.rs
git commit -m "Add handle_key(KeyCode) -> bool to SemanticSearchModal"
```

---

### Task 3: Update all `set_no_index`/`set_indexing` callers in `app.rs`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Replace `set_no_index()` call in `open_semantic_search()`**

At line 1066, replace:
```rust
                self.search_modal.set_no_index();
```
with:
```rust
                self.search_modal.set_loading("No index found. Press I to build, Esc to close.");
```

- [ ] **Step 2: Replace all `set_indexing(...)` calls**

Replace all 9 occurrences in app.rs. Here is each one:

Line 880:
```rust
                self.search_modal.set_indexing("Indexing...");
```
→
```rust
                self.search_modal.set_loading("Indexing...");
```

Line 892:
```rust
        self.search_modal.set_indexing("Starting indexer...");
```
→
```rust
        self.search_modal.set_loading("Starting indexer...");
```

Line 932:
```rust
                Ok(IndexMessage::Progress(msg)) => self.search_modal.set_indexing(msg),
```
→
```rust
                Ok(IndexMessage::Progress(msg)) => self.search_modal.set_loading(msg),
```

Line 953-955:
```rust
                self.search_modal
                    .set_indexing(format!("Index build failed: {} (Esc)", e));
```
→
```rust
                self.search_modal
                    .set_loading(format!("Index build failed: {} (Esc)", e));
```

Line 973:
```rust
                        self.search_modal.set_indexing(msg);
```
→
```rust
                        self.search_modal.set_loading(msg);
```

Line 998-1000:
```rust
                    self.search_modal.set_indexing(format!(
                        "Model loading... {}/{}",
                        progress.current, progress.total
                    ));
```
→
```rust
                    self.search_modal.set_loading(format!(
                        "Model loading... {}/{}",
                        progress.current, progress.total
                    ));
```

Line 1071:
```rust
                    .set_indexing("Index schema outdated — rebuilding...");
```
→
```rust
                    .set_loading("Index schema outdated — rebuilding...");
```

Line 1086:
```rust
            self.search_modal.set_indexing(msg);
```
→
```rust
            self.search_modal.set_loading(msg);
```

Line 1089:
```rust
        self.search_modal.set_indexing("Loading embedding model...");
```
→
```rust
        self.search_modal.set_loading("Loading embedding model...");
```

- [ ] **Step 3: Build to verify no compilation errors**

```bash
cargo build 2>&1
```
Expected: compiles without errors. `set_no_index` and `set_indexing` are gone from `modal.rs`, so any missed call site will be a compile error.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "Replace set_no_index/set_indexing with set_loading in app.rs"
```

---

### Task 4: Unify input dispatch — remove `handle_modal_key`, clean up `handle_ctrl_key`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Rewrite modal check in `handle_key` (lines 142-146)**

Replace:
```rust
        if self.search_modal.is_open() {
            self.handle_modal_key(code);
            return;
        }
```
with:
```rust
        if self.search_modal.handle_key(code) {
            if code == KeyCode::Enter {
                self.select_search_result();
            } else if matches!(code, KeyCode::Char('I') | KeyCode::Char('i')) {
                self.force_rebuild_index();
            }
            return;
        }
```

- [ ] **Step 2: Remove modal check from `handle_ctrl_key` (lines 204-212)**

Replace:
```rust
        if self.search_modal.is_open() {
            match code {
                KeyCode::Char('c') => self.should_quit = true,
                KeyCode::Char('n') => self.search_modal.move_down(),
                KeyCode::Char('p') => self.search_modal.move_up(),
                _ => {}
            }
            return;
        }
```
with Ctrl+c handling that closes modal instead of quitting:
```rust
        if code == KeyCode::Char('c') && self.search_modal.is_open() {
            self.search_modal.close();
            return;
        }
```

- [ ] **Step 3: Delete `handle_modal_key` method (lines 1093-1134)**

Delete the entire `handle_modal_key` function (42 lines, from `fn handle_modal_key` through the closing `}`).

- [ ] **Step 4: Update test that references `ModalState::Indexing`**

At line 1818, the test checks `ModalState::Indexing { .. }` but the state is now `Loading`. Replace:
```rust
            ModalState::Indexing { .. }
```
with:
```rust
            ModalState::Loading { .. }
```

The test at line 1808 calls `app.handle_key(KeyCode::Char('I'))` which resolves to `Action::ForceIndex` via keybinding (mode.rs:208), calling `force_rebuild_index()` which opens the modal in Loading state. This flow is unchanged — the test only needs `ModalState::Indexing` → `ModalState::Loading` in its assertion.

- [ ] **Step 5: Build to verify no compilation errors**

```bash
cargo build 2>&1
```
Expected: compiles. `handle_modal_key` is removed, any remaining references are compile errors.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "Unify modal input dispatch, remove handle_modal_key"
```

---

### Task 5: Update `render_search_modal` — merge NoIndex/Indexing into Loading

**Files:**
- Modify: `src/ui/search_modal.rs`

- [ ] **Step 1: Rewrite match on ModalState (lines 20-30)**

Replace:
```rust
    match &modal.state {
        ModalState::Idle => {}
        ModalState::NoIndex => render_no_index(frame, area, app),
        ModalState::Indexing { message } => render_indexing(frame, area, message, app),
        ModalState::Typing { input } | ModalState::Loading { input } => {
            render_input(frame, area, input.as_str(), app)
        }
        ModalState::Results { input, results } => {
            render_results(frame, area, input.as_str(), results, modal.selected, app)
        }
    }
```
with:
```rust
    match &modal.state {
        ModalState::Closed => {}
        ModalState::Loading { input, message } => {
            if input.is_empty() {
                render_loading(frame, area, message.as_str(), app);
            } else {
                render_input(frame, area, input.as_str(), app);
            }
        }
        ModalState::Typing { input } => {
            render_input(frame, area, input.as_str(), app);
        }
        ModalState::Results { input, results } => {
            render_results(frame, area, input.as_str(), results, modal.selected, app);
        }
    }
```

- [ ] **Step 2: Rename `render_indexing` to `render_loading`**

Replace the function signature at line 33:
```rust
fn render_indexing(frame: &mut Frame, area: Rect, message: &str, app: &App) {
```
with:
```rust
fn render_loading(frame: &mut Frame, area: Rect, message: &str, app: &App) {
```

- [ ] **Step 3: Remove `render_no_index` function (lines 56-72)**

Delete the entire `render_no_index` function (17 lines). It is no longer used.

- [ ] **Step 4: Update imports**

Remove any unused imports from lines 1-9. Check after build.

- [ ] **Step 5: Build to verify**

```bash
cargo build 2>&1
```
Expected: compiles. No unused imports or dead code warnings for `render_no_index`.

- [ ] **Step 6: Commit**

```bash
git add src/ui/search_modal.rs
git commit -m "Merge NoIndex/Indexing into Loading state in search modal UI"
```

---

### Task 6: Final build, test, format, clippy

- [ ] **Step 1: Run full test suite**

```bash
cargo test
```
Expected: all tests pass.

- [ ] **Step 2: Format changed files**

```bash
rustfmt src/search/modal.rs src/app.rs src/ui/search_modal.rs
```

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -- -D warnings
```
Expected: no warnings.

- [ ] **Step 4: Commit final cleanup**

```bash
git add src/search/modal.rs src/app.rs src/ui/search_modal.rs
git commit -m "Final format and clippy pass after modal cleanup"
```<｜end▁of▁thinking｜>

<｜｜DSML｜｜tool_calls>
<｜｜DSML｜｜invoke name="read">
<｜｜DSML｜｜parameter name="filePath" string="true">/Users/dp/Repository/gluck/src/app.rs