# Ask: LLM 코드/변경 설명 기능 설계

- 날짜: 2026-10-21
- 상태: 리뷰 반영 (Fable 5.1 리뷰, 하단 "리뷰 반영" 참고)

## 목표

코드 리뷰 중 낯선 언어, 이디엄, 구문, 변경의 의도를 TUI를 떠나지 않고 LLM에게 물어본다.

1. Page explain: Diff/View 모드에서 지금 화면에 보이는 내용만으로 "이 변경이 왜 일어났는가 / 이 코드는 무엇을 하는가"를 설명.
2. Selection explain: 마우스 드래그로 선택한 텍스트 블록을 설명. 키보드 선택은 범위 밖.

비목표 (MVP 이후)
- 멀티턴 대화, 자유 질문 입력창 (후속 단계로 설계만 열어 둠)
- 키보드 기반 선택
- 응답 캐시(메모리/디스크), 비용 집계
- 드래그가 pane 경계를 넘을 때 자동 스크롤

## 제약

- 저가/공개 모델 위주 사용. 모델이 알아서 잘 해주길 기대할 수 없으므로 프롬프트가 구조와 분량을 강하게 지정한다.
- provider/model/API 키는 config 파일(`dirs::config_dir()/gluck/config.toml`, macOS는 `~/Library/Application Support/gluck/config.toml`)에 명시적으로 저장한 값이 우선이고, 비어 있는 항목만 셸 환경변수로 fallback 한다.
- UI 스레드를 막지 않는다 (기존 search/index 스레드 패턴과 동일하게 `mpsc`).
- 테스트는 네트워크 없이 돌아야 한다.

## 프로바이더

사용 예정 프로바이더 대부분이 OpenAI Chat Completions 호환 엔드포인트를 제공한다. 단일 클라이언트(`POST {base_url}/chat/completions`, `stream: true`, SSE)로 전부 커버한다.

아래 preset은 `[ask.providers.<name>]`의 이름이 일치할 때 `base_url`/`model`/키 env의 기본값으로 쓰인다.

| preset | fallback env | base_url | 기본 model |
|---|---|---|---|
| openrouter | `OPENROUTER_API_KEY` | `https://openrouter.ai/api/v1` | `deepseek/deepseek-chat` |
| deepseek | `DEEPSEEK_API_KEY` | `https://api.deepseek.com/v1` | `deepseek-chat` |
| gemini | `GEMINI_API_KEY` | `https://generativelanguage.googleapis.com/v1beta/openai` | `gemini-2.5-flash` |
| groq | `GROQ_API_KEY` | `https://api.groq.com/openai/v1` | `llama-3.3-70b-versatile` |
| cerebras | `CEREBRAS_API_KEY` | `https://api.cerebras.ai/v1` | `llama-3.3-70b` |
| mistral | `MISTRAL_API_KEY` | `https://api.mistral.ai/v1` | `mistral-small-latest` |
| openai | `OPENAI_API_KEY` | `https://api.openai.com/v1` | `gpt-4.1-mini` |

해석 순서 (config 우선, env fallback):

1. provider: `[ask].provider`가 있으면 그 이름. 비어 있으면 표 순서대로 fallback env가 설정된 첫 preset.
2. api_key: `[ask.providers.<name>].api_key` → `[ask.providers.<name>].api_key_env`가 가리키는 env → preset의 fallback env → 없음.
3. base_url/model: `[ask.providers.<name>]` 값 → preset 기본값. 둘 다 없으면 설정 오류.
4. 키 출처(config/env)는 패널 제목에 표시한다.

- `[ask.providers.<name>]`의 `base_url`, `model`, `api_key`가 preset 값을 덮어쓴다. 프리셋에 없는 호환 서버(로컬 Ollama `http://localhost:11434/v1` 등)는 `base_url` + `model`을 직접 지정하고 `api_key`는 비워 둘 수 있다 (Authorization 헤더 생략).
- 여러 provider를 등록해 두고 `provider` 값만 바꿔 전환한다. UI 내 전환 키는 후속.
- Anthropic Messages API는 형식이 달라 MVP에서는 제외. `Provider` trait 없이 `Endpoint { base_url, key, model }` 구조체 하나로 시작하고, 필요해지면 wire format enum을 추가한다.

HTTP 크레이트: `ureq` 3 (blocking, rustls 기본). 이미 worker thread 모델이라 async 런타임이 필요 없다. 응답 body를 `BufRead`로 읽으며 `data: {...}` 줄을 파싱한다.

- `http_status_as_error(false)`: 기본값이면 non-2xx의 body가 버려지므로 끄고 직접 status를 검사해 body 앞부분을 에러로 보여 준다.
- `timeout_global(None)`, `timeout_recv_body(None)`, `timeout_connect(Some(10s))`: 느린 스트림이 중간에 끊기지 않게.

## 아키텍처

```
src/ask/
├── mod.rs        # AskRequest, AskKind, AskMessage, spawn_ask()
├── provider.rs   # preset 표, AskConfig + env lookup(주입 가능) → Endpoint 해석
├── client.rs     # ureq POST + SSE 파서 (parse_sse(reader, on_delta))
├── prompt.rs     # system/user 프롬프트 템플릿, detail 레벨
├── context.rs    # DiffFile/텍스트 + 줄 범위 → 프롬프트용 텍스트 (ui/ 의존 없음)
└── panel.rs      # AskPanel 상태 머신 (Closed/Streaming/Done/Failed), 스크롤
src/ui/
└── ask_panel.rs  # 하단 드로어 렌더링
```

### 데이터 흐름

```
key 'a' ─→ App::ask()
          ├─ context::build(mode, selection, visible window) → AskContext
          ├─ prompt::render(AskContext, detail) → Vec<ChatMessage>
          └─ spawn_ask(endpoint, messages) → thread
                 └─ client::stream → AskMessage::Delta(String) … Done | Failed(String)
main loop ─→ app.drain_ask_messages()  (app.ask_streaming() 이면 poll 80ms)
```

- 요청마다 새 channel. 패널을 닫거나 재요청하면 기존 `Receiver`를 drop하고, worker는 다음 chunk의 `send` 실패로 루프를 빠져나간다. 별도 cancel flag 없음.
- 요청은 `Vec<ChatMessage>`로 표현해 후속 멀티턴(`?` 자유 질문)에서 assistant/user 턴을 덧붙이기만 하면 되게 한다.

### 컨텍스트 구성 (`context.rs`)

순수 함수로 만들어 단위 테스트한다.

Page explain (Diff):
- 메타: repo 이름, commit short_id + 메시지 전체(제목+본문), 파일 경로, `Language::from_path` 결과, 파일의 +/- 통계.
- 본문: 화면에 실제로 보이는 diff line을 unified 형식(` `, `+`, `-` 마커 + 옛/새 줄번호)으로. 보이는 범위는 렌더 시 캡처한 row_map의 min..=max로 정한다 (wrap 시 `scroll + height`는 과대 추정). side-by-side여도 항상 unified로 직렬화.
- hunk 자체가 context 줄을 포함하므로 Diff page에는 extra context를 붙이지 않는다.

Page explain (View):
- 메타: commit, 파일 경로, 언어, 이 파일이 해당 커밋에서 변경되었는지 여부.
- 본문: 화면에 보이는 줄들 (줄번호 포함).

Selection explain:
- 선택된 줄들 + 위/아래 `extra_context_lines` 만큼의 주변 코드 (명시적으로 구분 표기).
- 선택은 mouse Up 시점에 `file.lines` 인덱스 범위(Diff) 또는 파일 줄 범위(View) + `Pane`으로 변환해 저장한다. context builder는 `ui/`를 모르고 이 범위만 받는다.
- side-by-side 선택은 pane(old/new)에 해당하는 줄만 뽑고 메타에 기록.

크기 제한: 직렬화 결과가 `ask.max_context_chars`(기본 12_000)를 넘으면 extra context부터 줄이고, 그래도 넘으면 본문을 잘라 `… (truncated)` 표기.

### 프롬프트 (`prompt.rs`)

저가 모델을 상정해 다음을 명시한다.

- 역할: 시니어 리뷰어가 해당 언어가 처음인 동료에게 설명.
- 출력 언어: `ask.language` (기본 "Korean"). 코드 식별자는 원문 유지.
- 출력 구조 고정 (Markdown 헤더):
  - Page/Diff: `## 요약` (1–2문장) → `## 변경 의도` (왜 바뀌었나, 추측이면 추측이라고 표시) → `## 낯선 구문/이디엄` (언어 특유 문법, 표준 라이브러리 API, 패턴을 항목별로; 없으면 생략) → `## 리뷰 포인트` (잠재 버그/주의점, 최대 3개)
  - Selection: `## 무엇을 하나` → `## 구문 해설` (토큰/구문 단위로 쪼개 설명) → `## 왜 이렇게 썼나` (대안 대비 장점) → `## 관련 개념` (검색 키워드 2–3개)
- 금지: 서두/맺음말, 표, 코드 전체 재출력, 화면에 없는 코드에 대한 단정. "첫 줄을 바로 `## 요약`으로 시작"을 명시.
- 배치: system에는 역할/언어/금지 규칙, user 메시지에는 메타 → 코드 블록 → 섹션 템플릿 순서 (저가 모델은 마지막 지시를 가장 잘 따른다).
- 코드 블록 라벨: "unified diff: `-` old, `+` new, 줄번호 old/new". `## 변경 의도`는 커밋 메시지를 근거로 인용하도록 지시.
- 분량: `ask.detail` = `brief` | `normal` | `deep`. 각 레벨이 섹션별 문장 수/항목 수 상한과 `max_tokens`(500 / 1200 / 2500)를 바꾼다. 기본 `normal`.
- few-shot 없음 (토큰 비용). 대신 섹션 템플릿을 그대로 보여 준다.
- 사용자 오버라이드: `ask.system_prompt`가 있으면 기본 system 프롬프트를 대체(템플릿 변수 `{language}`, `{detail}`).

`temperature` 0.2 (config에서 음수로 두면 생략 — reasoning 모델이 temperature를 거부하는 경우 대비). `ask.max_tokens`를 지정하면 detail 기본값을 덮어쓴다.

### 마우스 선택

현재 mouse capture가 켜져 있어 터미널 자체 선택이 동작하지 않는다(대부분의 터미널은 Shift/Option+drag로 우회 가능하지만 앱에 전달되지 않음). 앱 레벨 선택을 구현한다.

- 단위: 줄(logical line) 단위. 문자 단위 선택은 wrap/탭 확장/side-by-side 정렬 때문에 매핑이 복잡하고, "블록을 설명해 달라"는 용도에는 줄 단위로 충분하다. 문자 단위는 후속.
- 이벤트: `Down(Left)` in content pane → anchor 기록, `Drag(Left)` → head 갱신(pane 밖이면 가장 가까운 row로 clamp), `Up(Left)` → 확정. 자동 스크롤은 MVP 제외.
- 좌표 → 줄 매핑: 렌더 시 모드당 하나의 `row_map: Vec<Option<usize>>`(inner 영역 기준 화면 row → logical index, 패딩 row는 None)를 `App`에 저장. side-by-side는 aligned pair마다 양쪽 row 수가 같으므로(`resize_with`) 한 map으로 충분하고, pane은 클릭 col을 두 `Rect`과 비교해 정한다. wrap continuation row도 같은 logical index.
- logical index: View는 파일 줄 인덱스, Diff unified는 `file.lines` 인덱스, side-by-side는 aligned 인덱스. 렌더 시 aligned → (old `file.lines` 인덱스, new 인덱스) 표도 함께 캡처해 선택 확정 시 `file.lines` 범위로 변환.
- `render_diff`는 `&App` → `&mut App`으로 바꾼다 (`render_view`와 동일 패턴).
- 상태: `App.selection: Option<Selection { pane, anchor, head, dragging }>`. 파일/커밋/모드 전환, `v` 토글(인덱스 공간이 달라짐) 시 해제.
- `Esc` 우선순위 (`keybindings.resolve` 이전에 처리): 패널 열림 → 닫기 / 선택 있음 → 해제 / 그 외 기존 동작.
- 렌더: 선택 범위 row에 `palette.highlight_style()` 대신 배경만 바꾸는 `selection` 스타일(REVERSED 또는 theme bg) 적용, gutter 포함.
- 클릭(드래그 없이 Down→Up 같은 row)은 선택을 만들지 않고 기존 선택을 해제.

### UI: Ask 패널

배치: 하단 드로어. body 영역을 세로로 나눠 위 60%는 기존 콘텐츠, 아래 40%는 패널. side-by-side diff의 두 pane을 모두 가리지 않고, 설명을 읽으며 코드를 같이 볼 수 있다. 검색 모달처럼 중앙 overlay로 띄우면 설명 대상 코드를 가린다.

```
┌ DIFF: a1b2c3d ↦ e4f5g6h ──────────────────────────────────┐
│ src/app.rs | src/mode.rs                                   │
│┌ old ───────────────────┐┌ new ──────────────────────────┐│
││ 120  let x = foo();     ││ 120  let x = foo()?;         ││  ← 선택된 줄은 반전
││ ...                     ││ ...                          ││
│└─────────────────────────┘└──────────────────────────────┘│
│┌ Ask · selection L120–124 (new) · deepseek-chat ─ ⠋ ─────┐│
││ ## 무엇을 하나                                          ││
││ `?` 연산자는 Result가 Err일 때 즉시 반환하고 …         ││
│└──────────────────────────────────────────────────────────┘│
 [a] ask  [A] ask (deep)  [J/K] scroll  [r] retry  [Esc] close
```

- 키 (Diff/View 모드):
  - `a`: 선택이 있으면 selection explain, 없으면 page explain. 패널이 열려 있으면 재요청 대신 포커스 이동하지 않고 새 요청으로 교체.
  - `A`: 같은 동작을 `deep` 레벨로.
  - 패널이 열려 있는 동안: `[`/`]`로 페이지 스크롤(패널 높이 − 1줄), 마우스 휠(패널 영역)로 3줄 스크롤 (Diff에서는 `j`/`k`로 1줄 — Diff의 `j`/`k`는 평소 diff 1줄 스크롤이고 파일 이동은 `h`/`l`·←/→가 담당. View는 트리 이동에 `j`/`k`가 필요해 제외), `r` 재요청, `Esc` 닫기 + 스트림 취소. 그 외 키는 기존 모드로 전달되어 코드 탐색이 계속 가능 (j/k/u/d는 코드 쪽).
  - `Ctrl+J`는 legacy 터미널에서 `KeyCode::Enter`로 들어오므로 사용하지 않는다.
- 제목줄: 종류(page/selection L범위/pane) · model · 상태(spinner / done / error).
- 본문: 스트리밍 중에는 plain text로 wrap(`wrap::wrap_spans`), 완료 시 기존 `HighlightEngine`의 Markdown 하이라이트를 적용. 자동 스크롤은 사용자가 위로 스크롤하지 않은 경우에만 맨 아래 추적.
- SSE 파서 관용: `data:{`(공백 없음), `:` 주석 줄(OpenRouter keep-alive), `choices: []` usage chunk, `delta.content: null`, `delta.reasoning_content`(무시), HTTP 200 중간의 `{"error":{...}}`. `finish_reason == "length"`는 본문 끝에 `(truncated)` 표시.
- 에러: provider 미설정이고 fallback env도 없음 → 패널에 config 경로와 `[ask]`/`[ask.providers.<name>]` 예시, fallback env 이름 목록을 안내. HTTP 오류는 status + 응답 body 앞부분(키 마스킹).
- footer hint에 `[a] ask` 추가. 선택이 있으면 `[a] explain selection`.

### Config

```toml
[ask]
provider = "deepseek"    # 비우면 fallback env 자동 탐지
language = "Korean"
detail = "normal"        # brief | normal | deep
max_tokens = 0           # 0 = detail 기본값 (500/1200/2500)
temperature = 0.2
extra_context_lines = 20 # selection 전용
max_context_chars = 12000
system_prompt = ""       # 비어 있으면 내장 템플릿

[ask.providers.deepseek]
api_key = "sk-..."         # 생략 시 DEEPSEEK_API_KEY
# base_url, model 생략 시 preset 기본값

[ask.providers.openrouter]
api_key = "sk-or-..."
model = "google/gemini-2.5-flash"

[ask.providers.local]
base_url = "http://localhost:11434/v1"
model = "qwen2.5-coder:7b"
```

## 보안/프라이버시

- 요청 시점에만 화면 내용이 외부로 전송된다. 백그라운드 전송 없음.
- 키는 로그/에러 메시지에 남기지 않는다 (`tracing`에 Authorization 헤더 미기록, 에러 body에서 키 문자열 마스킹).
- 패널 제목에 provider/model이 항상 보이므로 어디로 보내는지 사용자가 알 수 있다.
- config 파일에 키가 평문으로 저장된다. `Config::save()`는 Unix에서 파일 권한을 0600으로 설정하고, 로드 시 group/other 읽기 권한이 있으면 `tracing::warn`.
- 테마 변경 시 `Config::save()`가 `[ask]` 블록 전체(키 포함)를 다시 기록한다 — 라운드트립 테스트로 키/provider 보존을 보장.
- `Debug` 출력에 키가 찍히지 않도록 `api_key`는 수동 `Debug` 구현으로 마스킹.

## 테스트

- `provider`: env lookup 클로저 주입. config 우선/env fallback 순서, provider 자동 탐지 순서, preset 기본값 + override, 미설정 에러, 키 없는 로컬 provider.
- `config`: `[ask.providers.*]` 파싱/라운드트립(키 보존), 0600 권한.
- `client::parse_sse`: 고정 바이트 스트림 → delta 순서, `[DONE]`, 빈 줄/주석 줄, 잘린 JSON 처리.
- `context`: 작은 `DiffFile`/`highlighted` 픽스처로 page/selection 직렬화, truncation.
- `prompt`: detail 레벨별 섹션 지시 포함 여부, system_prompt 오버라이드.
- `panel`: 닫힌 뒤 메시지 무시, 스크롤 clamp, follow 모드.
- `app`: 마우스 Down/Drag/Up → selection 범위 (기존 `mouse()` 헬퍼 + `render_to_lines`로 row_map 채움), 파일 전환/`v` 토글 시 해제, Esc 우선순위, 키 없을 때 `a` → Failed 패널.

## 구현 순서

1. `ask/provider.rs`, `ask/prompt.rs`, `ask/context.rs` (순수 로직 + 테스트)
2. `ask/client.rs` (ureq + SSE) + `ask/mod.rs` spawn/메시지
3. `ask/panel.rs` + `ui/ask_panel.rs` + 메인 루프 drain/poll
4. 마우스 선택 (row_map 캡처, Drag/Up 처리, 하이라이트)
5. config `[ask]`, footer, README/AGENTS.md 갱신
6. 수동 검증: 실제 키로 Diff page/selection 각각 1회

## 리뷰 반영 (Fable 5.1)

- 사용자 결정 (리뷰 이후): config 파일에 provider/model/키를 명시 저장하는 것이 우선, 셸 env는 fallback.
- 블로킹: `Ctrl+J/K` → `[`/`]`, `align_diff_lines` 비공개 의존 제거(선택을 `file.lines` 범위로 변환), ureq 3 `http_status_as_error(false)` + timeout 설정, `render_diff`의 `&mut App` 전환.
- 변경: row_map 모드당 1개 + page 범위 계산에도 사용, cancel flag 대신 channel-per-request, Esc 우선순위 명시, SSE 관용 규칙, 프롬프트 배치/금지 규칙 강화, detail별 `max_tokens`.
- MVP 제외: 응답 캐시, 드래그 자동 스크롤, Diff page extra context, `ui/selection.rs` 분리.

## 후속

- 패널에서 `?` 로 자유 질문 입력 (현재 컨텍스트 + 이전 답변을 이어 붙이는 멀티턴). 요청이 `Vec<ChatMessage>`라 턴 추가만으로 가능.
- 문자 단위 선택, 드래그 자동 스크롤, Anthropic Messages API wire format.
