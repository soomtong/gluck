# Semantic Search Design v2

## Status

- **Status:** Active, supersedes v1
- **Supersedes:** `docs/superpowers/specs/2026-05-22-semantic-search-design.md`
- **Subsumes:** `docs/superpowers/plans/2026-05-23-turbovec-integration-plan.md`
- **Reference architecture:** [MinishLab/semble](https://github.com/MinishLab/semble) (Python original) / [johunsang/semble_rs](https://github.com/johunsang/semble_rs) (Rust port). Same BM25 + Model2Vec + RRF stack, validated at NDCG@10 = 0.854.

## v1으로부터의 핵심 변경

| 항목 | v1 | v2 |
|---|---|---|
| Embedding 모델 | CodeBERT (768-dim) | **Model2Vec / potion-multilingual-128M** (256-dim) |
| 추론 엔진 | ONNX Runtime (`ort`) | **없음 — 정적 룩업** (`model2vec-rs`) |
| Native 의존성 | 필요 (libonnxruntime) | **없음** |
| Vector backend | brute-force f32 matrix | **turbovec IdMapIndex 4-bit** (8x 압축) |
| 청킹 전략 | 미정의 | **tree-sitter 함수 단위 + fixed-size fallback** |
| 한국어 BM25 | 미정의 | **character bigram (pg_bigm 스타일)** 또는 Lindera |
| TUI runtime | 모호 | **쿼리 임베딩을 TUI에서 직접 수행** (Model2Vec 로딩 ~50ms) |
| 모달 스코핑 | mode-filtered | **통합 모달 — Enter 시 mode 자동 전환** |
| Doc ID 공간 | 분리됨 | **Tantivy ↔ turbovec 동일 `u64` 카운터** |

핵심은 두 가지: **ONNX 제거로 단일 바이너리 정체성 회복**, **검증된 reference architecture(semble)에 정렬**.

---

## Architecture Overview

~~~
              ┌──────────────────────────────────────────────┐
              │              Indexing pipeline                │
              │                                              │
  git history │  GitRepo → CommitWalker → Chunker            │
  (libgit2)   │              │              │                │
              │              ▼              ▼                │
              │     [Commit messages]  [Code chunks]         │
              │              │              │                │
              │              └──────┬───────┘                │
              │                     ▼                        │
              │              SearchDocument                  │
              │              (sum type)                      │
              │                     │                        │
              │      ┌──────────────┼──────────────┐         │
              │      ▼              ▼              ▼         │
              │   Tantivy      model2vec-rs    doc_id        │
              │   (BM25)       (256-dim)       (u64)         │
              │      │              │              │         │
              │      ▼              ▼              ▼         │
              │  bm25/        vectors/        (shared key)   │
              │  index/       index.tvim                     │
              └──────────────────────────────────────────────┘

              ┌──────────────────────────────────────────────┐
              │               Search pipeline                 │
              │                                              │
              │     User query (string)                      │
              │             │                                │
              │   ┌─────────┴─────────┐                      │
              │   ▼                   ▼                      │
              │ Tantivy            model2vec-rs              │
              │ (BM25 top-K)       (encode → query vec)      │
              │   │                   │                      │
              │   │                   ▼                      │
              │   │                turbovec                  │
              │   │                (top-K cosine)            │
              │   ▼                   ▼                      │
              │   Vec<(u64,f32)>   Vec<(u64,f32)>            │
              │           │           │                      │
              │           └─────┬─────┘                      │
              │                 ▼                            │
              │             RRF fusion                       │
              │                 │                            │
              │                 ▼                            │
              │             Hydrate (doc_id → display)       │
              └──────────────────────────────────────────────┘
~~~

모든 컴포넌트가 pure Rust crate. ONNX Runtime, Python, GPU, network 일체 불필요.

---

## Components

### 1. Embedding model — `minishlab/potion-multilingual-128M`

- **Distill 출처:** sentence-transformers/LaBSE (다국어 sentence transformer)
- **Output:** 256-dim static embeddings
- **Context length:** theoretically unlimited (token-level lookup + mean pooling)
- **언어:** 101개, **한국어 포함**
- **Disk size:** ~110MB (model + tokenizer)
- **추론 비용:** 토큰화 + N개 lookup + mean — 수십 µs 단위
- **로딩 시간:** ~50ms (모델 파일 mmap)

대안 검토:

| 모델 | 차원 | 한국어 | 코드 | 메모 |
|---|---|---|---|---|
| **potion-multilingual-128M** | 256 | ✅ 101 lang | OK | **default 선택** |
| potion-code-16M | 256 | ❌ EN only | ✅ 최적화됨 | 향후 dual-embedding 시 후보 |
| potion-retrieval-32M | 512 | ❌ EN only | OK | 영문 retrieval 전용 |
| 직접 distill (ko-sroberta-multitask 등) | 가변 | ✅ 최적 | 가변 | Phase 2 옵션 |

**Phase 2 후보: dual-embedding** — 커밋 메시지(주로 한국어)에는 multilingual, 코드 청크에는 code-16M. 인덱스 크기 2배이지만 검색 품질 의미 있게 향상 예상. 본 spec scope 외.

### 2. Vector backend — `turbovec` IdMapIndex

- **Quantization:** 4-bit (8x 압축)
- **Storage:** 256-dim × 0.5 byte = 128 byte/doc + rotation matrix 256² × 4 byte = 256KB (한 번)
- **10K docs 기준:** ~1.3MB + 256KB
- **검색:** brute-force SIMD scoring (NEON/AVX-512BW), gluck 규모(<100K docs)에서 충분
- **L2 normalization** 인덱싱 직전 수행 (Model2Vec 출력은 unnormalized)

### 3. BM25 — Tantivy

#### 한국어 토큰화

**기본 채택: character bigram** (Tantivy `NgramTokenizer { min: 2, max: 2 }`)

이유:
- 영택님이 AlloyDB에서 이미 검증한 pg_bigm 스타일 — 일관성
- 형태소 분석기(Lindera) 의존성 없음 — 가벼움
- 한국어 + 영어 코드 식별자 모두 합리적 동작
- 조사 변형에 강함 (`"에러처리를"` → `["에러", "러처", "처리", "리를"]`)

**대안: Lindera (mecab-ko 호환)** — 더 정확한 lemmatization, 그러나 사전 파일(~50MB) 의존성. 본 spec MVP에서는 ngram 채택.

#### Schema

~~~rust
let mut schema_builder = Schema::builder();
schema_builder.add_text_field("id",         STORED);                // u64.to_string()
schema_builder.add_text_field("kind",       STORED | STRING);       // "commit" | "file" | "function"
schema_builder.add_text_field("title",      TEXT | STORED);         // ngram tokenizer
schema_builder.add_text_field("body",       TEXT);                  // ngram tokenizer
schema_builder.add_text_field("path",       STORED | STRING);
schema_builder.add_text_field("commit_oid", STORED | STRING);
schema_builder.add_u64_field ("line_start", STORED);
schema_builder.add_u64_field ("line_end",   STORED);
~~~

### 4. Chunking — `Chunk` 타입

`SearchDocument`를 한 단계 더 분해. tree-sitter 함수 단위 청킹이 핵심 시너지 (gluck에 이미 tree-sitter 인프라가 syntax highlight용으로 존재).

~~~rust
pub enum Chunk {
    /// 커밋 메시지 (모든 커밋, history-wide)
    CommitMessage {
        oid: String,
        title: String,
        body: String,
        author_time: i64,
    },
    /// 파일 전체 (작은 파일, 또는 tree-sitter 미지원 언어)
    WholeFile {
        commit_oid: String,
        path: String,
        content: String,
    },
    /// tree-sitter로 추출한 함수/메서드/클래스 단위
    Symbol {
        commit_oid: String,
        path: String,
        symbol_name: String,
        kind: SymbolKind,           // Function | Method | Struct | Impl | ...
        line_start: u32,
        line_end: u32,
        content: String,
    },
}

impl Chunk {
    pub fn embed_text(&self) -> String { /* ... */ }   // Model2Vec 입력
    pub fn bm25_title(&self) -> &str   { /* ... */ }   // Tantivy title
    pub fn bm25_body(&self)  -> &str   { /* ... */ }   // Tantivy body
}
~~~

청킹 전략:
- **파일 크기 < 4KB** → `WholeFile`
- **tree-sitter 지원 언어 + 파일 크기 ≥ 4KB** → `Symbol` 단위 분해, 최상위 함수/메서드/구조체 추출
- **미지원 언어** → `WholeFile` (단순)
- **이진 파일** → 건너뜀 (기존 `is_binary_blob()` 재사용)

### 5. RRF fusion

표준 RRF, `k=60`, 두 ranked list 모두 `Vec<(u64, f32)>` 타입.

~~~rust
pub fn rrf_fuse(
    bm25: &[(u64, f32)],
    vec:  &[(u64, f32)],
    k: f32,
    limit: usize,
) -> Vec<(u64, f32)> {
    let mut scores: HashMap<u64, f32> = HashMap::new();
    for (rank, (id, _)) in bm25.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vec.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    let mut out: Vec<_> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    out.truncate(limit);
    out
}
~~~

---

## Indexing scope (MVP)

| 대상 | 인덱스 여부 |
|---|---|
| 모든 커밋의 메시지 | ✅ |
| HEAD 시점의 모든 파일 | ✅ |
| 과거 커밋의 파일 blob | ❌ (Phase 2: diff hunk로 대체 검토) |
| Diff hunk | ❌ (Phase 2 후보) |
| 이진/거대 파일 | ❌ |

이 결정은 v1과 동일. semble과 달리 gluck은 **history-aware**: 모든 커밋 메시지를 인덱스함으로써 "언제 이 변경이 일어났는지" 질문에 답할 수 있음. 파일 내용은 HEAD만 인덱스하므로 *과거 시점의 코드 검색*은 Phase 2의 diff hunk 인덱싱으로 부분 해결.

---

## Storage Layout

~~~
.glc-index/
├── meta.toml
├── bm25/                    # Tantivy 인덱스 디렉토리
│   ├── meta.json
│   ├── *.fast
│   ├── *.fieldnorm
│   ├── *.idx
│   ├── *.pos
│   ├── *.term
│   └── ...
├── vectors/
│   └── index.tvim           # turbovec native format
└── model/                   # 선택 — 동봉 또는 별도 cache
    └── potion-multilingual-128M/
        ├── model.safetensors
        ├── tokenizer.json
        └── config.json
~~~

### `meta.toml` schema

~~~toml
version = 1
head_oid = "abc123..."
doc_count = 1843
indexed_at = 2026-05-23T12:34:56Z

[embedding]
model = "minishlab/potion-multilingual-128M"
dim = 256

[bm25]
tokenizer = "ngram_2_2"

[vector]
backend = "turboquant_4bit"
~~~

모델 위치: `model/` 디렉토리를 인덱스 안에 포함하면 portability ↑ (다른 머신에서 재인덱싱 없이 검색 가능). 단 ~110MB 추가 디스크. **MVP에서는 동봉**, 사용자가 `[embedding] cache_path` 설정으로 외부 캐시 디렉토리 지정 가능 (Phase 2).

---

## Doc ID 공간

`u64` 단조 증가 카운터를 indexing 중 유지. 모든 SearchDocument에 부여, Tantivy(`id` stored field as string)와 turbovec(`IdMapIndex` native u64)에 동일하게 기록.

이점:
- BM25 결과와 vector 결과가 같은 키 공간 → RRF가 단순 union
- Tantivy doc_id(segment-local)에 의존하지 않음 → segment merge에 안정
- 향후 incremental indexing 시 stable identifier

---

## TUI Integration

### 단축키
- **`/`** — 기존 plain search 유지 (legacy, simple keyword)
- **`S`** — semantic search 모달 열기 (Shift+s)
- **`Esc`** — 모달 닫기, 이전 mode로 복귀

### Modal layout (통합 — v1의 mode-filtered 폐기)

~~~
┌─ Search ──────────────────────────────────────────────┐
│ > 에러 처리                                            │
├───────────────────────────────────────────────────────┤
│ Commits                                                │
│   • bb20b71  Remove Todo Item                          │
│   • c80b8e9  Apply cargo fmt                           │
│                                                        │
│ Files & Symbols                                        │
│   • src/search/error.rs::handle_io_error  (fn)         │
│   • src/main.rs::main  (fn)                            │
│   • docs/error-handling.md                             │
└───────────────────────────────────────────────────────┘
~~~

- 모달은 현재 mode와 **무관**하게 항상 열림
- 결과는 두 섹션(Commits / Files & Symbols)으로 자동 그룹화
- **`Enter` 시 결과 종류에 따라 mode 자동 전환:**
  - Commit 선택 → Pick mode, 해당 커밋 highlight
  - File/Symbol 선택 → View mode, 해당 커밋 + 해당 파일 + 해당 라인으로 점프
- Diff mode에서도 활성화 — Esc로 복귀

### 쿼리 임베딩 비용

- 모델 로딩 (한 번): ~50ms (mmap)
- 쿼리당 임베딩: ~수십 µs (보통 < 1ms)
- BM25 + vector search: 합쳐서 < 10ms (gluck 규모)
- **총 latency 목표: 첫 검색 100ms, 이후 10ms 이내**

---

## CLI

~~~
glc index [PATH] [--force] [--batch-size N] [--max-file-size BYTES]
~~~

### 동작

- 기본: `.glc-index/`가 있고 `meta.version == 1`이며 `head_oid` 일치 → no-op + 메시지
- `--force` → 기존 인덱스 삭제 후 재구축
- `version` mismatch → 에러 메시지 + `--force` 사용 안내
- `head_oid` mismatch → 자동 재인덱싱 (HEAD가 움직였으므로)

### TUI에서 인덱스 부재 시

`S` 키 눌렀는데 `.glc-index/`가 없으면:

~~~
┌─ No index found ──────────────────────────────────────┐
│                                                        │
│  Run `glc index` to build the search index.            │
│  Estimated time: ~30 seconds for this repo.            │
│                                                        │
│              [ Build now ]   [ Cancel ]                │
└───────────────────────────────────────────────────────┘
~~~

`[Build now]` → TUI 내에서 인덱싱 진행 + progress bar. 외부 CLI 호출 강제하지 않음.

---

## Phase 2 — Out of Scope

본 spec에서 제외, 별도 plan으로 분리:

1. **Diff hunk 인덱싱.** 각 커밋의 변경 hunk를 별도 `Chunk` variant로. "언제 추가됐는지" 질문에 답 가능. semble의 *file coherence* reranking signal과 결합 시 강력.
2. **Incremental indexing.** `turbovec::IdMapIndex::remove`를 활용한 점진적 업데이트. 새 커밋만 인덱싱.
3. **Code-aware reranking** (semble에서 차용 가능한 신호):
   - **Adaptive weighting:** symbol-like 쿼리(`Foo::bar`)는 BM25 비중↑, 자연어 쿼리는 균형
   - **Definition boosts:** 정의 청크 > 참조 청크
   - **Identifier stem matching:** `parse_config` ↔ `parseConfig` ↔ `ConfigParser`
   - **Noise penalties:** 테스트 파일, generated 코드 down-rank
4. **Dual-embedding.** 커밋 메시지는 `potion-multilingual-128M`, 코드 청크는 `potion-code-16M`.
5. **Recency boost.** 커밋 결과에 시간 가중치 (`exp(-λ * age_days)`).
6. **Korean Lindera tokenizer 옵션.** 사용자가 ngram vs 형태소 분석 선택.
7. **Model cache externalization.** 인덱스 외부에 모델 캐시.

---

## Alternatives Considered

### A. ONNX-based (CodeBERT, jina-v3 등)

v1 접근. ONNX Runtime의 native dep이 단일 바이너리 정체성 훼손. fastembed-rs로 정적 링크 가능하나 바이너리 크기 +100MB. **기각**.

### B. `MinishLab/semble` 또는 `johunsang/semble_rs`를 사이드카로

semble은 architecture가 완벽히 일치하지만 **현재 워킹 트리만 인덱싱.** gluck의 history-aware 미션과 불일치 — FFF와 같은 문제. semble을 *reference implementation*으로 참고하되 **dependency로는 채택 안 함**. **기각**.

### C. Tantivy-only, semantic 포기

운영 단순함 최대. 그러나 *"에러 처리"* → `try`/`catch`/`Result` 매칭 같은 paraphrase/synonym 검색 불가. 영택님의 한국어 commit message + 영문 코드 식별자 mismatch 시나리오에서 특히 약함. **검토 끝에 기각**.

### D. ripgrep을 라이브러리로

인덱스 없이 on-the-fly grep. 운영 부담 zero, 그러나 history 코퍼스(수만 commit blob)에 대해 매번 grep은 latency 부담. **기각**.

---

## Design Notes

### v2가 semble의 선택과 일치하는 이유

semble의 NDCG@10 = 0.854는 우리에게 **두 가지 사실**을 알려줍니다:

1. **BM25 + Model2Vec + RRF는 충분히 좋다.** CodeRankEmbed (transformer-based, ~500MB) 대비 99% 품질. 즉 더 무거운 architecture를 추구할 동기가 작음.
2. **컴포넌트 선택의 자유도는 작다.** 같은 문제를 푸는 대부분의 production 시스템이 이 조합으로 수렴 중. (cf. Obsidian MCP guide도 동일 스택: SQLite FTS5 + Model2Vec + sqlite-vec.)

이 spec은 *발명*하지 않습니다. *검증된 패턴을 history 도메인에 적용*합니다.

### v2가 semble과 의도적으로 다른 점

| 측면 | semble / semble_rs | gluck v2 |
|---|---|---|
| 검색 대상 | 워킹 트리 (현재) | git history (과거 커밋 + HEAD) |
| 사용자 | AI 코딩 에이전트 | 인간 개발자 (TUI navigation) |
| 응답 형식 | 청크 텍스트 (token-efficient) | 위치 정보 (커밋/파일/라인) |
| 인덱스 invalidation | 파일 watch (실시간) | HEAD oid 비교 (명시적) |
| 청킹 도구 | Chonkie | gluck 자체 tree-sitter 인프라 재사용 |

특히 마지막 — gluck은 *이미 tree-sitter를 syntax highlight에 쓰고 있으므로 청킹에 재사용 가능*. semble은 자체 청킹 라이브러리(Chonkie) 의존. 이 차이가 의존성 수를 줄임.

### 타입 흐름의 일관성

`Chunk` (sum type) → `(doc_id: u64, embedding: [f32; 256])` → `VectorIndex::add` / `Tantivy::add_document` → `index.tvim` + `bm25/*` 라는 **단방향 데이터 흐름**이 `swift-spinning-whale.md`의 type-driven refactor 원칙과 일관됩니다. 양자화·압축·SIMD scoring은 `VectorIndex` 안에 캡슐화, BM25는 Tantivy 안에 캡슐화, RRF는 두 ranked list 합치는 작은 함수.

검색 quality나 ranking strategy를 바꿀 때 영향 범위가 좁습니다:
- 새 reranking signal 추가 → RRF 이후 단계에 끼움, 다른 곳 무영향
- Embedding 모델 교체 → `model_name` 변경 + reindex, 다른 곳 무영향
- Quantization 비트 폭 변경 → `VectorIndex::BIT_WIDTH` 상수, 다른 곳 무영향
- BM25 tokenizer 교체 → Tantivy schema 변경, vector path 무영향

---

## Open Questions (구현 전 확정 필요)

1. **모델 파일 위치:** `.glc-index/model/` 동봉 vs `$XDG_CACHE_HOME/glc/models/` 공유? MVP는 동봉으로 가지만, 여러 repo를 자주 인덱싱하면 디스크 낭비.
2. **Ngram 크기:** `(2,2)` 고정 vs `(2,3)` 혼합? 한국어 기준 2가 표준, 3은 recall ↑ index size ↑.
3. **Symbol 최소 크기:** 5줄 미만 함수도 청크화할지? semble은 합쳐서 처리.
4. **`/` plain search 유지 여부:** legacy로 남길 것인지 deprecate할 것인지.
5. **Index 위치 default:** `.glc-index/` (repo root, gitignore 필요) vs `$XDG_CACHE_HOME/glc/<repo-hash>/` (글로벌)?
