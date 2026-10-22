# Diff 모드 syntax highlight + 배경색 변경 표시 계획

## 목표

hunk(hunkdiff) 방식처럼 Diff 모드에서 추가/삭제/컨텍스트 줄 모두 tree-sitter 문법 색을 유지하고,
변경 여부는 글자색 대신 줄 배경색(tint)으로 표시한다.

참고 화면(hunk 0.24)에서 관찰된 요소:

1. 모든 줄이 syntax highlight 된다. 추가/삭제 줄도 키워드·타입·함수 색이 그대로다.
2. 삭제 줄은 어두운 빨강, 추가 줄은 어두운 초록 배경이 행 전체 폭(오른쪽 끝까지)을 채운다.
3. 줄 번호와 `+`/`-` 마커는 밝은 빨강/초록 글자색으로 표시된다.
4. 짝지어진 삭제/추가 줄에서 실제로 바뀐 토큰만 한 단계 더 진한 배경으로 강조된다(intra-line word diff).
   예: `.unwrap_or_default()`, `|| app.ask_streaming()`.
5. 왼쪽 끝 세로 막대(빨강/초록/회색)로 변경 종류를 다시 표시한다.
6. (범위 밖) 변경 없는 구간 접기(`▾ 120 unchanged lines`), `@@` hunk 헤더.

이번 계획은 1~4를 대상으로 하고, 5는 선택 사항, 6은 별도 작업으로 둔다.

## 현재 구조

- `src/git/diff.rs` — `DiffLine::{Context, Added, Removed}`에 `line_no`와 `content`(개행 포함)만 있다.
  context 3줄, git2 `diff_tree_to_tree`.
- `src/ui/diff.rs::style_for_line` — Added는 `fg(palette.added)`, Removed는 `fg(palette.removed)`, Context는 기본 스타일.
  내용은 `Span::styled(expand_tabs(content), style)` 한 덩어리라 하이라이트가 전혀 없다.
- `diff_line_rows` → `wrap::wrap_spans`가 span 단위로 줄바꿈하므로 여러 span(하이라이트 결과)을 그대로 넘길 수 있다.
- `HighlightEngine::highlight(source, path) -> Vec<Line<'static>>` — 파일 전체를 줄 단위 `Line`으로 돌려준다.
- `App.content_cache: FileContentCache` — `(commit oid, path)` → `FileContent::Text { raw, highlighted }`. View 모드가 채우고, 테마 변경 시 비워진다.
- `selection::selected_style()`은 `REVERSED`.

## 설계

### 1. 하이라이트 소스: 줄 단위가 아니라 blob 전체

diff 조각(hunk)만 하이라이트하면 여러 줄 문자열·블록 주석·문서 주석 안쪽 줄이 잘못 칠해진다.
hunk도 old/new 파일 전체를 기준으로 칠하는 것으로 보인다(화면의 `None => {}` 등 hunk 경계 줄 색이 정확함).

- 삭제/컨텍스트(old 쪽): `(from.id, old_path)` blob을 하이라이트해서 `old_line_no - 1`로 인덱싱
- 추가/컨텍스트(new 쪽): `(to.id, new_path)` blob을 하이라이트해서 `new_line_no - 1`로 인덱싱
- Context 줄은 new 쪽을 사용(unified), side-by-side는 각 pane이 자기 쪽을 사용

`content_cache` 키가 View 모드와 동일하므로 View → Tab → Diff 진입 시 new 쪽은 캐시 적중한다.
`load_view_file()`의 blob 읽기·바이너리 판별·`MAX_HIGHLIGHT_BYTES` 분기를
`App::load_file_content(&CommitInfo, &str) -> Option<FileContent>`로 추출해 두 모드가 공유한다.

폴백(현재 동작 유지):

- `FileChange::Added`면 old 없음, `Deleted`면 new 없음 — 해당 쪽은 필요 없음
- blob 읽기 실패, 바이너리, 인덱스 범위 초과, 줄 내용 불일치 → 해당 줄만 plain 텍스트 + 배경 tint
- 256KB 초과 파일 → `plain_lines` (하이라이트 없이 배경만)

안전장치: `highlighted[n]`의 텍스트가 `expand_tabs(strip_eol(content))`와 다르면 plain으로 폴백한다(CRLF, lossy UTF-8 등으로 어긋나는 경우 대비). 디버그 빌드에서는 `tracing::debug!`로 기록.

### 2. 하이라이트 결과 보관

`DiffState`에 선택 파일의 하이라이트 결과를 둔다.

```rust
pub struct DiffHighlight {
    pub key: (Oid, Oid, usize),             // (from, to, diff_result.files index)
    pub old: Option<Vec<Line<'static>>>,
    pub new: Option<Vec<Line<'static>>>,
}
// App
pub diff_highlight: Option<DiffHighlight>,
```

- `render_diff`가 이미 `&mut App`을 받으므로 렌더 시작 시 `App::ensure_diff_highlight()`를 호출한다.
  `(from oid, to oid, file index)` 키가 같으면 아무것도 하지 않으므로, 파일 선택이 바뀌는 모든 지점
  (Diff 진입, `h`/`l`/←/→, Ctrl+N/P, repo refresh)에 훅을 달 필요가 없다. 결과는 `App.diff_highlight`에 둔다.
- 실제 저장은 `content_cache`의 `FileContent` clone이면 충분(별도 캐시 불필요). `Vec<Line>` clone 비용이 문제 되면
  `FileContent::Text`의 `highlighted`를 `Arc<[Line<'static>]>`로 바꾸는 것을 후속 최적화로 둔다.
- 테마 순환(`Ctrl+T`) 시 `content_cache.clear()` 후 현재 Diff 파일의 `highlight`도 다시 계산.
- 큰 파일의 j/k 디바운스는 Diff에서는 파일 이동이 `h`/`l` 단위라 우선 생략. 체감 지연이 있으면 View의 `pending_view_load` 패턴을 그대로 적용.

### 3. 테마: 배경 tint 색 계산

팔레트마다 필드를 추가하지 않고 `Palette::diff_bg(added, strong) -> Option<Color>`가
`bg`와 `added`/`removed`를 섞어 계산한다(줄 배경 약 20%, 변경 토큰 약 40%).

- 6개 팔레트 모두 별도 값 없이 일관된 tint를 얻는다. 라이트 팔레트도 같은 공식으로 연한 파스텔이 된다.
- `plain` 팔레트(`bg: Color::Reset`, ANSI 16색)는 섞을 수 없으므로 `None` → 기존 글자색 방식으로 폴백.
- 특정 테마에서 어색하면 그 팔레트만 비율을 조정하는 방식으로 대응.

### 4. 렌더링 변경 (`src/ui/diff.rs`)

`style_for_line` 대신 줄 종류별 두 스타일을 만든다.

- `gutter_style`: 마커/줄번호. Added → `fg(added)`, Removed → `fg(removed)`, Context → `fg(dim)`. tint 모드에서는 배경도 같이.
- `bg_style`: Added → `bg(added_bg)`, Removed → `bg(removed_bg)`, Context → 없음.

`diff_line_rows`의 시그니처를 `content: &str` → `content: Vec<Span<'static>>`로 바꾸고,
각 span에 `span.style.patch(bg_style)`을 적용한다(구문 색의 fg는 유지, bg만 덮어씀).

행 끝까지 배경 채우기:

- 각 row `Line`에 `.style(bg_style)`을 지정한다. ratatui 0.30 `Paragraph`가 line style을 행 전체 폭에 적용하는지 먼저 확인
  (확인 실패 시 `width`까지 공백 span으로 패딩, wrap 꺼짐일 때는 pane 내부 폭 기준).
- side-by-side에서 짝이 없는 쪽의 빈 패딩 행(`Line::from("")`)은 tint 없음. hunk처럼 빗금/회색을 넣을지는 2단계에서 결정.

선택 영역: `mark_selected`는 `REVERSED`라 tint 배경이 글자색과 뒤바뀐다. 가독성이 나쁘면
선택 시 `bg(palette.accent)` 계열로 바꾸는 것을 검토(이 변경은 View 모드에도 영향 — 별도 판단).

`unified_line_rows`/`diff_line_rows_side`는 `(old: Option<&[Line]>, new: Option<&[Line]>)`를 받아
`DiffLine` 종류와 줄 번호로 해당 `Line`의 spans를 꺼낸다. 이 매핑을 `fn content_spans(dl, side, hl) -> Vec<Span>`로 분리해 단위 테스트한다.

### 5. 2단계: intra-line word diff

짝지어진 삭제/추가 줄에서 바뀐 부분만 진한 배경으로 강조한다.

- 짝: 이미 있는 `align_indices`의 `(Some(o), Some(n))` 쌍을 그대로 사용(unified도 같은 쌍 정보를 사용).
- 토큰화: 식별자 `[A-Za-z0-9_]+` / 공백 연속 / 그 외 1글자. 문자 단위보다 노이즈가 적다.
- diff: `similar` crate(`TextDiff::from_slices` 또는 `capture_diff_slices`, Myers)를 추가하거나,
  토큰 수가 작으니 자체 LCS(O(n·m), 줄당 수백 토큰 이하)로 구현. 의존성 추가를 피하려면 자체 구현 권장.
- 결과는 "변경된 byte 범위" 목록. 하이라이트 span을 이 경계에서 분할해 해당 구간에 `*_word_bg`를 patch.
  tab 확장 후 문자열 기준으로 계산해야 span 위치와 맞는다.
- 두 줄의 유사도가 낮으면(변경 토큰 비율 > 50%) 강조하지 않는다 — 완전히 다른 줄을 얼룩덜룩하게 칠하는 것 방지.
- 줄 수가 많은 파일 대비 결과를 `DiffHighlight`에 쌍 index → ranges로 캐시.

### 6. Advanced diff color 토글

기존 방식(글자색 diff, 하이라이트 없음)과 새 방식(Advanced diff color)을 모두 유지하고 토글한다.
기존 방식은 `plain` 팔레트 폴백으로 어차피 남아야 하므로 토글의 추가 비용은 작다.

- 설정: `[ui] advanced_diff = true` (기본 켜짐), `c` 키로 토글, 테마/wrap처럼 즉시 저장
- 실제 적용 여부: `config.ui.advanced_diff && palette.diff_bg(..).is_some()`
- 분기 지점은 렌더러의 내용 span 생성과 줄 스타일 계산 두 곳뿐. 줄바꿈·정렬·선택·행 매핑은 공유
- 꺼져 있으면 old/new blob 하이라이트 계산을 건너뛴다. 켜는 순간 렌더 시점의 `ensure_diff_highlight()`가 채운다
- intra-line word diff는 별도 토글 없이 Advanced에 포함(조합 수 증가 방지)
- 하단 힌트 `[c] color`

예상 증가량(테스트 제외): 전체 기능 ~290줄, 그중 토글 자체 ~40줄.
렌더 테스트는 켜짐/꺼짐 두 상태로 돌리고, 꺼짐 상태 기대값은 현재 동작과 동일하게 유지해 회귀 테스트로 쓴다.

선택 사항(보류): 왼쪽 세로 막대(`▌`)로 변경 종류 표시.

## 영향 범위

| 파일 | 변경 |
|---|---|
| `src/theme.rs` | `blend`, `diff_bg` 헬퍼, 테스트 |
| `src/config.rs` | `ui.advanced_diff` |
| `src/mode.rs` | `DiffHighlight`, `Action::ToggleDiffColor` + `c` 바인딩 |
| `src/app.rs` | `load_file_content` 추출, `diff_highlight` + `ensure_diff_highlight`, 토글, 테마 순환 시 무효화 |
| `src/ui/diff.rs` | `style_for_line` 대체, `diff_line_rows` span 입력, 행 배경 채우기, `content_spans` |
| `src/ui/diff_words.rs` (2단계, 신규) | 토큰화 + LCS + span 분할 |
| `src/ask/context.rs` | 영향 없음(raw `content` 사용 그대로) |

`DiffLine`/`compute_diff`는 바꾸지 않는다. 하이라이트는 표시 계층의 일이고 Ask 컨텍스트·통계는 raw 문자열을 쓰므로.

## 테스트

- `content_spans`: Context는 new 쪽 줄, Removed는 old 쪽 `line_no - 1`, Added는 new 쪽, 범위 밖/불일치 시 plain 폴백.
- `init_test_repo` + `add_file_commit`으로 Rust 파일 두 커밋 → `ensure_diff_highlight` 후 old/new가 채워지는지,
  `FileChange::Added`에서 old가 `None`인지.
- 여러 줄 블록 주석 내부 줄이 diff에 들어갔을 때 comment 스타일을 받는지(줄 단위 하이라이트가 아님을 보장).
- 렌더 결과 row의 모든 content span에 `bg(added_bg)`가 있고 fg는 구문 색인지, wrap 시 continuation row도 tint인지.
- plain 팔레트에서는 기존처럼 `fg(added)` 방식인지.
- 2단계: 토큰 diff 단위 테스트(화면 예시 두 쌍), 유사도 임계값, span 분할 경계(멀티바이트 문자 포함).

## 작업 순서

1. 테마 tint 계산 + plain 폴백 판정 + `advanced_diff` 설정/토글 키
2. `load_file_content` 추출 리팩터링 (View 동작 동일, 기존 테스트로 검증)
3. `DiffHighlight` 계산 + 렌더러 교체 + 행 배경 채우기 → 1차 완료 (hunk 1~3 재현)
4. 실제 터미널에서 6개 테마 확인, tint 값 조정, 선택 영역 가독성 확인
5. intra-line word diff (hunk 4 재현)
6. AGENTS.md(CLAUDE.md) Mode state machine 절의 Diff 설명과 Architecture gotchas에 하이라이트 소스 규칙 추가

## 리스크

- 큰 파일 진입 시 old/new 두 번 하이라이트 → 최초 진입 지연. `MAX_HIGHLIGHT_BYTES` 제한과 캐시로 완화, 필요시 디바운스.
- 터미널이 truecolor를 지원하지 않으면 RGB tint가 근사색으로 떨어져 진하게 보일 수 있음(`COLORTERM` 미설정 환경). plain 팔레트 폴백으로 회피 가능.
- 줄 번호 기반 매핑이 rename+수정, CRLF 파일에서 어긋날 수 있음 → 텍스트 일치 검사 폴백으로 방어.
