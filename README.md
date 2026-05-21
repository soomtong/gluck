# gluck

Git history file viewer — 터미널에서 git history의 파일을 탐색하고 읽는 도구.

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
