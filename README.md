# gluck

[![Version](https://img.shields.io/badge/version-0.3.1-blue)](https://github.com/soomtong/gluck)
[![Rust](https://img.shields.io/badge/rust-edition%202021-orange)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

> **g**it **l**og, **u**nfolds **c**ode into **k**nowledge.
>
> 커밋 로그라는 시간의 주름을 따라 접힌 코드가 펼쳐진다 — 읽고, 비교하고, 이해하는 행위 끝에 지식이 남는다.
>
> 바이너리 이름 `glc`는 손가락이 기억하는 명령어: home row에서 벗어나지 않고 `g l c`.
> **gluck**은 그 뒤에 *u*nfolding과 *k*nowing을 더한, 프로젝트의 온전한 이름이다.

**Git history file viewer** — 터미널에서 git history의 파일을 탐색하고 읽는 TUI 도구.

## 설치

### Cargo

```bash
cargo install --git https://github.com/soomtong/gluck
```

### Homebrew (macOS, Apple Silicon)

> Intel Mac(x86_64)용 바이너리는 GitHub Actions 지원 중단으로 제공하지 않습니다.
> Intel Mac 사용자는 아래 Cargo 설치 방법을 이용해 주세요.

```bash
brew tap soomtong/tap
brew install glc
```

### Linux (직접 빌드)

Linux용 바이너리는 별도로 제공되지 않습니다. Rust 툴체인이 설치되어 있다면 직접 빌드할 수 있습니다.

```bash
cargo install --git https://github.com/soomtong/gluck
```

또는 소스를 받아 빌드:

```bash
git clone https://github.com/soomtong/gluck
cd gluck
cargo build --release
# 빌드 결과: ./target/release/glc
```

## 사용법

```bash
glc                 # 현재 디렉토리의 git history 열기
glc /path/to/repo   # 특정 저장소 열기
```

### Pick 모드 — 커밋 탐색

| 키 | 동작 |
|----|------|
| `j` / `k` / `↑` / `↓` | 커밋 이동 |
| `Enter` / `l` | 선택 커밋 View 모드 |
| `/` | 커밋 검색 |
| `^T` | 색상 테마 전환 |
| `q` | 종료 |

### View 모드 — 파일 읽기

| 키 | 동작 |
|----|------|
| `j` / `k` / `↑` / `↓` | 파일 트리 이동 |
| `u` / `d` | 내용 스크롤 (3줄) |
| `J` / `K` | 내용 페이지 스크롤 |
| `.` | .gitignore 파일 필터 토글 |
| `Tab` | Diff 모드 전환 |
| `Esc` / `h` | Pick 모드 |
| `^N` / `^P` | 다음/이전 커밋 이동 |
| `^T` | 색상 테마 전환 |
| `q` | 종료 |

### Diff 모드 — 변경 비교

| 키 | 동작 |
|----|------|
| `j` / `k` / `↑` / `↓` | 변경 파일 이동 |
| `h` / `l` / `←` / `→` | 변경 파일 이동 |
| `u` / `d` | diff 내용 스크롤 (3줄) |
| `J` / `K` | diff 내용 페이지 스크롤 |
| `s` | side-by-side / unified 토글 |
| `Tab` | View 모드 |
| `Esc` | Pick 모드 |
| `^N` / `^P` | 다음/이전 커밋 쌍 이동 |
| `^T` | 색상 테마 전환 |
| `q` | 종료 |

## 설정

설정 파일은 XDG config 경로에 저장됩니다.

- **macOS**: `~/Library/Application Support/gluck/config.toml`
- **Linux**: `~/.config/gluck/config.toml`

```toml
[theme]
# 색상 테마: plain (기본), catppuccin, tokyo-night, nord, gruvbox, one-light
name = "plain"

[ui]
# u/d 키 스크롤 줄 수 (기본: 3)
scroll_lines = 3
```

## 배경

오픈소스 프로젝트를 공부할 때 가장 좋은 방법 중 하나는 git history를 따라가며 코드가 어떻게 변해왔는지 읽는 것이다. 하지만 기존 도구들은 각각 한계가 있다:

- **git log / git show** — 변경사항은 볼 수 있지만 해당 시점의 전체 파일을 읽기 불편하다
- **tig** — log 탐색에 훌륭하지만 파일 뷰어로는 부족하다
- **GitHub/GitLab** — 브라우저가 필요하고 커밋 간 이동이 느리다

gluck은 git history를 타임라인으로 삼아, 각 시점의 파일을 읽고 비교하는 데 집중한다.

## 핵심 기능

### Git log 탐색 (Pick)

- 커밋 리스트를 탐색하고 선택
- 파일 단위로 변경 이력 필터링
- 커밋 메시지, 작성자, 날짜 기반 검색

### View 모드

- 선택한 커밋 시점의 전체 파일 내용 조회
- 파일 트리 탐색 (해당 커밋 기준)
- syntax highlight 지원

### Diff 모드

- 두 커밋 간 변경사항 비교 (side-by-side / unified)
- 파일 단위 diff, 특정 함수/블록 단위 diff
- 변경된 파일만 필터링하여 보기

### Syntax Highlight

- 주요 언어 syntax highlight
- 터미널 색상 테마 지원

## 사용 시나리오

- 오픈소스 프로젝트를 처음 공부할 때 초기 커밋부터 따라가며 코드 읽기
- 특정 기능이 언제, 어떻게 추가되었는지 추적
- 버그 수정 커밋을 찾아 어떤 변경이 있었는지 확인
- 프로젝트 문서(history 내 README, docs 등)의 변천사 읽기

## 기술 방향

- Terminal 전용 TUI 도구
- 로컬 git repository 기반 동작

## 영감 / 참고

- [tig](https://github.com/jonas/tig) — ncurses 기반 git viewer
- [gitui](https://github.com/extrawurst/gitui) — Rust 기반 terminal git UI
- [delta](https://github.com/dandavison/delta) — syntax-highlighted diff viewer
