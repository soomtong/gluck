# Gleam syntax highlighting (+ tree-sitter 런타임 0.27 업그레이드)

`.gleam` 파일을 View/Diff에서 tree-sitter로 하이라이트한다. 선행 작업으로 tree-sitter 런타임을 0.23 → 0.27로 올려 ABI 15 grammar를 재생성 없이 쓴다. 검색 symbol chunking은 범위 밖(후속).

## 배경: ABI

ABI는 tree-sitter CLI가 생성한 `parser.c`의 `TSLanguage` 레이아웃 버전(`#define LANGUAGE_VERSION`)이다. Rust ABI와 무관하다.

| 런타임 | 지원 ABI |
|---|---|
| tree-sitter 0.23 (현재) | 13 ~ 14 |
| tree-sitter 0.27.1 (최신) | 13 ~ 15 |

현재 grammar는 모두 13~14다 (markdown-fork 13, 나머지 crates.io 언어 crate와 vendored linkerscript/lisette 14). 0.27로 올려도 그대로 로드된다.

## Grammar 조사

[gleam-lang/tree-sitter-gleam](https://github.com/gleam-lang/tree-sitter-gleam) (Apache-2.0, external scanner `scanner.c`).

| 후보 | ABI | 0.23 | 0.27 | 비고 |
|---|---|---|---|---|
| crates.io 1.0.0 (2025-01) | 13 | O | O | 문법이 오래됨(`const t = todo` 등에서 ERROR), 번들 query 순서 문제 |
| upstream `main` `fa6d0d9` (2026-09-18) | 15 | X | O | 최신 문법, query 정상 |
| upstream을 CLI 0.23 `--abi 14`로 재생성 | 14 | O | O | lisette 방식, sync마다 재생성 필요 |

- crates.io 1.0.0의 `HIGHLIGHT_QUERY`는 `(identifier) @variable` catch-all이 맨 뒤에 있어, tree-sitter-highlight에서는 fn 이름/const/module이 전부 `variable`로 칠해진다. upstream은 catch-all을 맨 앞으로 옮겨 해결했다.
- upstream `main` + 0.27 런타임: 재생성 없이 로드, 샘플 모두 `has_error=false`, upstream `highlights.scm` 그대로 function/constant/module/parameter/constructor 정상.
- upstream 저장소 루트에 Rust 바인딩(`Cargo.toml`, `bindings/rust`)이 있고, `build.rs`는 최신 템플릿(`std("c11")`, MSVC `-utf-8`)이다. `lib.rs`가 `LANGUAGE`, `HIGHLIGHTS_QUERY`, `LOCALS_QUERY`를 export한다.

## Phase 1: tree-sitter 0.23 → 0.27 업그레이드 (단독 커밋)

Spike(임시 worktree)에서 확인한 변경점은 3개뿐이다.

1. `Cargo.toml`: `tree-sitter = "0.27"`, `tree-sitter-highlight = "0.27"`, `streaming-iterator = "0.1"` 추가
2. `src/highlight/engine.rs`: `Highlighter::highlight`에 `encoding` 인자가 추가됨 → `highlight(config, source.as_bytes(), None, None, |_| None)`
3. `src/search/chunk/symbol.rs`: `QueryMatches`가 `Iterator`가 아니라 `StreamingIterator`로 바뀜, `QueryMatch.captures` 필드가 private → `captures()`
   ```rust
   use streaming_iterator::StreamingIterator;
   let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
   while let Some(m) = matches.next() {
       if let Some(s) = build_symbol_span(m, query, source) { ... }
   }
   // build_symbol_span: for cap in m.captures() { ... }
   ```

언어 crate들은 `tree-sitter-language`로만 결합돼 있어 버전 변경이 필요 없다. 직접 `tree-sitter`에 의존하는 `tree-sitter-markdown-fork`(`>=0.19`)와 `tree-sitter-yaml` 0.6.1(`>=0.21`)도 범위가 열려 있어 그대로 0.27을 따른다.

Spike 검증 결과:

- `cargo test`: 472 passed, 0 failed
- 회귀 비교: 14개 언어 샘플 파일 약 10만 5천 span을 0.23/0.27로 각각 하이라이트해 diff → asm 매크로 한 줄의 span 경계 차이 1건뿐 (사실상 동일)
- MSRV: tree-sitter 0.27은 `rust-version = 1.90`. CI는 `dtolnay/rust-toolchain@stable`, 로컬 1.97 → 문제없음

남은 검증: `cargo clippy --all-targets -D warnings`, `glc report`로 검색 지표(symbol chunk) 전후 비교, Windows release 빌드.

부수 효과: lisette `UPSTREAM.md`의 "CLI 0.23으로 ABI 14 재생성" 단계가 불필요해진다. 다음 sync부터 upstream `parser.c`를 그대로 복사하도록 문서를 고칠 수 있다(선택, 이번 범위에선 문서만 메모).

## Phase 2: Gleam 추가

1. 의존성: git dependency로 rev 고정
   ```toml
   tree-sitter-gleam = { git = "https://github.com/gleam-lang/tree-sitter-gleam", rev = "fa6d0d9" }
   ```
   gluck은 crates.io에 publish하지 않으므로 git dependency에 제약이 없다. 재생성이 필요 없어 vendor할 이유가 사라진다. 오프라인 빌드나 로컬 수정이 필요해지면 그때 `vendor/tree-sitter-gleam/`로 옮긴다.
2. `src/lang.rs`: `Language::Gleam`, `"gleam" => Some(Self::Gleam)`, `as_str` → `"gleam"`, 테스트 `detects_gleam` (`supports_symbol_chunking`은 false)
3. `src/highlight/engine.rs`
   - `make_gleam_config()`: `HighlightConfiguration::new(LANGUAGE.into(), "gleam", HIGHLIGHTS_QUERY, "", LOCALS_QUERY)` — locals를 넘겨야 `#is-not? local`과 parameter 추적이 동작한다. `INJECTIONS_QUERY`(doc comment → markdown)는 injection 콜백이 `None`이라 생략
   - `register_languages()`에 등록
   - `HIGHLIGHT_NAMES`/theme 변경 불필요 (`comment.unused`는 prefix 매칭으로 `comment`, `error`/`warning`은 무색)
   - 테스트: 색 존재, 키워드(`pub`/`fn`/`let`/`case`), fn 이름 → `function`, `io.println`의 `io` → `module`
4. `CLAUDE.md`: Highlighted languages에 Gleam 추가, `tree-sitter` 버전 표기 0.27로 정정, `QueryMatches`가 StreamingIterator라는 점 메모
5. 검증: `cargo test`, clippy, 변경 파일만 `rustfmt`, 실제 Gleam 저장소에서 `glc` View/Diff 육안 확인 (advanced diff color 포함)

## 대안 (업그레이드를 미룰 경우)

upstream `grammar.json`을 `npx tree-sitter-cli@0.23 generate --abi 14`로 재생성해 `vendor/tree-sitter-gleam/`에 넣는다 (lisette와 동일, spike에서 동작 확인). sync마다 재생성이 필요하므로 Phase 1을 권장한다.

## 후속 (선택)

- 검색 symbol chunking: upstream `tags.scm` 참고해 `chunk/symbol.rs`에 Gleam query(function, type, constant) 추가. 인덱스 내용이 바뀌므로 `INDEX_VERSION` 증가.
