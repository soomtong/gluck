# HANDOFF: gluck 시맨틱 검색 구현 — turbovec + fastembed + Korean BM25

## 목표

gluck(터미널 TUI git history viewer)에 하이브리드 시맨틱 검색을 추가한다.
- `glc index` 서브커맨드로 레포를 인덱싱
- TUI에서 `S` 키로 통합 검색 모달 (Files + Commits 동시 표시)
- BM25(tantivy) + Vector(turbovec 4-bit) + RRF fusion
- 한국어 커밋 메시지 지원 (character bigram tokenizer)
- 단일 바이너리 유지 (turbovec은 pure Rust, native dep 없음)
- **주의**: fastembed는 내부적으로 `ort` (ONNX Runtime)를 사용하므로 native dep이 존재함

## 현재 브랜치

`semantic` — 시맨틱 검색 구현 브랜치 (main에서 분기)

## 완료된 작업

- [x] 시맨틱 검색 아키텍처 리뷰 (Claude Chat과 협업)
  - CodeBERT/ort → fastembed 교체 결정
  - Korean bigram tokenizer 도입 결정
  - tree-sitter 함수 단위 청킹 결정
  - turbovec IdMapIndex 4-bit 백엔드 확정
  - 통합 모달 (Files + Commits) 디자인 확정
- [x] 구현 계획 작성: `docs/superpowers/plans/2026-05-23-turbovec-semantic-search.md`
  - 12개 Task, TDD 방식, 완전한 코드 포함

## 시도했으나 제외된 접근

- **CodeBERT + ort**: ONNX Runtime native dep 필요 → gluck 단일 바이너리 약속 위반. 폐기.
  - **단, fastembed도 내부적으로 `ort`를 사용한다.** "native dep 없음" 목표는 아직 달성되지 않은 상태. fastembed를 제거하려면 벡터 검색을 포기(BM25 전용)하거나 pure-Rust 임베딩 크레이트로 교체해야 함.
- **brute-force f32 cosine**: `docs/superpowers/plans/2026-05-22-semantic-search.md`의 구버전 계획. turbovec 4-bit로 교체 (8x 메모리 절감 + SIMD).
- **모드별 필터링 모달** (Pick→Commits only, View→Files only): 통합 모달로 단일화.

## 남은 작업

구현 계획대로 12개 Task를 순서대로 실행한다.
`docs/superpowers/plans/2026-05-23-turbovec-semantic-search.md` 참조.

- [x] **Task 1**: 의존성 추가 + 모듈 스켈레톤 (`Cargo.toml`, `src/search/` 디렉토리 신규 생성)
- [x] **Task 2**: VectorIndex — turbovec IdMapIndex 래퍼 (`src/search/vector.rs`)
- [x] **Task 3**: Bm25Index + Korean bigram tokenizer (`src/search/bm25.rs`, `src/search/chunk.rs`)
- [x] **Task 4**: RRF fusion (`src/search/rrf.rs`)
- [x] **Task 5**: Chunk 타입 + tree-sitter Rust 함수 청킹 (`src/search/chunk.rs` 완성)
- [x] **Task 6**: EmbeddingModel — fastembed 래퍼 + 테스트 stub (`src/search/embedding.rs`)
- [x] **Task 7**: 인덱서 파이프라인 (`src/search/indexer.rs`)
- [x] **Task 8**: CLI 서브커맨드 (`src/cli.rs`, `src/main.rs`)
- [x] **Task 9**: SearchConfig (`src/config.rs`)
- [x] **Task 10**: SemanticSearchModal 상태머신 (`src/search/modal.rs`, `src/mode.rs`)
- [x] **Task 11**: UI 렌더러 + App 통합 (`src/ui/search_modal.rs`, `src/app.rs`)
- [x] **Task 12**: E2E 검증 + `.gitignore`

## 핵심 파일

### 이미 존재하는 파일 (수정 대상)
- `Cargo.toml` — `turbovec = "0.1.3"`, `tantivy = "0.22"`, `fastembed = "4"` 추가 필요
- `src/lib.rs` — `pub mod search;` 추가 필요
- `src/cli.rs` — `Commands::Index { force, ... }` 추가 필요
- `src/main.rs` — 서브커맨드 라우팅 추가 필요
- `src/mode.rs:161-175` — `Action` enum에 `SemanticSearch` 추가 필요
- `src/mode.rs:183-210` — `KeyBindings::default_bindings()`에 `S` 키 바인딩 추가 필요
- `src/app.rs:16-30` — `App` 구조체에 `search_engine`, `search_modal` 필드 추가 필요
- `src/app.rs:111-163` — `handle_key`에 모달 인터셉트 로직 추가 필요
- `src/app.rs:58-68` — `render()`에 모달 오버레이 추가 필요
- `src/config.rs:7-11` — `Config` 구조체에 `search: SearchConfig` 필드 추가 필요
- `src/ui/mod.rs` — `pub mod search_modal;` 추가 필요

### 신규 생성 대상
- `src/search/mod.rs` — SearchEngine facade, Meta, SearchError, DocKind, SearchResult
- `src/search/vector.rs` — VectorIndex (turbovec::IdMapIndex 래퍼), l2_normalize
- `src/search/bm25.rs` — Bm25Index, BigramTokenizer (Korean 지원)
- `src/search/rrf.rs` — rrf_fuse
- `src/search/chunk.rs` — Chunk, split_file (tree-sitter + fixed-size)
- `src/search/embedding.rs` — EmbeddingModel (fastembed + stub)
- `src/search/indexer.rs` — build_index_with_model
- `src/search/modal.rs` — SemanticSearchModal, ModalAction
- `src/ui/search_modal.rs` — ratatui overlay 렌더러

### 참고 문서
- `docs/superpowers/plans/2026-05-23-turbovec-semantic-search.md` — **구현 계획 (완전한 코드 포함)**
- `docs/2026-05-23-turbovec-integration-plan.md` (= `docs/plans/2026-05-23-turbovec-integration-plan.md`) — 설계 결정 배경
- `docs/superpowers/plans/2026-05-22-semantic-search.md` — 구버전 계획 (ONNX, 참고용만)

## 아키텍처 요점

```
Chunk (doc_id: u64)
  ├─ CommitMessage  → BM25 + Vector
  └─ File (tree-sitter fn / fixed-size) → BM25 + Vector

BM25Index::search(query) → Vec<(u64, f32)>   ← u64 = monotonic doc_id
VectorIndex::search(embedding, k) → Vec<(u64, f32)>

rrf_fuse(bm25, vec, k=60.0, limit=20) → Vec<(u64, f32)>

SearchEngine::hydrate(hits) → Vec<SearchResult>
  └─ doc_store: HashMap<u64, DocMeta>  (scan Tantivy at open)

Index layout:
  .glc-index/
    meta.toml          (version=2, head_oid, vector_backend="turboquant_4bit")
    bm25/              (Tantivy index)
    vectors/index.tvim (turbovec IdMapIndex)
```

## 설계 결정 요약

| 결정 | 선택 | 이유 |
|------|------|------|
| Vector backend | turbovec IdMapIndex 4-bit | native dep 없음, 8x 압축, data-oblivious |
| Embedding | fastembed AllMiniLML6V2 | 자동 다운로드, **ort(ONNX Runtime) native dep 있음**, 한국어는 JinaEmbeddingsV3로 교체 가능 |
| BM25 tokenizer | BigramTokenizer (custom Tantivy) | 한국어 bigram + 영어 whitespace 병행 |
| Chunking | tree-sitter Rust 함수 + fixed-size | gluck이 이미 tree-sitter-rust 보유 |
| doc_id | 단조 증가 u64 | Tantivy stored TEXT + turbovec 동일 space 공유 |
| meta version | 2 | 이전 brute-force 인덱스와 호환 불가 → --force 유도 |
| 모달 | 통합 (Files + Commits) | power-user 친화, Tab으로 섹션 전환 |

## 다음 에이전트에게

1. `superpowers:executing-plans` 또는 `superpowers:subagent-driven-development` 스킬을 사용해 계획을 실행한다.
2. 계획 파일: `docs/superpowers/plans/2026-05-23-turbovec-semantic-search.md`
3. **Task 1부터 순서대로** 실행 — 의존성 체인이 있음 (vector → bm25 → rrf → chunk → embedding → indexer 순)
4. 각 Task는 테스트 먼저 작성 → 실패 확인 → 구현 → 통과 확인 → 커밋 순서
5. turbovec IdMapIndex ID 타입 주의: 계획에서는 `i64` 캐스팅 처리했으나, 실제 API가 `u64`면 캐스팅 제거 (`src/search/vector.rs` Task 2 주의사항 참조)
6. fastembed v4 API: `TextEmbedding::try_new(InitOptions::new(model))` 패턴 — v5로 업그레이드 시 API 변경 확인
7. `src/search/embedding.rs`의 `MODEL` 상수를 `JinaEmbeddingsV3`로 바꾸면 한국어 품질 향상 (1024-dim, fastembed v4 지원 여부 확인 필요)
