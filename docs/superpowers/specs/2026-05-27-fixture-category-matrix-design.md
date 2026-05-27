# Fixture 카테고리 매트릭스 설계 (`glc report` 평가 표본 확장)

- 작성: 2026-05-27
- 범위: `tests/fixtures/search_queries.toml` + `src/search/report/` 모듈
- 출처 동기: `검색 품질 리포트 학습 가이드.md` 섹션 5의 "다음 단계로 좋은 작업" #1 (fixture 다양성 매트릭스 설계)

## 1. 배경

현행 fixture는 7개의 positive query로만 구성되어 있어 두 가지 한계가 있다.

1. **표본 부족**: 1개 쿼리가 메트릭의 1/7(약 14%)을 차지 → 베이스라인 비교 시 잡음이 크다.
2. **카테고리 편향**: 영문 식별자 + 영문 자연어 위주. 한국어/오타/패러프레이즈/negative(무관 쿼리)는 측정 불가.

목표는 단순한 양적 확장이 아니라 **카테고리 커버리지** 확보다. "검색이 어디서 잘 되고 어디서 못 되는가"를 카테고리별로 진단할 수 있어야 한다.

## 2. 핵심 결정

| # | 결정 | 비고 |
|---|------|------|
| D1 | 총 30개 쿼리, 카테고리별 가중 분포 (exact 8 / natural 6 / korean 4 / typo 4 / paraphrase 4 / negative 4) | 사용 빈도 가중 |
| D2 | Negative query는 **블랙리스트 방식**으로 평가 (특정 path/path_prefix가 top-10에 등장하면 위반) | RRF score 절대치 비교 불안정성 회피 |
| D3 | `[[query]]` 배열 단일 유지. `category = "negative"`로 negative 디스패치 | 스키마 단순화 |
| D4 | 전체 aggregate는 positive 26개만 평균 (기존 의미 보존) | 카테고리/negative는 별도 섹션 |
| D5 | 카테고리는 fixture 필수 필드. 기존 7개 fixture는 동시 마이그레이션 | backward-compat 불필요 (gluck 자체 회귀용 한정) |

## 3. Fixture 스키마

### 3.1 TOML 형식

```toml
# Positive
[[query]]
category = "exact_identifier"  # 신규 필수 필드
text = "tantivy delete_term"
expected = [
    { path = "src/search/bm25.rs", kind = "Symbol", title = "delete_doc" },
]

# Negative (블랙리스트)
[[query]]
category = "negative"
text = "react component lifecycle"
forbidden = [
    { path_prefix = "src/" },
]
```

### 3.2 카테고리 enum

6개의 닫힌 집합. 새 카테고리 추가는 spec 변경 동반.

| Category | 코드 식별자 | 목적 |
|----------|------------|------|
| `exact_identifier` | `ExactIdentifier` | 함수/타입/식별자 직접 검색. BM25 강점 검증. |
| `natural_language` | `NaturalLanguage` | 영문 자연어 "어떻게 X를 하나" 형태. Vector 강점 검증. |
| `korean` | `Korean` | 한국어 자연어. ngram_2_2 + 다국어 임베딩의 한국어 처리 검증. |
| `typo` | `Typo` | 1~2글자 오타. ngram 부분매칭 견고성 검증. |
| `paraphrase` | `Paraphrase` | 동의어/패러프레이즈. Vector 의미 매칭 검증. |
| `negative` | `Negative` | 도메인 외 쿼리. 블랙리스트로 위반 잡기. |

### 3.3 `forbidden` 규칙

```toml
forbidden = [
    { path = "src/main.rs" },         # 정확 일치
    { path_prefix = "src/search/" },  # 접두 일치
]
```

`path`와 `path_prefix`는 상호 배타. 한 항목에 하나만 지정. 둘 다 누락 또는 둘 다 지정 시 로드 에러.

### 3.4 Loader 검증 규칙

`src/search/report/fixtures.rs::load()`:

- `category`는 enum 파싱 실패 시 `Toml` 에러.
- `category != Negative` → `expected` 비어있지 않아야 함. `forbidden` 필드가 있으면 거부 (fixture 작성 실수 조기 발견).
- `category == Negative` → `forbidden` 비어있지 않아야 함. `expected` 필드가 있으면 거부 (negative는 정답이 없는 것이 정답).
- `forbidden` 항목 각각: `path` XOR `path_prefix` 정확히 하나.
- `path_prefix` 매칭 시맨틱: `hit.path.starts_with(prefix)` (대소문자 구분, 단순 문자열 접두 일치. glob/regex 아님).

신규 에러 variant:
```rust
ReportError::InvalidNegativeQuery { index: usize, reason: String }
ReportError::InvalidForbiddenRule { query_index: usize, rule_index: usize, reason: String }
```

## 4. 평가 (`metrics.rs`)

### 4.1 `QueryEval` enum

```rust
pub enum QueryEval {
    Positive(PositiveEval),
    Negative(NegativeEval),
}

pub struct PositiveEval {
    pub query: String,
    pub category: Category,
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
    pub first_hit_rank: Option<usize>,
    pub hit_paths: Vec<String>,
}

pub struct NegativeEval {
    pub query: String,
    pub passed: bool,
    pub violations: Vec<NegativeViolation>,
}

pub struct NegativeViolation {
    pub rank: usize,        // 1-indexed, top-10 내
    pub path: String,
    pub matched_rule: String,  // 표시용: "path_prefix=src/" 또는 "path=src/main.rs"
}
```

### 4.2 평가 함수

- `evaluate_positive(query: &FixtureQuery, results: &[SearchResult]) -> PositiveEval` — 기존 `evaluate()` 로직 유지 + `category` 필드 채움.
- `evaluate_negative(query: &FixtureQuery, results: &[SearchResult]) -> NegativeEval` — top-10을 순회하며 각 hit을 `forbidden` 규칙들과 매칭. 매칭되면 `NegativeViolation` 추가. `passed = violations.is_empty()`.
- `evaluate(query: &FixtureQuery, results: &[SearchResult]) -> QueryEval` — `category`로 디스패치.

### 4.3 Aggregate

```rust
pub struct AggregateEval {        // 전체 (positive only)
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
    pub n_queries: usize,
}

pub struct CategoryAggregate {    // 카테고리별 sub-aggregate
    pub category: Category,
    pub n: usize,
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
}

pub struct NegativeAggregate {
    pub n: usize,
    pub pass_rate: f32,
}
```

- `aggregate(per_query: &[QueryEval]) -> AggregateEval` — `Positive`만 필터해서 평균.
- `aggregate_by_category(per_query: &[QueryEval]) -> Vec<CategoryAggregate>` — 카테고리별 group, **Negative 제외**, 결과 순서는 `exact_identifier → natural_language → korean → typo → paraphrase` 고정 (diff 안정).
- `aggregate_negatives(per_query: &[QueryEval]) -> NegativeAggregate`.

## 5. 리포트 출력 (`render.rs`)

### 5.1 Markdown 구조

```markdown
## Aggregate (positive only, n=26)
| MRR | R@5 | R@10 | NDCG@10 |
|-----|-----|------|---------|
| 0.750 | 0.929 | 1.000 | 0.784 |

## By Category
| Category | n | MRR | R@5 | R@10 | NDCG@10 |
|----------|---|-----|-----|------|---------|
| exact_identifier | 8 | ... |
| natural_language | 6 | ... |
| korean           | 4 | ... |
| typo             | 4 | ... |
| paraphrase       | 4 | ... |

## Negative Queries (n=4, pass rate 75.0%)
| # | Query | Result |
|---|-------|--------|
| 1 | react component lifecycle | PASS |
| 2 | django migrations | FAIL (rank 3: src/search/indexer.rs, matched_rule=path_prefix=src/) |
| ... |

## Per-Query (positive only)
| # | Cat | Query | Hit | MRR | R@5 | R@10 | NDCG |
|---|-----|-------|-----|-----|-----|------|------|
| 1 | exact | tantivy delete_term | 1 | 1.000 | ... |
```

규칙:
- `## Aggregate` 헤더에 `n=` 명시 → baseline 비교 시 표본 변화 즉시 감지.
- `## By Category` 행 순서 고정.
- `## Negative Queries`는 PASS는 한 줄, FAIL은 위반 정보 inline. 위반이 여러 건이면 줄바꿈 후 추가 행.
- `## Per-Query`에 Category 컬럼(약어 `exact`/`natural`/`korean`/`typo`/`paraphrase`) 추가.

### 5.2 stdout 출력

`to_stdout()`도 동일 정보를 텍스트 표로 출력. 기존 스타일(고정폭 표) 유지.

## 6. Fixture 30개 작성 가이드라인

| 카테고리 | n | 작성 원칙 |
|----------|---|----------|
| exact_identifier | 8 | 함수/타입/식별자 이름. Symbol 정답 위주. 현행 7개 중 해당 항목 흡수. |
| natural_language | 6 | 영문 자연어 "how to X" 형태. 일부 현행 항목 흡수. |
| korean | 4 | 한국어 자연어 쿼리 신규 작성. |
| typo | 4 | 1~2글자 오타. 원본 쿼리와 같은 정답 path. |
| paraphrase | 4 | 동의어/패러프레이즈 신규. |
| negative | 4 | 서로 다른 외부 도메인 (편향 방지). 블랙리스트로 `src/` 차단. |

**작성 시 원칙**:
- 정답 path는 `git ls-tree HEAD`로 존재 확인 (`modal_state.rs` 재발 방지).
- Symbol 정답은 tree-sitter가 추출하는 구문 한정 (Rust는 `function_item`만 Symbol).
- 구체 쿼리/정답 라벨링은 implementation phase에서 결정.

## 7. 변경 파일

| 파일 | 변경 |
|------|------|
| `tests/fixtures/search_queries.toml` | 30개로 확장, 6개 카테고리 모두 포함 |
| `src/search/report/fixtures.rs` | `Category` enum, `category` 필수 필드, `ForbiddenRule`, negative 디스패치 검증 |
| `src/search/report/metrics.rs` | `QueryEval` enum, `evaluate_negative()`, `CategoryAggregate`, `NegativeAggregate` |
| `src/search/report/mod.rs` | `Report`에 `by_category`, `negatives` 필드. `run()`에서 신규 aggregate 호출 |
| `src/search/report/render.rs` | 3개 신규 섹션, 헤더 `n=` 표시, per-query Cat 컬럼 |

## 8. 테스트 전략

### 8.1 단위 테스트

`fixtures.rs::tests`:
- 카테고리 enum 파싱 (모든 6개).
- positive에 `forbidden` 있으면 거부.
- negative에 `expected` 있으면 거부.
- `forbidden`에 `path`와 `path_prefix` 동시 지정 시 거부, 둘 다 누락 시 거부.
- 기존 `loads_project_fixtures`를 `>=30` 및 6개 카테고리 모두 1개 이상 존재 검증으로 강화.

`metrics.rs::tests`:
- `evaluate_negative()` PASS 케이스 (위반 없음).
- `evaluate_negative()` FAIL 케이스 — `path` 정확 매칭, `path_prefix` 매칭 각각.
- `aggregate()`가 Negative variant를 제외함을 검증.
- `aggregate_by_category()`가 카테고리별로 정확히 group되고 Negative 제외함을 검증.
- 카테고리 순서가 고정(`ExactIdentifier` → … → `Paraphrase`)임을 검증.

### 8.2 E2E

`report::e2e_tests::report_runs_end_to_end_and_writes_markdown`:
- fixture에 positive + negative 각각 1개 이상 포함하도록 갱신.
- markdown 출력에 `## By Category`, `## Negative Queries` 헤더 포함 검증.
- `## Aggregate (positive only, n=…)` 헤더 포맷 검증.

### 8.3 수동 검증

- `cargo run -- index` 후 `cargo run -- report --out /tmp/r.md`로 실제 출력 확인.
- 각 negative 4개가 PASS인지, FAIL이면 어떤 hit이 위반했는지 사람이 봐서 합리적인지 검토.

## 9. 범위 밖 (다음 사이클)

`검색 품질 리포트 학습 가이드.md` 섹션 5의 항목 중 이번 spec 범위는 #1만. 나머지는 별도 cycle:

- #2 Q3 "RRF" 약점 — BM25/Vector raw 점수 로깅
- #3 인덱스 빌드 메트릭 (build time / memory / disk)
- #4 임계값 기반 회귀 alert
- #5 임베딩 모델 A/B

리포트 학습 가이드 문서 자체 업데이트도 이번 spec 범위 외 — 새 형식의 실제 리포트가 1회 생성된 후 별도로 수행.

## 10. 마이그레이션 노트

기존 7개 fixture는 `category` 필드가 없어 신규 로더로 로드 실패한다. 다음을 같은 PR에 묶는다:

1. 신규 로더/메트릭/렌더 코드
2. fixture를 30개로 동시 교체

부분 적용 상태가 발생하지 않게 한다.
