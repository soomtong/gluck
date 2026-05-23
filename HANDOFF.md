# HANDOFF: gluck 시맨틱 검색 구현 완료 — 버그 픽스 단계

## 목표

gluck(터미널 TUI git history viewer)에 하이브리드 시맨틱 검색을 추가한다.
- `glc index` 서브커맨드로 레포를 인덱싱
- TUI에서 `S` 키로 통합 검색 모달 (Files + Commits 동시 표시)
- BM25(tantivy) + Vector(turbovec 4-bit) + RRF fusion
- 한국어 커밋 메시지 지원 (character bigram tokenizer)
- 단일 바이너리 유지 (model2vec-rs는 pure Rust, ONNX 없음)

## 현재 브랜치

`semantic-v2` — 시맨틱 검색 구현 브랜치 (main에서 분기)

## 완료된 작업

- [x] **Task 1–12**: 전체 구현 완료 (커밋 `7fdd99b`)
  - BM25 + Vector + RRF 인덱스 파이프라인
  - `glc index` CLI 서브커맨드
  - SemanticSearchModal TUI
  - model2vec-rs + turbovec 통합
- [x] **버그 픽스 1**: `src/search/chunk.rs:49` — UTF-8 char boundary 패닉 수정
  - 한국어 문자(3바이트)가 2048 바이트 경계에 걸릴 때 발생하는 `panic`
  - `content[..2048]` → `content[..content.floor_char_boundary(2048)]` 로 수정
  - **아직 커밋 안 됨** (staged 필요)
- [x] **버그 픽스 2**: `src/search/vector.rs` — 미사용 `TempDir` import 제거
  - **아직 커밋 안 됨**

## 시도했으나 제외된 접근

- **CodeBERT + ort**: ONNX Runtime native dep → 단일 바이너리 약속 위반. 폐기.
- **fastembed v4**: 내부적으로 `ort` 사용 → 동일 이유 폐기.
- **글로벌 모델 캐시**: `dirs::cache_dir()/glc/models/` + hf-hub 직접 사용 방안 검토했으나 보류.
  - 현재는 model2vec-rs가 hf-hub 기본값(`~/.cache/huggingface/hub/`) 사용 — 변경 없음.

## 남은 작업

- [ ] **커밋**: chunk.rs + vector.rs 버그 픽스 커밋
- [ ] **E2E 검증**: `glc index` 실행 시 패닉 없이 완료되는지 확인
- [ ] **TUI 검증**: `S` 키 눌러 모달 동작 확인
- [ ] **PR**: `semantic-v2` → `main` 병합

## 핵심 파일

- `src/search/chunk.rs:49` — `floor_char_boundary(2048)` 픽스 위치 (미커밋)
- `src/search/vector.rs:68` — TempDir import 제거 (미커밋)
- `src/search/embedding.rs` — EmbeddingModel, model2vec-rs 래퍼
- `src/search/indexer.rs:68` — `model_dir = index_dir.join("model")` (현재 사용 안 됨, 데드 코드)
- `src/search/mod.rs` — SearchEngine facade
- `src/search/modal.rs` — SemanticSearchModal 상태머신
- `src/ui/search_modal.rs` — ratatui 오버레이 렌더러
- `src/app.rs` — App 통합 (search_engine, search_modal 필드)
- `docs/plans/2026-05-23-semantic-search-design-v2.md` — 설계 문서 (완전한 spec)

## 다음 에이전트에게

1. `src/search/chunk.rs`와 `src/search/vector.rs` 두 파일을 커밋
2. `cargo build` 후 `glc index` 실행해서 패닉 없이 완료되는지 확인
3. TUI 실행해서 `S` 키 → 검색 모달 동작 확인
4. 이상 없으면 `semantic-v2` → `main` PR 생성

### 참고: indexer.rs 데드 코드
`src/search/indexer.rs:68`의 `model_dir = index_dir.join("model")` 은 실제로 사용되지 않음.
model2vec-rs의 `from_pretrained(MODEL_ID)` 호출 시 HF hub 기본 캐시(`~/.cache/huggingface/hub/`)를 사용.
`.glc-index/model/` 디렉토리는 생성되지 않음. 향후 정리 대상.
