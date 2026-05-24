# SemanticSearchModal 정리 설계

## 배경

`src/app.rs`가 1957줄까지 비대해지면서 모달 관련 코드가 여러 곳에 산재해 있다.
범용 모달 추상화 없이 `SemanticSearchModal` 하나만 존재하며, 입력 디스패치·렌더링·상태 머신이 ad-hoc으로 붙어 있다.
이번 설계는 **새 추상화 없이 기존 코드를 최소한으로 정리**하여 `app.rs`의 모달 관련 복잡도를 낮추는 데 목표를 둔다.

## 현황

### 문제점

1. **입력 디스패치 분산** — `handle_key`(app.rs:142)와 `handle_ctrl_key`(app.rs:203) 양쪽에서 `search_modal.is_open()` 체크 + 거의 동일한 패턴으로 분기
2. **Ctrl 키 중복** — Modal 열렸을 때 Ctrl+n/p는 `handle_ctrl_key`에서, 일반 Down/Up은 `handle_key`에서 별도 처리 → 키 바인딩이 두 군데에 분산
3. **상태 머신 과다** — 6개 상태(Idle, Typing, Loading, Results, NoIndex, Indexing). NoIndex/Indexing은 Loading 하나로 합칠 수 있음
4. **렌더링 ad-hoc** — `render()`에서 모드 match 후 별도 `if is_open()` 체크(app.rs:92-93)

### 대상 코드 위치

| 파일 | 줄 수 | 변경 범위 |
|------|--------|-----------|
| `src/search/modal.rs` | 147 | 상태 enum 단순화, `handle_key` 메서드 추가 |
| `src/app.rs` | 1957 | 입력 디스패치 통합, render 정리, handle_modal_key 제거 |
| `src/ui/search_modal.rs` | 165 | 닫힌 상태 렌더링 분기 정리 |

## 설계

### 1. 모달 상태 머신 단순화 (6 → 4)

```rust
pub enum ModalState {
    Closed,
    Typing { input: String },
    Loading { input: String, message: String },
    Results { input: String, results: Vec<SearchResult> },
}
```

**변경 사항**:

- `Idle` → `Closed` (의미 명확화)
- `NoIndex` 제거 → `Loading { message: "No index found. Build index?" }`로 표현
- `Indexing { message }` 제거 → `Loading { input: "", message }`로 표현
- `set_no_index()` → `set_loading(message)`로 통합
- `set_indexing(msg)` → `set_loading(msg)`로 통합

**`ModalState::input()` 헬퍼**:

```rust
pub fn input(&self) -> &str {
    match self {
        ModalState::Typing { input } => input,
        ModalState::Loading { input, .. } => input,
        ModalState::Results { input, .. } => input,
        ModalState::Closed => "",
    }
}
```

### 2. 입력 디스패치 통합 (handle_modal_key 제거)

`SemanticSearchModal`에 `handle_key(KeyCode) -> bool` 메서드 추가:

```rust
impl SemanticSearchModal {
    /// 키를 소비했으면 true, 무시했으면 false.
    /// Enter/Char('I')/Char('i')는 true 반환 후 호출부에서 후처리.
    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match &self.state {
            ModalState::Closed => return false,
            ModalState::Loading { .. } => {
                match code {
                    KeyCode::Esc => self.close(),
                    KeyCode::Char('I') | KeyCode::Char('i') => {} // 호출부에서 force_rebuild_index
                    _ => return false,
                }
            }
            _ => {}
        }
        match code {
            KeyCode::Esc => self.close(),
            KeyCode::Backspace => { self.pop_char(); }
            KeyCode::Down => self.move_down(),
            KeyCode::Up => self.move_up(),
            KeyCode::Enter => { /* 결과 선택 — 호출부에서 처리 */ }
            KeyCode::Char(c) => { self.push_char(c); }
            _ => return false,
        }
        true
    }
}
```

**app.rs 변경**:

```rust
// handle_key — 단일 진입점으로 통합
pub fn handle_key(&mut self, code: KeyCode) {
    // 1. 모달이 키를 소비했으면 중단
    if self.search_modal.handle_key(code) {
        // 후처리: Enter → 결과 선택, I/i → 인덱스 재빌드
        if code == KeyCode::Enter {
            self.select_search_result();
        } else if matches!(code, KeyCode::Char('I') | KeyCode::Char('i')) {
            self.force_rebuild_index(); // 내부에서 set_loading 호출
        }
        return;
    }
    // ... 나머지 기존 로직 (is_searching, Esc clear, Diff h/l, keybindings.resolve)
}

// handle_ctrl_key — 모달 체크 제거, 여기서는 Ctrl+c/d/n/p/t/f/b만 처리
pub fn handle_ctrl_key(&mut self, code: KeyCode) {
    // 모달 열렸을 때 Ctrl+c는 종료만, 그 외 Ctrl은 모달 닫힌 상태와 동일하게 동작
    match code {
        KeyCode::Char('c') => {
            if self.search_modal.is_open() {
                self.search_modal.close();
            } else {
                self.should_quit = true;
            }
        }
        KeyCode::Char('d') => self.debug_overlay = !self.debug_overlay,
        KeyCode::Char('n') => self.prev_commit(),
        KeyCode::Char('p') => self.next_commit(),
        KeyCode::Char('t') => self.next_theme(),
        KeyCode::Char('f') => self.pick_page_down(),
        KeyCode::Char('b') => self.pick_page_up(),
        _ => {}
    }
}
```

**키 폴스루**: 모달이 open 상태일 때 `handle_key`가 `false`를 반환한 키는 모달을 무시하고 다음으로 넘어간다. 예: 검색 중 Ctrl+n은 `handle_key`에서 무시 → `handle_ctrl_key`에서 커밋 이동 (검색 결과 선택과 별개로 커밋 이동 가능).

### 3. 렌더링 정리

`render()` 변경:

```rust
pub fn render(&self, frame: &mut Frame) {
    match &self.mode {
        Mode::Pick(_) => ui::pick::render_pick(frame, frame.area(), self),
        Mode::View(_) => ui::view::render_view(frame, frame.area(), self),
        Mode::Diff(_) => ui::diff::render_diff(frame, frame.area(), self),
    }

    ui::search_modal::render_search_modal(frame, self);

    if self.debug_overlay {
        self.render_debug_overlay(frame);
    }
}
```

**`render_search_modal` 변경**:

- `if !modal.is_open()` → early return (변경 없음)
- `ModalState::Closed` / `ModalState::Idle` / `ModalState::NoIndex` 분기 제거
- `ModalState::Loading { message }` → `message`를 포함한 하나의 렌더링 함수로 통합 (기존 `render_indexing` + `render_input`)
- `ModalState::Typing` / `ModalState::Results` → 기존과 동일

### 4. 인덱싱 파이프라인 (변경 없음)

- `index_rx`, `engine_rx` mpsc 채널은 `App`에 유지
- `drain_index_messages()` → `self.search_modal.set_loading(msg)`
- `drain_engine_messages()` → `self.search_modal.set_results(...)`

### 5. ModalAction enum 제거

`ModalAction` enum은 현재 어디서도 사용되지 않으므로 제거한다.

## 변경 요약

| 위치 | 변경 |
|------|------|
| `modal.rs` | `ModalState` 6→4 단순화, `ModalAction` 제거, `handle_key(KeyCode) -> bool` 추가, `set_no_index`/`set_indexing` → `set_loading` 통합 |
| `app.rs` | `handle_modal_key` 제거 (~40줄), `handle_key`/`handle_ctrl_key`에서 모달 체크 통합, `render()` 정리 |
| `ui/search_modal.rs` | `ModalState::NoIndex`/`Indexing` 분기 제거, `Loading`에 인덱싱 렌더링 통합 |

## 영향도

- **새 의존성 없음**
- **기존 기능 100% 보존** — 시맨틱 검색 동작 변경 없음
- **추후 확장 용이** — 다른 모달 추가 시 `handle_key` return false 패턴으로 쉽게 확장 가능
