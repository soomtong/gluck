# Type-Driven Refactoring Plan

## Context

gluck TUI git 로그 뷰어의 타입 안전성을 개선하여, 런타임에만 발견 가능한 불법 상태를 컴파일 타임에 차단한다. 핵심 목표: Optional/Boolean 조합으로 표현된 다중 상태를 Rust enum(Sum Type)으로 전환하여 불변식을 타입 시스템에 강제한다.

---

## 변경 순서 (의존성 기준)

### Step 1: Commits 캐싱 (E) — `app.rs`

`back()`, `switch_mode()`, `next_commit()`, `prev_commit()`에서 `list_commits()`를 반복 호출하는 문제를 해결.

**변경:**
- `App`에 `commits: Vec<CommitInfo>` 필드 추가
- `App::new()`에서 한 번만 로드
- `back()`, `switch_mode()`, `next_commit()`, `prev_commit()`에서 `self.commits` 사용
- `update_pick_diff()`, `compute_changed_paths()`는 이미 `state.commits`나 repo 직접 사용 → 변경 불필요

**수정 파일:** `src/app.rs`

### Step 2: 검색 상태를 PickState로 통합 (B) — `mode.rs`, `app.rs`, `ui/pick.rs`, `ui/layout.rs`

`App.searching: bool` + `App.search_input: String` → `PickState` 내부로 이동.

**새 타입 (`mode.rs`):**
```rust
pub enum SearchState {
    Idle { query: Option<String> },
    Active { input: String },
}
```

**PickState 변경:**
- `query: Option<String>` → `search: SearchState`
- `apply_search` 로직을 `SearchState` 메서드로 이동

**App 변경:**
- `searching: bool` + `search_input: String` 필드 제거
- `handle_key()`에서 `self.searching` → `matches!(self.mode, Mode::Pick(pick) if matches!(pick.search, SearchState::Active(_)))`
- `handle_search_input()`, `apply_search()`, `start_search()`가 `PickState`의 search 필드 직접 조작

**UI 변경:**
- `ui/pick.rs`: `app.searching` → PickState의 search 상태 확인
- `ui/layout.rs`: `render_search_bar` 호출 조건 동일하게 유지

**수정 파일:** `src/mode.rs`, `src/app.rs`, `src/ui/pick.rs`, `src/ui/layout.rs`

### Step 3: ViewState 파일 로딩 상태를 Sum Type으로 (A) — `mode.rs`, `app.rs`, `ui/view.rs`

`content: Option<String>` + `highlighted: Vec<Line>` → enum으로 통합.

**새 타입 (`mode.rs`):**
```rust
pub enum FileContent {
    NotLoaded,
    Binary,
    Text { raw: String, highlighted: Vec<Line<'static>> },
}
```

**ViewState 변경:**
- `content: Option<String>` + `highlighted: Vec<Line<'static>>` → `file_content: FileContent`

**App 변경:**
- `load_view_file()`: 3가지 분기에서 각 variant 생성
- `page_down()`, `page_up()`: line_count 계산 시 `FileContent` 매칭

**UI 변경:**
- `ui/view.rs`: 렌더링 시 `FileContent` 매칭으로 분기

**수정 파일:** `src/mode.rs`, `src/app.rs`, `src/ui/view.rs`

### Step 4: DiffLine을 Sum Type으로 (D) — `git/diff.rs`, `ui/diff.rs`

kind-종속적 line number를 variant에 직접 포함.

**새 타입 (`git/diff.rs`):**
```rust
pub enum DiffLine {
    Context { old_line_no: u32, new_line_no: u32, content: String },
    Added { line_no: u32, content: String },
    Removed { line_no: u32, content: String },
}
```

**git/diff.rs 변경:**
- `DiffLineKind` enum 제거
- `compute_diff()`에서 직접 variant 생성
- `DiffLineKind::Added/Removed/Context` 매칭 → `DiffLine::Added/Removed/Context` 매칭

**ui/diff.rs 변경:**
- `style_for_kind`, `render_unified`, `render_side_by_side`에서 새 `DiffLine` 매칭
- `file_stats`에서 필터 로직 조정

**수정 파일:** `src/git/diff.rs`, `src/ui/diff.rs`

### Step 5: DiffFile 변경 타입을 Sum Type으로 (C) — `git/diff.rs`, `ui/diff.rs`, `ui/pick.rs`, `app.rs`

`old_path: Option<String>` + `new_path: Option<String>` → 변경 종류 enum.

**새 타입 (`git/diff.rs`):**
```rust
pub enum FileChange {
    Added { path: String },
    Deleted { path: String },
    Modified { old_path: String, new_path: String },
}
```

**DiffFile 변경:**
- `old_path` + `new_path` → `change: FileChange`

**compute_diff() 변경:**
- delta 정보로부터 `FileChange` variant 생성

**UI 변경:**
- `ui/diff.rs`: 파일명 표시 시 `file.change`에서 추출
- `ui/pick.rs`: `file_stats`와 파일명 표시 동일하게
- `app.rs`: `next_commit/prev_commit`에서 path 비교 로직 조정

**수정 파일:** `src/git/diff.rs`, `src/ui/diff.rs`, `src/ui/pick.rs`, `src/app.rs`

### Step 6: 도메인 에러 타입 (F) — `git/repo.rs`, `git/commit.rs`, `git/tree.rs`, `git/diff.rs`

`anyhow::Result`를 도메인 에러로 교체.

**새 타입 (`git/repo.rs`):**
```rust
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Not a git repository: {0}")]
    RepositoryNotFound(String),
    #[error("Commit not found: {0}")]
    CommitNotFound(String),
    #[error("Tree walk failed: {0}")]
    TreeWalkFailed(String),
    #[error("Blob read failed: {0}")]
    BlobReadFailed(String),
    #[error("Diff computation failed: {0}")]
    DiffFailed(String),
}
```

**적용:**
- 각 함수의 `anyhow::Result<T>` → `Result<T, GitError>`
- `app.rs`의 `App::new()`는 `anyhow::Result` 유지 (최상위 진입점)
- `thiserror` 크레이트 추가 필요

**수정 파일:** `Cargo.toml`, `src/git/repo.rs`, `src/git/commit.rs`, `src/git/tree.rs`, `src/git/diff.rs`, `src/app.rs`

---

## 검증

각 Step 완료 후:
1. `cargo build` — 컴파일 확인
2. `cargo test` — 기존 테스트 통과 확인
3. `cargo run` — 수동 TUI 동작 확인 (Step 1~5 각각)
4. 최종: `cargo clippy` — 경고 없는지 확인
