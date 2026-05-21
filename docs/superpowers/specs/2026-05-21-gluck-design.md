# gluck - Terminal Git History File Viewer

## 개요

터미널에서 git history의 파일을 탐색하고 읽는 TUI 도구. git history를 타임라인으로 삼아 각 시점의 파일을 읽고 비교하는 데 집중한다.

## 기술 스택

- **언어**: Rust
- **TUI 프레임워크**: ratatui (crossterm backend)
- **Git 연동**: git2-rs (libgit2 기반 네이티브)
- **Syntax highlighting**: tree-sitter
- **로깅/디버깅**: tracing + tracing-subscriber

## 아키텍처: Mode-State Machine

단일 `App` 구조체가 현재 모드(Pick/View/Diff)를 상태로 관리하고, 각 모드가 자체 렌더링/이벤트 핸들러를 가지는 패턴.

```
App { mode: Mode }
├── Mode::Pick(PickState) → 커밋 리스트, 검색
├── Mode::View(ViewState) → 파일 트리 + 파일 내용
└── Mode::Diff(DiffState) → 두 커밋 비교
```

## 프로젝트 구조

```
gluck/
├── Cargo.toml
├── src/
│   ├── main.rs              # 진입점, CLI 파싱
│   ├── app.rs               # App 상태 관리, 모드 전이
│   ├── mode.rs              # Mode enum, KeyBindings, 모드 전이 로직
│   ├── git/                 # Git 데이터 레이어
│   │   ├── mod.rs
│   │   ├── repo.rs          # Repository 래퍼 (git2-rs)
│   │   ├── commit.rs        # 커밋 조회, 검색
│   │   ├── tree.rs          # 파일 트리 탐색
│   │   └── diff.rs          # Diff 계산
│   ├── ui/                  # ratatui UI 레이어
│   │   ├── mod.rs
│   │   ├── layout.rs        # 공통 레이아웃 구조
│   │   ├── pick.rs          # Pick 모드 UI
│   │   ├── view.rs          # View 모드 UI
│   │   └── diff.rs          # Diff 모드 UI
│   ├── highlight/           # Syntax highlighting
│   │   ├── mod.rs
│   │   └── engine.rs        # tree-sitter 래퍼
│   └── debug.rs             # 디버깅/성능 측정 유틸리티
```

## 모드 전이

```
Pick ──[Enter]──→ View ──[Tab]──→ Diff ──[Esc]──→ Pick
  ↑                  │                   │
  └────[Esc]─────────┘                   │
  └────────────────[Esc]─────────────────┘
```

## 키 바인딩

멀티키 구조: 각 액션에 여러 키를 바인딩 가능. 현재는 하드코딩된 기본값 사용, 추후 설정 파일에서 커스텀 매핑 지원 예정.

```rust
struct KeyBindings {
    move_down: Vec<Key>,    // ['j', Down]
    move_up: Vec<Key>,      // ['k', Up]
    enter: Vec<Key>,        // [Enter, 'l']
    back: Vec<Key>,         // [Esc, 'h']
    search: Vec<Key>,       // ['/']
    quit: Vec<Key>,         // ['q', Ctrl+C]
    toggle_view: Vec<Key>,  // ['s']
    switch_mode: Vec<Key>,  // [Tab]
}
```

| 액션 | 기본 키 1 | 기본 키 2 |
|---|---|---|
| 위로 이동 | `k` | `Up` |
| 아래로 이동 | `j` | `Down` |
| 선택/진입 | `Enter` | `l` |
| 뒤로가기 | `Esc` | `h` |
| 검색 | `/` | - |
| 종료 | `q` | `Ctrl+C` |
| 뷰 토글 | `s` | - |
| 모드 전환 | `Tab` | - |

## 모드 상세 설계

### Pick 모드

```
┌─────────────────────────────────────────┐
│ gluck - Pick Mode                       │
│ [/] Search: ___________________________ │
├─────────────────────────────────────────┤
│ ● abc1234  2024-01-15  Add auth module │
│   def5678  2024-01-14  Fix login bug   │
│   ghi9012  2024-01-13  Init project    │
│                                         │
├─────────────────────────────────────────┤
│ [j/k] move  [Enter] view  [/] search   │
└─────────────────────────────────────────┘
```

- git2-rs RevisionWalker로 커밋 역순 조회
- 검색: 커밋 메시지, 작성자, 해시 기반 필터링
- 스크롤은 커서 기반 (현재 선택 커밋 중심)

### View 모드

```
┌──────────────┬──────────────────────────┐
│ File Tree    │ Content                  │
│              │                          │
│ > src/       │  1  fn main() {          │
│   main.rs    │  2      println!("hi");  │
│   lib.rs     │  3  }                    │
│ > docs/      │                          │
├──────────────┴──────────────────────────┤
│ [j/k] move  [Enter] open  [Tab] diff   │
└─────────────────────────────────────────┘
```

- 좌측: 선택한 커밋의 파일 트리 (git tree 객체 탐색)
- 우측: 선택한 파일 내용 (tree-sitter syntax highlight 적용)
- 파일 확장자 기반 tree-sitter 언어 자동 감지

### Diff 모드

```
┌─────────────────────────────────────────┐
│ gluck - Diff: abc1234 vs def5678        │
├───────────────────┬─────────────────────┤
│ - old code        │ + new code          │
│   unchanged       │   unchanged         │
│ - removed line    │ + added line        │
├───────────────────┴─────────────────────┤
│ [s] toggle view  [j/k] move  [Tab] back│
└─────────────────────────────────────────┘
```

- 기본 side-by-side, `s`로 unified 토글
- git2-rs 네이티브 Diff 기능 사용
- 변경된 파일만 필터링하여 파일 목록 표시

## 핵심 타입

```rust
// git/repo.rs
struct GitRepo {
    repo: Repository,
}

// git/commit.rs
struct CommitInfo {
    id: Oid,
    short_id: String,
    author: String,
    date: SystemTime,
    message: String,
}

// git/tree.rs
struct FileEntry {
    name: String,
    path: String,
    kind: EntryKind,  // File | Directory
}

// mode.rs
enum Mode {
    Pick(PickState),
    View(ViewState),
    Diff(DiffState),
}

struct PickState {
    commits: Vec<CommitInfo>,
    selected: usize,
    scroll: usize,
    query: Option<String>,
}

struct ViewState {
    commit: CommitInfo,
    tree: Vec<FileEntry>,
    selected_file: usize,
    content: Option<String>,
    highlighted: Vec<Line>,
}

struct DiffState {
    from: CommitInfo,
    to: CommitInfo,
    files: Vec<String>,
    selected_file: usize,
    side_by_side: bool,
}
```

## 데이터 흐름

```
[사용자 입력] → App::handle_event()
                    │
                    ├── Pick: GitRepo::commits() → PickState 업데이트
                    ├── View: GitRepo::tree() + GitRepo::blob()
                    │              → tree-sitter 하이라이팅 → ViewState
                    └── Diff: GitRepo::diff() → DiffState
                                    │
                    App::render() ← 현재 Mode ← ratatui 프레임
```

- GitRepo는 모든 git 작업의 단일 진입점
- tree-sitter 하이라이팅은 View 모드에서 파일 로딩 시 수행
- Diff는 git2-rs의 네이티브 Diff 기능 활용

## 디버깅 및 성능 측정

### 로깅 (tracing)

- `tracing` 크레이트로 구조화된 로깅
- CLI 플래그로 로그 레벨 제어: `gluck --log-level debug` 또는 `RUST_LOG=gluck=debug gluck`
- TUI와 충돌하지 않도록 로그는 파일로 출력 (`gluck.log`)
- 주요 로깅 포인트:
  - 모드 전이 (Pick → View → Diff)
  - Git 작업 소요 시간 (commits, tree walk, diff 계산)
  - tree-sitter 파싱 시간
  - 키 입력 처리

```rust
// debug.rs
fn init_logging(level: &str) {
    let file = File::create("gluck.log").unwrap();
    tracing_subscriber::fmt()
        .with_max_level(TracingLevelFilter::from(level))
        .with_writer(file)
        .with_ansi(false)
        .init();
}
```

### 성능 측정 (tracing + span)

- `tracing::instrument`로 함수별 자동 소요 시간 측정
- `--perf` 플래그로 성능 요약 출력 모드
- 측정 대상:
  - `GitRepo::commits()` — 커밋 리스트 로딩
  - `GitRepo::tree()` — 파일 트리 탐색
  - `GitRepo::blob()` — 파일 내용 로딩
  - `GitRepo::diff()` — Diff 계산
  - `HighlightEngine::highlight()` — Syntax 하이라이팅
  - `App::render()` — 프레임 렌더링

```rust
#[tracing::instrument(skip(self))]
fn load_commits(&self) -> Result<Vec<CommitInfo>> {
    // 자동으로 elapsed 시간 로깅
}
```

### 디버그 모드

- `--debug` 플래그: 상태 패널 표시 (현재 모드, 커서 위치, 로드된 데이터 수 등)
- `Ctrl+D` 키: 런타임에 디버그 오버레이 토글
- 대형 저장소(10K+ 커밋) 성능 프로파일링을 위한 벤치마크 스크립트

## CLI 인터페이스

프로젝트 이름은 `gluck`, 실행 파일 이름은 `glc`.

```
glc [options] [path]

Options:
  path              Git 저장소 경로 (기본: 현재 디렉토리)
  --log-level LEVEL 로그 레벨 (trace|debug|info|warn|error)
  --debug           디버그 오버레이 활성화
  -h, --help        도움말
  -V, --version     버전 정보
```

`Cargo.toml`에서 `[[bin]] name = "glc"`로 설정.
