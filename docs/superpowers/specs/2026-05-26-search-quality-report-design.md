# `glc report` — 검색 품질·성능 리포트 설계

**Status:** Draft
**Date:** 2026-05-26
**Scope:** gluck 자체 회귀 추적용 CLI 서브명령. `tests/fixtures/search_queries.toml` 픽스처를 기반으로 검색 품질(MRR/Recall/NDCG)과 성능(p50/p95/p99/QPS) 리포트를 stdout과 (옵션) markdown 파일로 생성한다.

**선행 작업:** [2026-05-26 incremental-indexing plan](../plans/2026-05-26-incremental-indexing.md) "Out of Scope #1" 항목의 후속.

---

## Motivation

BM25 토크나이저, 임베딩 모델, RRF 가중치, chunk 분할 규칙 같은 변경이 검색 품질에 미치는 영향을 사람이 매번 인덱싱하고 `s` 키로 확인하는 건 비현실적이다. 미리 정의된 쿼리/정답 쌍에 대해 MRR/Recall/NDCG를 계산해 한 화면에 보여주는 명령이 필요하다. 부수적으로 같은 쿼리셋을 반복 실행해 latency 분포도 함께 수집한다.

이 명령은 **gluck 자신을 dogfooding**하는 회귀 도구다. 다른 레포에서 실행해도 동작은 하지만 의미 있는 정답이 없으므로 일반 사용자용은 아니다.

---

## Non-goals

- 사용자 레포별 fixture 자동 탐색·생성 (gluck 외 레포는 지원 범위 밖)
- 시간 추이 추적 / 베이스라인 비교 (이번 단계에서는 한 번의 리포트 생성만)
- 자동 PR 코멘트, CI artifact 업로드 (필요해지면 별도)
- Graded relevance 등급 (정답은 binary)
- BM25/Vector 단계별 latency 분해

---

## CLI

```
glc report [--fixtures PATH] [--out FILE.md] [--warmup N] [--iters N] [--limit N]
```

| 옵션 | 기본값 | 설명 |
|------|--------|------|
| `--fixtures` | `tests/fixtures/search_queries.toml` | 쿼리/정답 파일 경로 |
| `--out` | (없음) | markdown 출력 파일. 미지정 시 stdout만 |
| `--warmup` | `3` | 측정 전 각 쿼리 워밍업 횟수 |
| `--iters` | `10` | 각 쿼리 측정 반복 횟수 |
| `--limit` | `10` | `SearchEngine::search`의 top-k. NDCG@10/Recall@10 계산에 필요 |

표준 출력은 항상 사람이 읽기 좋은 표 (`comfy-table`). `--out` 지정 시 같은 내용을 markdown 표로 저장.

---

## Architecture

### 모듈 배치

```
src/search/report/
├── mod.rs       # run(repo, opts) 진입점, ReportOptions
├── fixtures.rs  # FixtureSet/FixtureQuery/ExpectedHit + load()
├── metrics.rs   # 매칭 규칙 + per-query/aggregate 메트릭 계산 (순수 함수)
├── perf.rs      # warmup+측정 루프, LatencyStats
└── render.rs    # stdout(comfy-table) + markdown 직렬화
```

`run()` 흐름:

1. fixture 로드 (실패 즉시 종료)
2. `SearchEngine::open(.glc-index)` (실패 시 "Run `glc index` first")
3. 현재 working tree HEAD oid를 `engine.IndexMeta.head_oid`와 비교 → 불일치 시 경고 1줄 기록
4. `perf::run_perf(engine, queries, warmup, iters, limit)` → `(LatencyStats, last_iter_results)`
5. last_iter_results로 `metrics::evaluate` → per-query, `metrics::aggregate` → aggregate
6. 인덱스 정적 정보 수집 (디스크 크기, doc_count by kind)
7. `render::to_stdout(report)` + `--out` 지정 시 `render::to_markdown_file(report, path)`

신규 파일만 5개. 기존 코드 변경은:

- `src/cli.rs`: `Commands::Report { fixtures, out, warmup, iters, limit }` 추가
- `src/main.rs`: `Report` 분기 → `gluck::search::report::run(&repo, &opts)`
- `src/search/mod.rs`: `pub mod report;` 등록
- `Cargo.toml`: `comfy-table`, `humansize` 의존성 추가

신규 fixture 파일:

- `tests/fixtures/search_queries.toml`: gluck 레포에 대해 손으로 작성한 5~10개 쿼리/정답 쌍

---

## Fixture 포맷

```toml
# tests/fixtures/search_queries.toml

[[query]]
text = "incremental indexing fallback"
expected = [
    { path = "src/search/diff.rs" },
    { path = "src/search/indexer.rs", kind = "Symbol", title = "build_index_incremental" },
]

[[query]]
text = "tantivy delete_term"
expected = [
    { path = "src/search/bm25.rs", kind = "Symbol", title = "delete_doc" },
]
```

타입:

```rust
// src/search/report/fixtures.rs
#[derive(Debug, serde::Deserialize)]
pub struct FixtureSet {
    #[serde(rename = "query")]
    pub queries: Vec<FixtureQuery>,
}

#[derive(Debug, serde::Deserialize)]
pub struct FixtureQuery {
    pub text: String,
    pub expected: Vec<ExpectedHit>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ExpectedHit {
    pub path: String,
    pub kind: Option<DocKind>,      // None: path만 일치하면 hit
    pub title: Option<String>,      // Symbol 정밀 매칭 용
}

pub fn load(path: &Path) -> Result<FixtureSet, ReportError>;
```

`DocKind`는 `src/search/mod.rs`의 기존 enum을 그대로 재사용. `serde::Deserialize`는 이미 derive되어 있음.

---

## 메트릭

### 매칭 규칙

```rust
fn matches(expected: &ExpectedHit, hit: &SearchResult) -> bool {
    if hit.meta.path.as_deref() != Some(&expected.path) { return false; }
    if let Some(k) = &expected.kind {
        if &hit.meta.kind != k { return false; }
    }
    if let Some(t) = &expected.title {
        // DocMeta.title 은 Symbol일 때 "{symbol_name} ({path})", 그 외엔 그대로
        let name = hit.meta.title.split_once(" (")
            .map(|(s, _)| s)
            .unwrap_or(&hit.meta.title);
        if name != t { return false; }
    }
    true
}
```

### Per-query 메트릭

`results: &[SearchResult]` (top-`limit`, RRF 후 정렬됨), `expected: &[ExpectedHit]`에 대해:

- **first_hit_rank**: `results` 안에서 어떤 expected와도 매칭되는 첫 인덱스+1. 없으면 `None`.
- **MRR**: `1.0 / first_hit_rank` (없으면 0). 빈 expected는 0이 아니라 1.0으로 약속(아래 엣지케이스 참고).
- **Recall@k**: `expected` 중 top-k 안에 등장한 비율 (`hit_count_in_top_k / |expected|`). |expected|=0 이면 1.0.
- **NDCG@k**: binary relevance.
  - `rel_i = 1 if results[i-1]이 어떤 expected와 매칭, else 0`
  - `DCG@k = Σ_{i=1..=k} rel_i / log2(i+1)`
  - `IDCG@k = Σ_{i=1..=min(k, |expected|)} 1 / log2(i+1)`
  - `NDCG@k = DCG / IDCG` (IDCG=0 시 1.0)
- 같은 path의 여러 chunk가 매칭되어도 **첫 하나만 카운트**. 중복 점수는 무시.

```rust
// src/search/report/metrics.rs
pub struct QueryEval {
    pub query: String,
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
    pub first_hit_rank: Option<usize>,
    pub hit_paths: Vec<String>,
}

pub struct AggregateEval {
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
    pub n_queries: usize,
}

pub fn evaluate(query: &FixtureQuery, results: &[SearchResult]) -> QueryEval;
pub fn aggregate(per_query: &[QueryEval]) -> AggregateEval;
```

`metrics.rs`는 `SearchEngine`에 의존하지 않고 `SearchResult`만 받으므로 단위 테스트 용이.

---

## 성능 측정

```rust
// src/search/report/perf.rs
pub struct LatencyStats {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub mean_ms: f64,
    pub qps: f64,
}

pub fn run_perf(
    engine: &SearchEngine,
    queries: &[FixtureQuery],
    warmup: usize,
    iters: usize,
    limit: usize,
) -> (LatencyStats, Vec<Vec<SearchResult>>);
```

흐름:
1. warmup회 각 쿼리 검색 (타이밍 버림)
2. iters회 반복하며 각 쿼리 latency 기록
3. 마지막 iter의 결과(쿼리당 1개)를 반환 — 품질 평가에 사용
4. 모든 latency 샘플로 통계 산출

`iters=10`이면 `len(latencies) = 10 × n_queries`. p99는 표본 수 부족(쿼리 10개면 100표본)으로 "표본 최댓값에 근사"라는 점을 출력에 footnote로 표기.

QPS = `total_iters_count / total_elapsed_seconds`.

---

## 출력 (Markdown)

```markdown
# Search Quality Report

- Generated: 2026-05-26T12:34:56Z
- HEAD (working tree): d09861e
- Index dir: .glc-index (4.5 MB, 12345 docs)
- Warning: HEAD ≠ index.head_oid — run `glc index` to refresh   ← 불일치 시에만 표시

## Aggregate
| Metric | Value |
|--------|-------|
| MRR | 0.78 |
| Recall@5 | 0.65 |
| Recall@10 | 0.82 |
| NDCG@10 | 0.71 |
| Queries | 8 |

## Performance (warmup=3, iters=10)
| p50 | p95 | p99* | mean | QPS |
|-----|-----|------|------|-----|
| 23.1 ms | 41.4 ms | 52.0 ms | 26.0 ms | 38.5 |

\* iters=10 표본에서 p99는 표본 최댓값 근사

## Index
- Embedding: potion-multilingual-128M (256-dim)
- BM25 tokenizer: default+lowercase
- Vector backend: turbovec
- HEAD: d09861e (indexed 2026-05-26T11:00:00Z)
- Docs: Commit=87, File=145, Symbol=12113

## Per-Query
| # | Query | MRR | R@5 | R@10 | NDCG@10 | Hit Rank | Hit Paths |
|---|-------|-----|-----|------|---------|----------|-----------|
| 1 | incremental indexing fallback | 1.00 | 1.00 | 1.00 | 0.93 | 1 | src/search/diff.rs, src/search/indexer.rs |
| 2 | tantivy delete_term | 0.50 | 1.00 | 1.00 | 0.63 | 2 | src/search/bm25.rs |
```

stdout 출력은 같은 정보를 `comfy-table::presets::UTF8_FULL`로 직조. 색은 의도적으로 사용하지 않음 — CI 로그에서 깨지지 않게.

---

## 엣지케이스 / 에러 처리

| 조건 | 동작 |
|------|------|
| `.glc-index/` 없음 | stderr에 `Run \`glc index\` first` 안내 후 exit 1 |
| fixture 파일 없음 | stderr에 경로와 에러 메시지 후 exit 1 |
| fixture에 쿼리 0개 | exit 1 |
| `expected = []`인 쿼리 | fixture 로드 시점에 검증 실패로 거부 (메트릭 정의가 무의미해짐) |
| HEAD ≠ `index.head_oid` | stdout 상단에 경고 1줄, 계속 진행 (인덱스가 약간 stale해도 품질 추세는 유효) |
| embedding dim mismatch | `SearchEngine::open`에서 이미 에러로 종료 |
| `--out` 디렉토리 부재 | 부모 디렉토리는 미리 존재 필요. mkdir은 하지 않음 (사용자 의도 보호) |

`ReportError`는 `src/search/report/mod.rs`에 정의:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("fixtures file not found: {0}")]
    FixturesMissing(PathBuf),
    #[error("no queries in fixtures")]
    EmptyFixtures,
    #[error("search engine error: {0}")]
    Search(#[from] SearchError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Toml(String),
}
```

---

## 의존성 추가

- `comfy-table = "7"` — stdout 표 렌더링
- `humansize = "2"` — 디스크 크기 사람 가독 (`4.5 MB`)

`chrono`는 indexer가 이미 사용. CI(`cargo clippy --all-targets -D warnings`)에서 새 경고가 없어야 한다.

---

## 테스트 전략

### 단위 테스트 (cargo test, 네트워크 불필요)

- `metrics.rs`:
  - `evaluate`: 매칭 규칙(path만 / kind 포함 / title 포함), first_hit_rank, MRR/Recall@5/Recall@10/NDCG@10 계산
  - 매칭 없음 → MRR=0, Recall=0, NDCG=0
  - fixture에서 빈 expected → load 단계 에러
  - 같은 path 여러 chunk → 첫 hit만 카운트
  - `aggregate`: 평균 계산, n_queries=0 처리
- `fixtures.rs`:
  - TOML 정상 파싱
  - 누락된 `text`/`expected` 에러
- `perf.rs`:
  - `latency_stats(samples)`: p50/p95/p99/mean 정확도

### 통합 테스트 (`#[ignore]`, 임베딩 모델 필요)

- 임시 레포 → `build_index` → 작은 fixture 1개로 `report::run` 호출 → 반환된 markdown 문자열에 다음 키워드 포함:
  - `"# Search Quality Report"`
  - `"MRR"`, `"NDCG@10"`
  - `"## Per-Query"`
- `--out` 경로 지정 시 파일 존재 검증

기존 `tests/fixtures/search_queries.toml`은 dogfooding용이라 lint/format 외 자동 검증 없음. 새 쿼리 추가는 사람이 PR에서 검토.

---

## File Structure 요약

- **Create** `src/search/report/mod.rs` — `run(&GitRepo, &ReportOptions)` 진입, `ReportError`, `ReportOptions`
- **Create** `src/search/report/fixtures.rs` — TOML 로드 + 타입
- **Create** `src/search/report/metrics.rs` — 매칭/계산 순수 함수
- **Create** `src/search/report/perf.rs` — warmup/측정 루프, LatencyStats
- **Create** `src/search/report/render.rs` — stdout(`comfy-table`) + markdown 직렬화
- **Create** `tests/fixtures/search_queries.toml` — 초기 5~10개 쿼리
- **Modify** `src/search/mod.rs` — `pub mod report;`
- **Modify** `src/cli.rs` — `Commands::Report { ... }` variant
- **Modify** `src/main.rs` — `Report` 분기 처리
- **Modify** `Cargo.toml` — `comfy-table`, `humansize` 추가
- **Modify** `CLAUDE.md` — `glc report` 명령 한 줄 추가

---

## Open Items

이번 spec에서 결정 보류:
- per-query 표가 8+ 쿼리에서 길어지면 stdout이 정리 안 됨 → 필요해지면 `--top N`/`--worst-only` 옵션 추가 (현재 범위 밖)
- HTML/JSON 출력 — 필요해지면 별도 PR
- baseline 비교 (`glc report --baseline last_report.json`) — 별도 plan
