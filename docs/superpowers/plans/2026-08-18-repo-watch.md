# Repo Watch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** glc TUI 실행 중 외부 커밋/ref 변경(새 커밋, branch 전환, rebase, pull)을 5초 주기 폴링으로 감지해 커밋 목록에 반영한다.

**Architecture:** 메인 루프를 `event::read()` 블로킹에서 `event::poll(1초)`로 전환하고, 5초마다 `repo.head()`의 (oid, ref 이름) 튜플을 스냅샷과 비교한다. Pick 모드에서는 즉시 `CommitStore`를 재빌드하고 oid 기반으로 선택 위치를 복원하며, View/Diff 모드에서는 footer에 알림만 표시하고 Pick 복귀 시점에 갱신을 적용한다. diff/tree 캐시는 oid 기반 불변이므로 건드리지 않는다.

**Tech Stack:** Rust, git2 (libgit2), crossterm, ratatui. 새 의존성 없음.

**Spec:** `docs/superpowers/specs/2026-08-18-repo-watch-design.md`

---

## File Structure

| 파일 | 변경 | 책임 |
|---|---|---|
| `src/git/repo.rs` | Modify | `GitRepo::head_info()` — HEAD (oid, shorthand) 스냅샷 조회 |
| `src/app.rs` | Modify | watch 상태 필드 3개 + `check_repo_changed()` / `apply_repo_refresh()` / `poll_repo_watch()` + `back()` 연동 |
| `src/main.rs` | Modify | 루프를 poll 기반으로 전환, `poll_repo_watch()` 호출 |
| `src/ui/view.rs` | Modify | footer에 "repo changed" 힌트 추가 |
| `src/ui/diff.rs` | Modify | footer에 "repo changed" 힌트 추가 |

테스트는 각 파일의 기존 `#[cfg(test)] mod tests`에 추가한다 (별도 테스트 파일 없음 — repo 컨벤션).

**주의:** repo에 포맷팅 부채가 있으므로 `cargo fmt`는 전체 실행 금지. 수정한 파일만 `rustfmt src/<file>.rs`.

---

### Task 1: `GitRepo::head_info()`

HEAD의 (oid, ref shorthand)를 반환하는 저렴한 스냅샷 메서드. unborn HEAD(커밋 없는 repo)나 `.git` 손상 시 `None`.

**Files:**
- Modify: `src/git/repo.rs` (impl GitRepo, 35행 부근)
- Test: `src/git/repo.rs`의 `pub mod tests`

- [ ] **Step 1: 실패하는 테스트 작성**

`src/git/repo.rs`의 `pub mod tests` 안, 기존 `test_create_n_commits` 아래에 추가:

```rust
    #[test]
    fn test_head_info_unborn_head_returns_none() {
        let (dir, _repo) = init_test_repo();
        let git_repo = GitRepo::open(dir.path()).unwrap();
        assert!(git_repo.head_info().is_none());
    }

    #[test]
    fn test_head_info_tracks_new_commits() {
        let (dir, repo) = init_test_repo();
        let first = add_file_commit(&repo, "a.txt", b"a", "first");
        let git_repo = GitRepo::open(dir.path()).unwrap();

        let (oid1, name1) = git_repo.head_info().unwrap();
        assert_eq!(oid1, first);
        assert!(!name1.is_empty());

        let second = add_file_commit(&repo, "b.txt", b"b", "second");
        let (oid2, _) = git_repo.head_info().unwrap();
        assert_eq!(oid2, second);
    }

    #[test]
    fn test_head_info_detects_branch_switch_same_oid() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"a", "first");
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head_commit, false).unwrap();

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let (oid1, name1) = git_repo.head_info().unwrap();

        repo.set_head("refs/heads/feature").unwrap();
        let (oid2, name2) = git_repo.head_info().unwrap();

        assert_eq!(oid1, oid2);
        assert_ne!(name1, name2);
    }
```

- [ ] **Step 2: 테스트가 실패(컴파일 에러)하는지 확인**

Run: `cargo test head_info`
Expected: FAIL — `no method named 'head_info' found`

- [ ] **Step 3: 최소 구현**

`src/git/repo.rs`의 `impl GitRepo`에서 `repository()` 아래에 추가:

```rust
    /// Cheap snapshot of HEAD for change polling.
    /// Returns None on unborn HEAD or repository errors.
    pub fn head_info(&self) -> Option<(git2::Oid, String)> {
        let head = self.repo.head().ok()?;
        let oid = head.target()?;
        let name = head.shorthand().unwrap_or("HEAD").to_string();
        Some((oid, name))
    }
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test head_info`
Expected: PASS (3 tests)

- [ ] **Step 5: 포맷 및 커밋**

```bash
rustfmt src/git/repo.rs
git add src/git/repo.rs
git commit -m "GitRepo에 head_info 스냅샷 메서드 추가"
```

---

### Task 2: App watch 상태 + `check_repo_changed()`

`App`에 HEAD 스냅샷/플래그/타이머 필드를 추가하고, 스냅샷 비교 메서드를 만든다.

**Files:**
- Modify: `src/app.rs` (struct App 31-54행, App::new 57-90행, impl 끝부분)
- Test: `src/app.rs`의 `mod tests` (1281행 이후)

- [ ] **Step 1: 테스트 헬퍼 추가**

`src/app.rs`의 `mod tests`에서 기존 `test_app()` 아래에 추가 (외부 커밋을 만들 수 있도록 `Repository` 핸들을 유지하는 변형):

```rust
    fn test_app_with_repo() -> (tempfile::TempDir, git2::Repository, App) {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First commit");
        add_file_commit(&repo, "b.txt", b"second", "Second commit");
        add_file_commit(&repo, "a.txt", b"third", "Third commit");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let app = App::new(git_repo, Config::default()).unwrap();
        (dir, repo, app)
    }
```

- [ ] **Step 2: 실패하는 테스트 작성**

같은 `mod tests`에 추가:

```rust
    #[test]
    fn test_check_repo_changed_noop_without_changes() {
        let (_dir, _repo, mut app) = test_app_with_repo();
        assert!(!app.check_repo_changed());
    }

    #[test]
    fn test_check_repo_changed_detects_external_commit() {
        let (_dir, repo, mut app) = test_app_with_repo();
        add_file_commit(&repo, "c.txt", b"new", "External commit");
        assert!(app.check_repo_changed());
        // Snapshot updated: second call is a no-op
        assert!(!app.check_repo_changed());
    }
```

- [ ] **Step 3: 테스트가 실패(컴파일 에러)하는지 확인**

Run: `cargo test check_repo_changed`
Expected: FAIL — `no method named 'check_repo_changed' found`

- [ ] **Step 4: 구현**

(a) `src/app.rs` 상단 import에 추가:

```rust
use std::time::{Duration, Instant};
```

(b) `struct App` 필드 끝(`engine_rx` 아래)에 추가:

```rust
    pub last_head: Option<(git2::Oid, String)>,
    pub repo_changed: bool,
    pub last_head_check: Instant,
```

(c) `App::new()`에서 `let store = ...` 위에 스냅샷을 먼저 뜨고:

```rust
        let last_head = repo.head_info();
```

구조체 초기화 리터럴 끝(`engine_rx: None,` 아래)에 추가:

```rust
            last_head,
            repo_changed: false,
            last_head_check: Instant::now(),
```

(d) `impl App` 안(예: `is_indexing` 위)에 상수와 메서드 추가:

```rust
    /// Compare current HEAD against the last snapshot. Updates the snapshot
    /// on change. Unreadable HEAD (unborn, deleted .git) is treated as no
    /// change so the app keeps running and retries next tick.
    pub fn check_repo_changed(&mut self) -> bool {
        let current = self.repo.head_info();
        if current.is_none() {
            return false;
        }
        if current != self.last_head {
            self.last_head = current;
            true
        } else {
            false
        }
    }
```

상수는 `struct App` 정의 위, 파일 상단 레벨에 추가:

```rust
pub const HEAD_POLL_INTERVAL: Duration = Duration::from_secs(5);
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test check_repo_changed`
Expected: PASS (2 tests)

주의: `Duration`/`HEAD_POLL_INTERVAL`이 아직 미사용이면 clippy 경고가 날 수 있다. Task 4에서 사용되므로 이 시점에는 `cargo test`만 통과하면 된다 (커밋은 Task 3과 묶어도 되지만, 여기서는 바로 커밋).

- [ ] **Step 6: 포맷 및 커밋**

```bash
rustfmt src/app.rs
git add src/app.rs
git commit -m "App에 HEAD 변경 감지 상태와 check_repo_changed 추가"
```

---

### Task 3: `apply_repo_refresh()` + `back()` 연동

store 재빌드 + Pick 상태 갱신(검색 필터 유지, oid 기반 선택 복원). View/Diff에서 Pick으로 돌아올 때 지연된 갱신을 적용.

**Files:**
- Modify: `src/app.rs` (`back()` 394행 부근, impl에 메서드 추가)
- Test: `src/app.rs`의 `mod tests`

- [ ] **Step 1: 실패하는 테스트 작성**

`mod tests`에 추가. `test_app_with_repo()`의 커밋 목록은 최신순으로 [Third, Second, First]:

```rust
    #[test]
    fn test_apply_repo_refresh_shows_new_commit_and_preserves_selection() {
        let (_dir, repo, mut app) = test_app_with_repo();
        // Select "Second commit" (index 1)
        app.handle_key(KeyCode::Char('j'));
        let selected_oid = {
            let Mode::Pick(state) = &app.mode else {
                panic!("expected pick mode")
            };
            state.commits[state.filtered_indices[state.selected]].id
        };

        add_file_commit(&repo, "c.txt", b"new", "External commit");
        assert!(app.check_repo_changed());
        app.apply_repo_refresh();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 4);
        // Same commit still selected, shifted down by the new commit
        assert_eq!(
            state.commits[state.filtered_indices[state.selected]].id,
            selected_oid
        );
        assert_eq!(state.selected, 2);
        assert!(!app.repo_changed);
    }

    #[test]
    fn test_apply_repo_refresh_clamps_selection_after_history_rewrite() {
        let (_dir, repo, mut app) = test_app_with_repo();
        // Selection stays on newest commit (index 0)
        let first_commit_oid = {
            let Mode::Pick(state) = &app.mode else {
                panic!("expected pick mode")
            };
            state.commits[2].id
        };
        // Hard-reset to the oldest commit: the selected (newest) oid disappears
        let obj = repo.find_object(first_commit_oid, None).unwrap();
        repo.reset(&obj, git2::ResetType::Hard, None).unwrap();

        assert!(app.check_repo_changed());
        app.apply_repo_refresh();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 1);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_apply_repo_refresh_keeps_active_filter() {
        let (_dir, repo, mut app) = test_app_with_repo();
        // Filter to "second" → 1 match
        app.handle_key(KeyCode::Char('/'));
        for c in "second".chars() {
            app.handle_key(KeyCode::Char(c));
        }
        app.handle_key(KeyCode::Enter);

        add_file_commit(&repo, "d.txt", b"x", "Second helping");
        assert!(app.check_repo_changed());
        app.apply_repo_refresh();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        // Filter still applied and re-run over the new list: both "Second*" match
        assert_eq!(state.filtered_indices.len(), 2);
        assert_eq!(state.commits.len(), 4);
    }

    #[test]
    fn test_back_applies_pending_refresh() {
        let (_dir, repo, mut app) = test_app_with_repo();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));

        add_file_commit(&repo, "c.txt", b"new", "External commit");
        assert!(app.check_repo_changed());
        app.repo_changed = true; // as poll_repo_watch sets it outside Pick

        app.handle_key(KeyCode::Esc);
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 4);
        assert!(!app.repo_changed);
    }
```

- [ ] **Step 2: 테스트가 실패(컴파일 에러)하는지 확인**

Run: `cargo test apply_repo_refresh`
Expected: FAIL — `no method named 'apply_repo_refresh' found`

- [ ] **Step 3: 구현**

(a) `impl App`에 메서드 추가 (`check_repo_changed` 아래):

```rust
    /// Rebuild the commit store from the current HEAD and, in Pick mode,
    /// re-apply the active filter and restore the selection by commit oid.
    /// Outside Pick mode only the store is rebuilt; callers refresh the
    /// visible state when transitioning back to Pick.
    pub fn apply_repo_refresh(&mut self) {
        let prev_total = self.store.total_loaded();
        let Ok(mut new_store) = CommitStore::new(&self.repo, 200) else {
            return;
        };
        // Keep the previous scroll depth available after the rebuild.
        while new_store.total_loaded() < prev_total && !new_store.exhausted {
            if new_store.load_batch(&self.repo).is_err() {
                break;
            }
        }
        self.store = new_store;
        self.repo_changed = false;

        if let Mode::Pick(state) = &mut self.mode {
            let prev_oid = state
                .filtered_indices
                .get(state.selected)
                .map(|&i| state.commits[i].id);
            let prev_selected = state.selected;
            state.commits = self.store.loaded.clone();
            let query = state.query().map(|s| s.to_string());
            match query {
                Some(q) => state.update_filter(&q),
                None => {
                    state.filtered_indices = (0..state.commits.len()).collect();
                    state.scroll = 0;
                }
            }
            state.selected = prev_oid
                .and_then(|oid| state.commits.iter().position(|c| c.id == oid))
                .and_then(|full_idx| {
                    state.filtered_indices.iter().position(|&i| i == full_idx)
                })
                .unwrap_or_else(|| {
                    prev_selected.min(state.filtered_indices.len().saturating_sub(1))
                });
        }
        if matches!(self.mode, Mode::Pick(_)) {
            self.update_pick_diff();
        }
    }
```

(b) `back()` (394행 부근) 맨 앞에 지연 갱신 적용:

```rust
    fn back(&mut self) {
        if self.repo_changed {
            // Deferred refresh from View/Diff: rebuild the store first so the
            // PickState below is built from the fresh commit list.
            self.apply_repo_refresh();
        }
        match &self.mode {
```

(참고: 이 시점의 mode는 View/Diff이므로 `apply_repo_refresh()`는 store 재빌드 + 플래그 해제만 수행하고, 선택 복원은 기존 `back()` 로직이 commit id로 처리한다.)

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test apply_repo_refresh && cargo test test_back_applies_pending_refresh`
Expected: PASS (4 tests)

- [ ] **Step 5: 기존 테스트 회귀 확인**

Run: `cargo test`
Expected: 전체 PASS

- [ ] **Step 6: 포맷 및 커밋**

```bash
rustfmt src/app.rs
git add src/app.rs
git commit -m "Repo 변경 시 store 재빌드와 선택 복원 로직 추가"
```

---

### Task 4: `poll_repo_watch()` + 메인 루프 poll 전환

5초 타이머 판단 + 모드별 분기(Pick 즉시 갱신 / 그 외 플래그)를 App 메서드로 캡슐화하고, `main.rs` 루프를 블로킹에서 poll 기반으로 바꾼다.

**Files:**
- Modify: `src/app.rs` (impl에 메서드 추가)
- Modify: `src/main.rs:101-126` (`run_app`)
- Test: `src/app.rs`의 `mod tests`

- [ ] **Step 1: 실패하는 테스트 작성**

`mod tests`에 추가. 타이머는 `last_head_check`를 과거로 되돌려 만료시킨다:

```rust
    #[test]
    fn test_poll_repo_watch_refreshes_immediately_in_pick() {
        let (_dir, repo, mut app) = test_app_with_repo();
        add_file_commit(&repo, "c.txt", b"new", "External commit");
        app.last_head_check = std::time::Instant::now() - HEAD_POLL_INTERVAL;

        app.poll_repo_watch();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 4);
        assert!(!app.repo_changed);
    }

    #[test]
    fn test_poll_repo_watch_defers_refresh_outside_pick() {
        let (_dir, repo, mut app) = test_app_with_repo();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));

        add_file_commit(&repo, "c.txt", b"new", "External commit");
        app.last_head_check = std::time::Instant::now() - HEAD_POLL_INTERVAL;

        app.poll_repo_watch();

        // Viewed content untouched; only the flag is raised
        assert!(app.repo_changed);
        assert!(matches!(app.mode, Mode::View(_)));
    }

    #[test]
    fn test_poll_repo_watch_respects_interval() {
        let (_dir, repo, mut app) = test_app_with_repo();
        add_file_commit(&repo, "c.txt", b"new", "External commit");
        // last_head_check was just set in App::new → within the interval
        app.poll_repo_watch();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 3);
        assert!(!app.repo_changed);
    }
```

- [ ] **Step 2: 테스트가 실패(컴파일 에러)하는지 확인**

Run: `cargo test poll_repo_watch`
Expected: FAIL — `no method named 'poll_repo_watch' found`

- [ ] **Step 3: App 메서드 구현**

`impl App`의 `apply_repo_refresh` 아래에 추가:

```rust
    /// Called every main-loop iteration. Checks HEAD at most once per
    /// HEAD_POLL_INTERVAL. In Pick mode a detected change refreshes the
    /// commit list immediately; in View/Diff it only raises `repo_changed`
    /// so the footer can show a notice without disturbing the viewed content.
    pub fn poll_repo_watch(&mut self) {
        if self.last_head_check.elapsed() < HEAD_POLL_INTERVAL {
            return;
        }
        self.last_head_check = Instant::now();
        if self.check_repo_changed() {
            if matches!(self.mode, Mode::Pick(_)) {
                self.apply_repo_refresh();
            } else {
                self.repo_changed = true;
            }
        }
    }
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test poll_repo_watch`
Expected: PASS (3 tests)

- [ ] **Step 5: 메인 루프 전환**

`src/main.rs`의 `run_app`(101-126행)을 다음으로 교체. 변경점: (1) draw 직후 `poll_repo_watch()` 호출, (2) 비인덱싱 분기가 `event::read()` 블로킹 대신 `event::poll(1초)` 사용:

```rust
fn run_app(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        if app.needs_clear {
            terminal.clear()?;
            app.needs_clear = false;
        }
        terminal.draw(|f| app.render(f))?;

        app.poll_repo_watch();

        if app.is_indexing() {
            app.drain_index_messages();
            app.drain_engine_messages();
            app.drain_search_results();
            if event::poll(Duration::from_millis(80))? {
                read_and_dispatch(app)?;
            }
        } else {
            app.drain_search_results();
            if event::poll(Duration::from_secs(1))? {
                read_and_dispatch(app)?;
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
```

- [ ] **Step 6: 전체 테스트 + clippy**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: 전체 PASS, clippy 경고 없음

- [ ] **Step 7: 포맷 및 커밋**

```bash
rustfmt src/app.rs src/main.rs
git add src/app.rs src/main.rs
git commit -m "메인 루프를 poll 기반으로 전환하고 5초 주기 HEAD 감시 연결"
```

---

### Task 5: View/Diff footer 알림

`repo_changed`가 서있으면 footer 힌트 끝에 "repo changed"를 표시한다. Pick은 자동 갱신되므로 표시 불필요.

**Files:**
- Modify: `src/ui/view.rs:136-146`
- Modify: `src/ui/diff.rs:73-82`

UI 렌더 테스트 인프라(TestBackend)가 repo에 없으므로 이 태스크는 컴파일 + 수동 확인으로 검증한다 (Task 6에서 수동 스모크 테스트).

- [ ] **Step 1: view.rs footer 수정**

`src/ui/view.rs` 136-146행의 `let hints = [...]` 배열을 Vec으로 바꾸고 조건부 힌트 추가:

```rust
    let mut hints = vec![
        ("[j/k]", "move"),
        ("[u/d]", "scroll"),
        ("[J/K]", "page"),
        ("[^P/^N]", "commit"),
        ("[.]", "ign"),
        ("[Enter]", "open"),
        ("[Tab]", "diff"),
        ("[Esc]", "back"),
    ];
    if app.repo_changed {
        hints.push(("[!]", "repo changed"));
    }
    layout::render_footer(frame, footer, &app.palette, &hints);
```

- [ ] **Step 2: diff.rs footer 수정**

`src/ui/diff.rs` 73-82행도 동일하게:

```rust
    let mut hints = vec![
        ("[j/k/←/→]", "file"),
        ("[u/d]", "scroll"),
        ("[J/K]", "page"),
        ("[^P/^N]", "commit"),
        ("[s]", "view"),
        ("[Tab]", "back"),
        ("[Esc]", "pick"),
    ];
    if app.repo_changed {
        hints.push(("[!]", "repo changed"));
    }
    layout::render_footer(frame, footer, &app.palette, &hints);
```

- [ ] **Step 3: 빌드 + clippy 확인**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS. (`render_footer`는 `&[(&str, &str)]`를 받으므로 `&Vec`이 그대로 deref된다.)

- [ ] **Step 4: 포맷 및 커밋**

```bash
rustfmt src/ui/view.rs src/ui/diff.rs
git add src/ui/view.rs src/ui/diff.rs
git commit -m "View/Diff footer에 repo changed 알림 표시"
```

---

### Task 6: 수동 스모크 테스트 + 문서 갱신

- [ ] **Step 1: 수동 스모크 테스트**

터미널 A에서:

```bash
cargo run --bin glc -- .
```

터미널 B에서 (같은 repo):

```bash
git commit --allow-empty -m "smoke: watch test"
```

확인 사항:
1. Pick 모드: 최대 5초(+poll 1초) 안에 새 커밋이 목록 맨 위에 나타나고, 보고 있던 커밋에 커서가 유지된다.
2. Enter로 View 진입 후 터미널 B에서 커밋 하나 더 → footer에 `[!] repo changed` 표시, 보던 내용 유지.
3. Esc로 Pick 복귀 → 목록 갱신, 알림 소멸.
4. idle 시 CPU 사용률이 눈에 띄게 오르지 않는지 (`top` 등으로 확인).

정리:

```bash
git reset --hard HEAD~2   # 스모크 커밋 2개 제거
```

(주의: 스모크 커밋 이후 다른 커밋이 없는지 `git log --oneline -3`으로 확인 후 실행.)

- [ ] **Step 2: CLAUDE.md 갱신**

`CLAUDE.md`의 "Architecture gotchas" 섹션에 추가:

```markdown
### Repo watch

- Main loop is poll-based (`event::poll(1s)`; 80ms while indexing). `App::poll_repo_watch()` checks `GitRepo::head_info()` (oid + ref shorthand) every `HEAD_POLL_INTERVAL` (5s).
- On change: Pick mode rebuilds `CommitStore` immediately (filter re-applied, selection restored by oid); View/Diff only set `repo_changed` and show a footer notice — the refresh applies in `back()`.
- Diff/tree caches are oid-keyed and immutable — never cleared on refresh.
```

- [ ] **Step 3: 커밋**

```bash
git add CLAUDE.md
git commit -m "CLAUDE.md에 repo watch 동작 문서화"
```
