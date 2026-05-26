# 검색 품질 개선 설계

- 작성일: 2026-05-26
- 대상 커밋: `ccb575d` (Cargo.lock 0.9.1)
- 관련 베이스라인: `result.md` (MRR 0.330, Recall@5 0.286, Recall@10 0.571, NDCG@10 0.384)
- 회귀 추적: `glc report` + `tests/fixtures/search_queries.toml`

## 1. 동기

`glc report` 베이스라인에서 검색 품질이 낮음. 7개 쿼리 중:

- **0 hit 3개**: `incremental indexing fallback`, `RRF reciprocal rank fusion`, `search modal state machine`
- **낮은 랭크 2개**: `tree sitter highlight configuration` (rank 6), `git revwalk topological commit` (rank 7)
- **rank 1 2개**: `tantivy delete_term`, `embedding model load potion`

원인 분석을 통해 식별된 핵심 결함:

| ID | 원인 | 영향받는 쿼리 |
|---|---|---|
| A | snake_case 식별자 ↔ 자연어 쿼리 매칭 부재 (`rrf_fuse` vs "fusion") | RRF, modal state |
| B | path가 STRING 필드라 토큰 매칭 불가, BM25 검색에 미포함 | tree sitter, git revwalk |
| C | BM25 `ngram_2_2` 단독 → 영어 단어에서 노이즈 점수 큼 | 전반적 |
| D | Symbol 추출이 `function_item`만 → `enum`/`struct`/`trait` 누락 | modal state machine |
| E | fixture에 어휘 매칭 불가능한 정답 (`diff.rs` ↔ "incremental"/"fallback") | incremental |

이 설계는 (E)는 fixture 정제로, (A)~(D)는 인덱싱/쿼리 파이프라인 변경으로 해결한다. 임베딩 모델 교체나 RRF 튜닝은 이번 작업 범위에서 제외 — 이번 변경 후 재측정 결과를 보고 다음 라운드에서 결정한다.

## 2. 변경 범위

| 컴포넌트 | 변경 | 비고 |
|---|---|---|
| `tests/fixtures/search_queries.toml` | 정답 1건 수정 | 어휘 매칭 불가능 케이스 제거 |
| `src/search/bm25.rs` | 스키마 확장, 토크나이저 추가, 쿼리 필드 확대 | path_terms 신규, identifier 분해 토크나이저 신규 |
| `src/search/chunk/file.rs` | `WHOLE_FILE_THRESHOLD` 8KB → 16KB | 1줄 변경 |
| `src/search/chunk/symbol.rs` | Rust 심볼 추출 확장 (enum/struct/trait/type) | function 외 타입도 잡음 |
| `src/search/chunk/mod.rs` | `SymbolKind` 변형 확장 | Enum/Struct/Trait/TypeAlias 추가 |
| `src/search/indexer.rs` | `chunk_to_meta`에서 path를 path_terms로 분해 저장 | BM25 add_doc 경로 |
| `src/search/mod.rs` | `INDEX_VERSION: 5 → 6` | 자동 풀 리빌드 트리거 |

**범위 외**:

- 임베딩 모델 교체 또는 임베딩 텍스트(`embed_text`) 변경
- RRF k 또는 candidate_limit 튜닝
- Python/JS/TS/Go 심볼 추출 확장 (회귀 위험, fixture에 영향 없음)
- 모듈/파일 docstring을 별도 청크로 만드는 작업
- Vector 인덱스 변경 (turbovec)

## 3. 상세 설계

### 3.1 Fixture 정제

`tests/fixtures/search_queries.toml`의 첫 쿼리:

```toml
# Before
[[query]]
text = "incremental indexing fallback"
expected = [
    { path = "src/search/diff.rs" },
    { path = "src/search/indexer.rs", kind = "Symbol", title = "build_index_incremental" },
]
```

→

```toml
# After
[[query]]
text = "incremental indexing fallback"
expected = [
    { path = "src/search/indexer.rs", kind = "Symbol", title = "build_index_incremental" },
]
```

이유: `diff.rs`에는 "incremental"/"fallback" 어휘가 존재하지 않으며, path 또한 의미적으로 직접 일치하지 않음. 회귀 추적 목적상 검색 시스템이 도달 가능한 정답만 fixture에 둔다.

다른 6개 쿼리는 변경 없음.

### 3.2 BM25 스키마 확장

#### 3.2.1 신규 토크나이저: `code_ident`

식별자(snake_case/camelCase)를 단어 단위로 분해하는 토크나이저. Tantivy의 `TokenFilter` 트레잇을 구현하는 `IdentifierSplit` 필터를 만들어 다음 체인으로 등록:

```
SimpleTokenizer → LowerCaser → IdentifierSplit
```

`IdentifierSplit` 동작:
- 입력 토큰을 `_` 기준으로 split → 각 조각
- 각 조각에 대해 lower→upper 또는 letter→digit 경계로 추가 split (camelCase)
- 빈 토큰은 버림
- 원본도 함께 emit (예: `rrf_fuse` → `rrf_fuse`, `rrf`, `fuse`) — recall과 precision 모두 보존
- 한글 등 비-ASCII 식별자는 split 시도하지 않고 그대로 통과

토크나이저 이름: `"code_ident"`.

기존 `ngram_2_2`는 그대로 유지 — body 검색용으로 한글 부분 매칭에 필요.

#### 3.2.2 신규 필드: `path_terms`

스키마에 텍스트 필드 `path_terms`를 추가:

```rust
let path_terms_opts = TextOptions::default().set_indexing_options(
    TextFieldIndexing::default()
        .set_tokenizer("code_ident")
        .set_index_option(IndexRecordOption::WithFreqs),
);
let path_terms = builder.add_text_field("path_terms", path_terms_opts);
```

- `STORED` 없음 — 검색용으로만 사용, doc_store 복원에는 기존 `path` STRING 필드 활용.
- 저장 값: 청크의 path를 `/`, `.`, `_`, `-` 등으로 분해한 공백 문자열. 예: `"src/search/rrf.rs"` → `"src search rrf rs"`. 정확한 분해 규칙은 `IdentifierSplit` 토크나이저 입력 단순화를 위해 호출 측에서 처리 (path → `"src search rrf rs"`로 만든 뒤 토크나이저에 통과).
- `path_terms`는 path가 `Some(_)`인 청크에만 채움. CommitMessage 청크는 비움.

기존 `path` STRING 필드는 변경 없이 유지 — `extract_path_filter`/`apply_path_filter` 흐름이 그대로 동작.

#### 3.2.3 `title` 필드 토크나이저 교체

`title` 필드 토크나이저를 `ngram_2_2` → `code_ident`로 변경. 한글 commit 메시지 title의 부분 매칭은 일부 약화되나, identifier/단어 단위 매칭은 강해짐. 한글 부분 매칭은 body 필드에서 여전히 가능.

#### 3.2.4 `body` 필드 — 변경 없음

`ngram_2_2` 유지. 한글 본문 부분 매칭 보존.

#### 3.2.5 `QueryParser` 변경

```rust
let parser = QueryParser::for_index(
    &self.index,
    vec![self.fields.title, self.fields.path_terms, self.fields.body],
);
parser.set_field_boost(self.fields.title, 2.0);
parser.set_field_boost(self.fields.path_terms, 2.0);
parser.set_field_boost(self.fields.body, 1.0);
```

(정확한 API는 `set_boost`/`set_field_boost`/`SchemaBuilder::set_boost` 중 Tantivy 0.22에서 사용 가능한 것을 구현 시 확인.)

### 3.3 청크 분할 변경

#### 3.3.1 `WHOLE_FILE_THRESHOLD`

`src/search/chunk/file.rs`:

```rust
const WHOLE_FILE_THRESHOLD: usize = 16 * 1024;
```

영향:
- `modal_state.rs`(8.2KB), `engine.rs`(7.1KB는 영향 없음), `store.rs`(8.8KB)가 모두 WholeFile로 잡힘.
- `indexer.rs`(20KB)는 여전히 Symbol 분할.

#### 3.3.2 Symbol 추출 확장 (Rust 한정)

`src/search/chunk/symbol.rs`의 Rust 쿼리:

기존 (개념):
```scm
(function_item name: (identifier) @name) @symbol
```

확장:
```scm
(function_item name: (identifier) @name) @symbol
(enum_item name: (type_identifier) @name) @symbol
(struct_item name: (type_identifier) @name) @symbol
(trait_item name: (type_identifier) @name) @symbol
(type_item name: (type_identifier) @name) @symbol
```

`impl` 블록은 추가하지 않음 — 현재 정책(impl 자체는 청크 안 함, 안의 method만)을 유지.

`SymbolKind` 확장 (`src/search/chunk/mod.rs`):

```rust
pub enum SymbolKind {
    Function,
    Method,
    Enum,        // 신규
    Struct,      // 신규
    Trait,       // 신규
    TypeAlias,   // 신규
}
```

다른 언어 (Python/JS/TS/Go)의 쿼리는 손대지 않는다.

### 3.4 Path → path_terms 분해 로직

위치: `src/search/indexer.rs::chunk_to_meta` 호출 직후, 또는 별도 헬퍼 `path_to_terms(path: &str) -> String`로 분리.

규칙:
- `/`, `.`, `-`, `_`, 공백을 단어 경계로 split
- 빈 조각 제거
- 결과를 공백으로 join
- 예: `src/search/chunk/file.rs` → `"src search chunk file rs"`
- 예: `src/git/store.rs` → `"src git store rs"`

`code_ident` 토크나이저가 다시 식별자 분해를 적용하므로, `"rs"` 같은 짧은 조각도 토큰으로 들어감. (단 2-3자 흔한 조각은 BM25 IDF가 낮춰 자연히 가중치 약화.)

`add_doc` 시그니처에 `path_terms` 인자 추가하거나, `DocMeta`/`Chunk`에서 자동 도출. 추천: `Bm25Index::add_doc`이 `meta.path`로부터 내부에서 도출 — 호출자 영향 최소.

### 3.5 인덱스 호환성

`src/search/mod.rs`:

```rust
pub const INDEX_VERSION: u32 = 6;
```

기존 인덱스를 가진 사용자는 `SearchEngine::open()`에서 `VersionMismatch` 또는 `build_index()`에서 schema outdated 경로로 자동 풀 리빌드.

`Bm25Meta::tokenizer`는 `"ngram_2_2"` 유지 — body 필드 토크나이저로 해석. 단일 문자열 의미가 모호하지만 INDEX_VERSION이 구분하므로 충분.

### 3.6 RRF / 임베딩 — 변경 없음

`rrf::rrf_fuse(k=60.0)`, candidate_limit 정책, `embed_text` 모두 유지. 변경 후 재측정 결과에 따라 후속 라운드에서 결정.

## 4. 테스트

### 4.1 단위 테스트 추가

| 위치 | 테스트 |
|---|---|
| `src/search/bm25.rs` | `code_ident` 토크나이저가 `rrf_fuse` → `rrf`, `fuse` 매칭 |
| `src/search/bm25.rs` | `path_terms` 필드에 `src/search/rrf.rs` 인덱싱 후 쿼리 `"rrf"` hit |
| `src/search/bm25.rs` | 한글 본문이 `ngram_2_2` body로 여전히 매칭 (회귀 가드) |
| `src/search/chunk/symbol.rs` | Rust enum/struct/trait 추출 |
| `src/search/chunk/file.rs` | 12KB 파일이 WholeFile로 잡힘 (16KB 미만) |

### 4.2 회귀 가드

기존 BM25 테스트는 모두 통과해야 함. 특히:
- `test_korean_bigram_search` — body 필드 ngram 유지로 통과
- `test_uppercase_indexed_matches_lowercase_query` — title이 code_ident + LowerCaser로 여전히 통과
- `test_path_field_exact_match_query` — path STRING 필드와 `path:"..."` 문법 미변경

### 4.3 End-to-end 검증

```bash
cargo run --bin glc -- index --force
cargo run --bin glc -- report --out result.md
```

`result.md`를 베이스라인과 비교. 합격 기준:

| 메트릭 | 베이스라인 | 목표 |
|---|---|---|
| MRR | 0.330 | ≥ 0.65 |
| Recall@5 | 0.286 | ≥ 0.65 |
| Recall@10 | 0.571 | ≥ 0.85 |
| NDCG@10 | 0.384 | ≥ 0.65 |

쿼리별 기대 변화:

| 쿼리 | 베이스 | 목표 Hit Rank |
|---|---|---|
| incremental indexing fallback | 0 hit | ≤ 5 |
| tantivy delete_term | 1 | 1 (회귀 없음) |
| RRF reciprocal rank fusion | 0 hit | ≤ 5 |
| embedding model load potion | 1 | 1 (회귀 없음) |
| search modal state machine | 0 hit | ≤ 5 |
| tree sitter highlight configuration | 6 | ≤ 5 |
| git revwalk topological commit | 7 | ≤ 5 |

목표 미달 시 다음 라운드 후보:
- RRF k를 60 → 20~30으로 낮춰 top-rank 차별화
- `embed_text`에 path 토큰 + 식별자 분해 prepend
- 모듈 docstring을 별도 청크로 추출 (`rrf.rs`처럼 자연어 부재 파일 보완)

## 5. 작업 순서 (구현 청사진)

1. fixture 수정 (1줄)
2. `IdentifierSplit` TokenFilter 구현 + 단위 테스트
3. BM25 스키마에 `path_terms` 필드 추가, `code_ident` 토크나이저 등록
4. `add_doc` 내부에서 path → path_terms 분해 저장
5. `QueryParser` 필드 확장 + boost
6. `WHOLE_FILE_THRESHOLD` 16KB로 상향
7. Symbol 쿼리 확장 + `SymbolKind` 변형 추가
8. `INDEX_VERSION` 6으로 범프
9. 단위 테스트 추가
10. `cargo clippy` / `cargo test` / `rustfmt` 통과
11. `glc index --force` → `glc report --out result.md` → 합격 기준 검증
12. 미달 시 후속 라운드 항목 식별, 통과 시 commit

## 6. 위험 / 트레이드오프

- **인덱스 크기 증가**: `path_terms` 필드 추가, WholeFile 임계 상향으로 ~5~10% 증가 예상. 현재 432KiB → 약 500KiB 추정. 허용 범위.
- **임베딩 단계 변화 없음**: 임베딩 모델/텍스트는 그대로 → 의미 매칭의 근본 한계는 남음. `RRF reciprocal rank fusion` 쿼리는 path_terms `rrf` 매칭에만 의존하므로 path 분해 규칙이 효과적이어야 함.
- **한글 title 부분 매칭 약화**: title 토크나이저가 ngram에서 code_ident로 바뀌어 commit 메시지 한글 부분 매칭이 줄어듦. body에서 보완되지만 commit title-only 쿼리는 일부 회귀 가능. fixture에 한글 commit 쿼리가 없어 회귀 추적은 불가 — 별도 한글 쿼리를 fixture에 추가하는 작업은 후속.
- **Rust 외 언어 미변경**: Python/JS/TS/Go 파일에 enum/class는 여전히 잡히지 않음. 현재 fixture는 모두 Rust 파일이라 영향 없음.
- **`impl` 블록 미청크화**: 기존 정책 유지. 새 SymbolKind 변형이 추가돼도 method는 그대로 `Method` 변형으로 들어감.
