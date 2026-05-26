# Search Quality Report (`glc report`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `glc report` 서브명령을 추가해 gluck 자신의 검색 품질(MRR/Recall@5,10/NDCG@10)과 성능(p50/p95/p99/QPS)을 `tests/fixtures/search_queries.toml` 기반으로 측정하고 stdout(또는 `--out` markdown 파일)에 리포트로 출력한다.

**Architecture:** `src/search/report/` 서브모듈에 `fixtures.rs`(TOML 로드) · `metrics.rs`(순수 매트릭 계산) · `perf.rs`(warmup + 반복 측정 latency 통계) · `render.rs`(stdout `comfy-table` + markdown) · `mod.rs`(`run` 오케스트레이션)로 책임을 분리. CLI는 `Commands::Report`를 추가해 `report::run`을 호출하며, fixture가 빈 expected를 거부하고 인덱스 부재/HEAD 불일치 같은 엣지케이스는 안내성 메시지로 처리한다.

**Tech Stack:** Rust 2021, clap (이미 사용), serde + toml (이미 사용), tantivy/turbovec (`SearchEngine`을 통해 read-only 사용), 신규 의존성 `comfy-table 7` + `humansize 2`.

**Spec:** [`docs/superpowers/specs/2026-05-26-search-quality-report-design.md`](../specs/2026-05-26-search-quality-report-design.md)

---

## File Structure

신규 파일:
- `src/search/report/mod.rs` — `ReportOptions`, `ReportError`, `Report` 데이터 모델, `run()`
- `src/search/report/fixtures.rs` — TOML 파서, `FixtureSet`, `FixtureQuery`, `ExpectedHit`, `load()`
- `src/search/report/metrics.rs` — `matches()`, `QueryEval`, `AggregateEval`, `evaluate()`, `aggregate()`
- `src/search/report/perf.rs` — `LatencyStats`, `latency_stats()`, `run_perf()`
- `src/search/report/render.rs` — `to_stdout()`, `to_markdown_string()`
- `tests/fixtures/search_queries.toml` — gluck 레포 대상 초기 쿼리 5~7개

수정 파일:
- `src/search/mod.rs` — `pub mod report;` 한 줄 추가
- `src/cli.rs` — `Commands::Report { ... }` variant 추가
- `src/main.rs` — `Report` 분기 → `gluck::search::report::run`
- `Cargo.toml` — `comfy-table`, `humansize` 추가
- `CLAUDE.md` — 명령 한 줄 추가

각 step은 2~5분 단위. 모든 task는 TDD(test → fail → impl → pass → commit)를 따른다.

---

## Task 1: Cargo deps 추가 (`comfy-table`, `humansize`)

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: `Cargo.toml` `[dependencies]` 섹션에 추가**

`Cargo.toml`의 `[dependencies]` 마지막 의존성 뒤(`tempfile = "3"` 위쪽 또는 적절한 위치)에 추가:

```toml
comfy-table = "7"
humansize = "2"
```

- [ ] **Step 2: 컴파일 확인**

Run: `cargo build --lib`
Expected: 성공 (다운로드 후 컴파일 통과)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "Add comfy-table and humansize dependencies"
```

---

## Task 2: `report` 모듈 스켈레톤 + 등록

**Files:**
- Create: `src/search/report/mod.rs`
- Modify: `src/search/mod.rs`

이 task는 단순 등록이라 별도 테스트 없음. 후속 task에서 채워질 빈 모듈만 등록.

- [ ] **Step 1: `src/search/report/mod.rs` 생성**

```rust
//! `glc report` 서브명령 구현. 검색 품질·성능 리포트 생성.

pub mod fixtures;
pub mod metrics;
pub mod perf;
pub mod render;

use std::path::PathBuf;

use thiserror::Error;

use crate::search::SearchError;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("fixtures file not found: {0}")]
    FixturesMissing(PathBuf),
    #[error("no queries in fixtures")]
    EmptyFixtures,
    #[error("query #{0} has empty `expected` array")]
    EmptyExpected(usize),
    #[error("search engine error: {0}")]
    Search(#[from] SearchError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Toml(String),
}

impl From<toml::de::Error> for ReportError {
    fn from(e: toml::de::Error) -> Self {
        Self::Toml(e.to_string())
    }
}
```

- [ ] **Step 2: `src/search/mod.rs`에 모듈 등록**

`src/search/mod.rs:1` 부근의 `pub mod ...;` 선언 그룹에 추가:

```rust
pub mod report;
```

(다른 `pub mod` 라인들과 같은 위치, 알파벳 순으로 `rrf`와 `silence` 사이에 들어감.)

- [ ] **Step 3: 빌드 확인**

Run: `cargo build --lib`
Expected: `fixtures/metrics/perf/render` 모듈 부재로 컴파일 실패

이 컴파일 에러는 task 3에서 빈 파일들을 만들면 해결되므로, 잠시 실패 상태로 두지 말고 **다음 step에서 빈 파일들을 함께 만든다**.

- [ ] **Step 4: 빈 서브모듈 4개 생성**

다음 4개 파일을 비어 있는 상태로 생성 (각 파일 내용은 한 줄 주석만):

`src/search/report/fixtures.rs`:
```rust
//! Fixture TOML 로드 — Task 3에서 채움.
```

`src/search/report/metrics.rs`:
```rust
//! 메트릭 계산 — Task 4에서 채움.
```

`src/search/report/perf.rs`:
```rust
//! 성능 측정 — Task 5에서 채움.
```

`src/search/report/render.rs`:
```rust
//! 출력 렌더링 — Task 6에서 채움.
```

- [ ] **Step 5: 빌드 확인**

Run: `cargo build --lib`
Expected: 성공

- [ ] **Step 6: Commit**

```bash
git add src/search/mod.rs src/search/report/
git commit -m "Add empty search::report module skeleton"
```

---

## Task 3: `fixtures.rs` — TOML 로드 + 빈 expected 거부

**Files:**
- Modify: `src/search/report/fixtures.rs`

- [ ] **Step 1: Write the failing tests**

`src/search/report/fixtures.rs`의 내용을 다음으로 **교체**:

```rust
//! Fixture TOML 로드.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::search::DocKind;
use crate::search::report::ReportError;

#[derive(Debug, Deserialize)]
pub struct FixtureSet {
    #[serde(default, rename = "query")]
    pub queries: Vec<FixtureQuery>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureQuery {
    pub text: String,
    pub expected: Vec<ExpectedHit>,
}

#[derive(Debug, Deserialize)]
pub struct ExpectedHit {
    pub path: String,
    #[serde(default)]
    pub kind: Option<DocKind>,
    #[serde(default)]
    pub title: Option<String>,
}

pub fn load(path: &Path) -> Result<FixtureSet, ReportError> {
    if !path.exists() {
        return Err(ReportError::FixturesMissing(path.to_path_buf()));
    }
    let s = std::fs::read_to_string(path)?;
    let set: FixtureSet = toml::from_str(&s)?;
    if set.queries.is_empty() {
        return Err(ReportError::EmptyFixtures);
    }
    for (i, q) in set.queries.iter().enumerate() {
        if q.expected.is_empty() {
            return Err(ReportError::EmptyExpected(i));
        }
    }
    Ok(set)
}

// 디버깅용: PathBuf 인자를 받는 thin wrapper
pub fn load_path(p: &PathBuf) -> Result<FixtureSet, ReportError> {
    load(p.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &tempfile::TempDir, body: &str) -> PathBuf {
        let p = dir.path().join("q.toml");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn loads_minimal_query() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
text = "hello"
expected = [{ path = "src/main.rs" }]
"#,
        );
        let set = load(&p).unwrap();
        assert_eq!(set.queries.len(), 1);
        assert_eq!(set.queries[0].text, "hello");
        assert_eq!(set.queries[0].expected[0].path, "src/main.rs");
        assert!(set.queries[0].expected[0].kind.is_none());
        assert!(set.queries[0].expected[0].title.is_none());
    }

    #[test]
    fn parses_kind_and_title() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
text = "delete_term"
expected = [
    { path = "src/search/bm25.rs", kind = "Symbol", title = "delete_doc" },
]
"#,
        );
        let set = load(&p).unwrap();
        let e = &set.queries[0].expected[0];
        assert_eq!(e.kind, Some(DocKind::Symbol));
        assert_eq!(e.title.as_deref(), Some("delete_doc"));
    }

    #[test]
    fn rejects_missing_file() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope.toml");
        match load(&missing) {
            Err(ReportError::FixturesMissing(p)) => assert_eq!(p, missing),
            other => panic!("expected FixturesMissing, got {other:?}"),
        }
    }

    #[test]
    fn rejects_zero_queries() {
        let dir = tempdir().unwrap();
        let p = write(&dir, "");
        match load(&p) {
            Err(ReportError::EmptyFixtures) => {}
            other => panic!("expected EmptyFixtures, got {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_expected() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
text = "hello"
expected = []
"#,
        );
        match load(&p) {
            Err(ReportError::EmptyExpected(i)) => assert_eq!(i, 0),
            other => panic!("expected EmptyExpected, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib search::report::fixtures::tests`
Expected: 5개 테스트 모두 PASS

(Step 1에서 구현과 테스트를 함께 작성했으므로 fail/pass 분리는 생략 — 작은 모듈은 한 번에 작성하고 결과로 검증.)

- [ ] **Step 3: Commit**

```bash
git add src/search/report/fixtures.rs
git commit -m "Add fixtures loader with empty-expected validation"
```

---

## Task 4: `metrics.rs` — 매칭 + MRR/Recall/NDCG

**Files:**
- Modify: `src/search/report/metrics.rs`

이 task는 `SearchEngine`을 전혀 쓰지 않는다. 테스트는 fixture와 가짜 `SearchResult`만으로 구성.

- [ ] **Step 1: Write failing tests + implementation in one shot**

`src/search/report/metrics.rs` 전체를 다음으로 교체:

```rust
//! 검색 품질 메트릭 — MRR, Recall@k, NDCG@k.

use crate::search::report::fixtures::{ExpectedHit, FixtureQuery};
use crate::search::SearchResult;

#[derive(Debug, Clone)]
pub struct QueryEval {
    pub query: String,
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
    pub first_hit_rank: Option<usize>,
    pub hit_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AggregateEval {
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
    pub n_queries: usize,
}

fn matches(expected: &ExpectedHit, hit: &SearchResult) -> bool {
    if hit.meta.path.as_deref() != Some(expected.path.as_str()) {
        return false;
    }
    if let Some(k) = &expected.kind {
        if &hit.meta.kind != k {
            return false;
        }
    }
    if let Some(t) = &expected.title {
        // Symbol DocMeta.title은 "{symbol_name} ({path})" 형식.
        let name = hit
            .meta
            .title
            .split_once(" (")
            .map(|(s, _)| s)
            .unwrap_or(&hit.meta.title);
        if name != t {
            return false;
        }
    }
    true
}

/// `results`의 i번째(0-indexed)가 expected 어딘가와 매칭되면 true.
fn is_relevant(expected: &[ExpectedHit], hit: &SearchResult) -> bool {
    expected.iter().any(|e| matches(e, hit))
}

pub fn evaluate(query: &FixtureQuery, results: &[SearchResult]) -> QueryEval {
    let mut first_hit_rank: Option<usize> = None;
    let mut hit_paths: Vec<String> = Vec::new();
    let mut hit_count_at_5 = 0usize;
    let mut hit_count_at_10 = 0usize;
    let mut dcg10 = 0.0_f64;

    for (i, r) in results.iter().take(10).enumerate() {
        let rank = i + 1;
        if is_relevant(&query.expected, r) {
            if first_hit_rank.is_none() {
                first_hit_rank = Some(rank);
            }
            if let Some(p) = &r.meta.path {
                if !hit_paths.iter().any(|x| x == p) {
                    hit_paths.push(p.clone());
                }
            }
            if rank <= 5 {
                hit_count_at_5 += 1;
            }
            hit_count_at_10 += 1;
            dcg10 += 1.0 / ((rank as f64 + 1.0).log2());
        }
    }

    let n_expected = query.expected.len().max(1);
    let recall_at_5 = (hit_count_at_5.min(query.expected.len()) as f32)
        / (query.expected.len().min(5).max(1) as f32);
    let recall_at_10 = (hit_count_at_10.min(query.expected.len()) as f32)
        / (n_expected as f32);

    // IDCG@10: min(10, |expected|) 개 위치에 1.0이 이상적으로 배치된 경우.
    let ideal_k = query.expected.len().min(10);
    let mut idcg10 = 0.0_f64;
    for i in 0..ideal_k {
        let rank = i + 1;
        idcg10 += 1.0 / ((rank as f64 + 1.0).log2());
    }
    let ndcg_at_10 = if idcg10 == 0.0 {
        1.0
    } else {
        (dcg10 / idcg10) as f32
    };

    let mrr = first_hit_rank.map(|r| 1.0 / r as f32).unwrap_or(0.0);

    QueryEval {
        query: query.text.clone(),
        mrr,
        recall_at_5,
        recall_at_10,
        ndcg_at_10,
        first_hit_rank,
        hit_paths,
    }
}

pub fn aggregate(per_query: &[QueryEval]) -> AggregateEval {
    let n = per_query.len();
    if n == 0 {
        return AggregateEval {
            mrr: 0.0,
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            ndcg_at_10: 0.0,
            n_queries: 0,
        };
    }
    let nf = n as f32;
    let sum = |f: fn(&QueryEval) -> f32| per_query.iter().map(f).sum::<f32>();
    AggregateEval {
        mrr: sum(|q| q.mrr) / nf,
        recall_at_5: sum(|q| q.recall_at_5) / nf,
        recall_at_10: sum(|q| q.recall_at_10) / nf,
        ndcg_at_10: sum(|q| q.ndcg_at_10) / nf,
        n_queries: n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{DocKind, DocMeta, SearchResult};

    fn meta(doc_id: u64, kind: DocKind, path: &str, title: &str) -> DocMeta {
        DocMeta {
            doc_id,
            kind,
            title: title.to_string(),
            commit_oid: "0".repeat(40),
            path: Some(path.to_string()),
            line_start: None,
            line_end: None,
        }
    }

    fn result(doc_id: u64, kind: DocKind, path: &str, title: &str) -> SearchResult {
        SearchResult {
            score: 1.0,
            meta: meta(doc_id, kind, path, title),
        }
    }

    fn fq(text: &str, expected: Vec<ExpectedHit>) -> FixtureQuery {
        FixtureQuery {
            text: text.to_string(),
            expected,
        }
    }

    fn eh(path: &str) -> ExpectedHit {
        ExpectedHit {
            path: path.to_string(),
            kind: None,
            title: None,
        }
    }

    fn eh_kind(path: &str, kind: DocKind) -> ExpectedHit {
        ExpectedHit {
            path: path.to_string(),
            kind: Some(kind),
            title: None,
        }
    }

    fn eh_full(path: &str, kind: DocKind, title: &str) -> ExpectedHit {
        ExpectedHit {
            path: path.to_string(),
            kind: Some(kind),
            title: Some(title.to_string()),
        }
    }

    #[test]
    fn first_rank_and_mrr() {
        // expected: src/a.rs. results: b, a, c → rank=2, MRR=0.5
        let q = fq("x", vec![eh("src/a.rs")]);
        let res = vec![
            result(1, DocKind::File, "src/b.rs", "src/b.rs"),
            result(2, DocKind::File, "src/a.rs", "src/a.rs"),
            result(3, DocKind::File, "src/c.rs", "src/c.rs"),
        ];
        let e = evaluate(&q, &res);
        assert_eq!(e.first_hit_rank, Some(2));
        assert!((e.mrr - 0.5).abs() < 1e-6);
    }

    #[test]
    fn no_match_gives_zero() {
        let q = fq("x", vec![eh("src/a.rs")]);
        let res = vec![result(1, DocKind::File, "src/b.rs", "src/b.rs")];
        let e = evaluate(&q, &res);
        assert_eq!(e.first_hit_rank, None);
        assert_eq!(e.mrr, 0.0);
        assert_eq!(e.recall_at_5, 0.0);
        assert_eq!(e.recall_at_10, 0.0);
        assert_eq!(e.ndcg_at_10, 0.0);
    }

    #[test]
    fn recall_at_k_counts_distinct_expected_hits() {
        // 두 개 expected, results에 둘 다 등장
        let q = fq("x", vec![eh("src/a.rs"), eh("src/b.rs")]);
        let res = vec![
            result(1, DocKind::File, "src/a.rs", "src/a.rs"),
            result(2, DocKind::File, "src/b.rs", "src/b.rs"),
            result(3, DocKind::File, "src/c.rs", "src/c.rs"),
        ];
        let e = evaluate(&q, &res);
        assert!((e.recall_at_10 - 1.0).abs() < 1e-6);
        assert!((e.recall_at_5 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn same_path_multiple_chunks_counts_once() {
        // 같은 path 두 번 등장하지만 첫 hit만 카운트
        let q = fq("x", vec![eh("src/a.rs"), eh("src/b.rs")]);
        let res = vec![
            result(1, DocKind::Symbol, "src/a.rs", "f1 (src/a.rs)"),
            result(2, DocKind::Symbol, "src/a.rs", "f2 (src/a.rs)"),
            result(3, DocKind::File, "src/b.rs", "src/b.rs"),
        ];
        let e = evaluate(&q, &res);
        assert_eq!(e.hit_paths, vec!["src/a.rs".to_string(), "src/b.rs".to_string()]);
    }

    #[test]
    fn matches_symbol_title_strips_path_suffix() {
        let q = fq(
            "x",
            vec![eh_full("src/a.rs", DocKind::Symbol, "build_index_incremental")],
        );
        let res = vec![result(
            1,
            DocKind::Symbol,
            "src/a.rs",
            "build_index_incremental (src/a.rs)",
        )];
        let e = evaluate(&q, &res);
        assert_eq!(e.first_hit_rank, Some(1));
    }

    #[test]
    fn kind_filter_rejects_wrong_kind() {
        let q = fq("x", vec![eh_kind("src/a.rs", DocKind::Symbol)]);
        let res = vec![result(1, DocKind::File, "src/a.rs", "src/a.rs")];
        let e = evaluate(&q, &res);
        assert_eq!(e.first_hit_rank, None);
    }

    #[test]
    fn ndcg_perfect_when_top_matches() {
        // expected 2개가 results의 1, 2위에 정확히 위치 → NDCG = 1.0
        let q = fq("x", vec![eh("src/a.rs"), eh("src/b.rs")]);
        let res = vec![
            result(1, DocKind::File, "src/a.rs", "src/a.rs"),
            result(2, DocKind::File, "src/b.rs", "src/b.rs"),
        ];
        let e = evaluate(&q, &res);
        assert!((e.ndcg_at_10 - 1.0).abs() < 1e-6, "got {}", e.ndcg_at_10);
    }

    #[test]
    fn aggregate_averages_metrics() {
        let q1 = QueryEval {
            query: "a".into(),
            mrr: 1.0,
            recall_at_5: 1.0,
            recall_at_10: 1.0,
            ndcg_at_10: 1.0,
            first_hit_rank: Some(1),
            hit_paths: vec![],
        };
        let q2 = QueryEval {
            query: "b".into(),
            mrr: 0.0,
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            ndcg_at_10: 0.0,
            first_hit_rank: None,
            hit_paths: vec![],
        };
        let agg = aggregate(&[q1, q2]);
        assert_eq!(agg.n_queries, 2);
        assert!((agg.mrr - 0.5).abs() < 1e-6);
        assert!((agg.ndcg_at_10 - 0.5).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib search::report::metrics::tests`
Expected: 8개 테스트 PASS

- [ ] **Step 3: Commit**

```bash
git add src/search/report/metrics.rs
git commit -m "Add MRR/Recall@k/NDCG@10 metrics with matching rules"
```

---

## Task 5: `perf.rs` — latency stats + warmup/측정 루프

**Files:**
- Modify: `src/search/report/perf.rs`

- [ ] **Step 1: 구현 + 단위 테스트 작성**

`src/search/report/perf.rs` 전체를 다음으로 교체:

```rust
//! 성능 측정 — warmup + iters 루프, percentile/평균 계산.

use std::time::Instant;

use crate::search::SearchEngine;
use crate::search::SearchResult;
use crate::search::report::fixtures::FixtureQuery;
use crate::search::report::ReportError;

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub mean_ms: f64,
    pub qps: f64,
    pub n_samples: usize,
}

/// 정렬된 밀리초 샘플에서 percentile (0.0~1.0)을 nearest-rank로 산출.
fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let rank = (p * sorted_ms.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted_ms.len() - 1);
    sorted_ms[idx]
}

pub fn latency_stats(samples_ms: &[f64], total_elapsed_s: f64) -> LatencyStats {
    let mut sorted: Vec<f64> = samples_ms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = if sorted.is_empty() {
        0.0
    } else {
        sorted.iter().sum::<f64>() / sorted.len() as f64
    };
    let qps = if total_elapsed_s > 0.0 {
        sorted.len() as f64 / total_elapsed_s
    } else {
        0.0
    };
    LatencyStats {
        p50_ms: percentile(&sorted, 0.50),
        p95_ms: percentile(&sorted, 0.95),
        p99_ms: percentile(&sorted, 0.99),
        mean_ms: mean,
        qps,
        n_samples: sorted.len(),
    }
}

/// warmup회 검색 후 iters회 반복하며 latency 수집. 마지막 iter의 결과(쿼리당 1개) 반환.
pub fn run_perf(
    engine: &SearchEngine,
    queries: &[FixtureQuery],
    warmup: usize,
    iters: usize,
    limit: usize,
) -> Result<(LatencyStats, Vec<Vec<SearchResult>>), ReportError> {
    for _ in 0..warmup {
        for q in queries {
            let _ = engine.search(&q.text, limit)?;
        }
    }

    let mut latencies_ms: Vec<f64> = Vec::with_capacity(iters * queries.len());
    let mut last_results: Vec<Vec<SearchResult>> = Vec::with_capacity(queries.len());

    let total_start = Instant::now();
    for iter_i in 0..iters {
        if iter_i == iters - 1 {
            last_results.clear();
        }
        for q in queries {
            let t0 = Instant::now();
            let r = engine.search(&q.text, limit)?;
            let dt_ms = t0.elapsed().as_secs_f64() * 1000.0;
            latencies_ms.push(dt_ms);
            if iter_i == iters - 1 {
                last_results.push(r);
            }
        }
    }
    let total_s = total_start.elapsed().as_secs_f64();

    Ok((latency_stats(&latencies_ms, total_s), last_results))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_basic() {
        let s = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&s, 0.5), 5.0);
        assert_eq!(percentile(&s, 0.95), 10.0);
        assert_eq!(percentile(&s, 0.99), 10.0);
    }

    #[test]
    fn percentile_empty() {
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn latency_stats_basic() {
        let samples = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let stats = latency_stats(&samples, 0.5);
        assert!((stats.mean_ms - 30.0).abs() < 1e-6);
        assert_eq!(stats.p50_ms, 30.0);
        assert_eq!(stats.p95_ms, 50.0);
        assert!((stats.qps - 10.0).abs() < 1e-6); // 5 samples / 0.5s
        assert_eq!(stats.n_samples, 5);
    }

    #[test]
    fn latency_stats_zero_elapsed_gives_zero_qps() {
        let stats = latency_stats(&[1.0, 2.0], 0.0);
        assert_eq!(stats.qps, 0.0);
    }
}
```

- [ ] **Step 2: Run unit tests**

Run: `cargo test --lib search::report::perf::tests`
Expected: 3개 테스트 PASS

(`run_perf`의 e2e 검증은 task 9의 통합 테스트에서.)

- [ ] **Step 3: Commit**

```bash
git add src/search/report/perf.rs
git commit -m "Add latency stats + run_perf warmup/measurement loop"
```

---

## Task 6: `render.rs` — markdown 직렬화 + stdout

**Files:**
- Modify: `src/search/report/render.rs`

`Report` 데이터 모델은 `mod.rs`에 정의하지만, 이 task에서 미리 사용하기 위해 동일 시그니처로 `render.rs` 안에 import. mod.rs는 task 7에서 만든다 — render의 함수는 `&Report`를 받지만 `Report` 정의는 task 7로 미룬다 → 순서가 꼬임. 그래서 **이 task에서 `Report` 구조체를 `mod.rs`에 먼저 추가**하고 render는 그것을 import한다.

- [ ] **Step 1: `mod.rs`에 `Report` 데이터 구조 추가**

`src/search/report/mod.rs`의 `impl From<toml::de::Error>` 뒤에 추가:

```rust
use crate::search::report::metrics::{AggregateEval, QueryEval};
use crate::search::report::perf::LatencyStats;

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub index_dir: PathBuf,
    pub size_bytes: u64,
    pub doc_count_total: usize,
    pub doc_count_commit: usize,
    pub doc_count_file: usize,
    pub doc_count_symbol: usize,
    pub head_oid: String,
    pub indexed_at: String,
    pub embedding_model: String,
    pub embedding_dim: usize,
    pub bm25_tokenizer: String,
    pub vector_backend: String,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub generated_at: String,
    pub working_head_oid: String,
    pub head_mismatch: bool,
    pub warmup: usize,
    pub iters: usize,
    pub limit: usize,
    pub aggregate: AggregateEval,
    pub latency: LatencyStats,
    pub index: IndexStats,
    pub per_query: Vec<QueryEval>,
}
```

- [ ] **Step 2: `render.rs` 구현 + 테스트**

`src/search/report/render.rs` 전체를 다음으로 교체:

```rust
//! Report → stdout(comfy-table) / markdown 문자열.

use std::fmt::Write;

use comfy_table::presets::UTF8_FULL;
use comfy_table::{ContentArrangement, Table};
use humansize::{format_size, BINARY};

use crate::search::report::Report;

pub fn to_markdown_string(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Search Quality Report\n");
    let _ = writeln!(s, "- Generated: {}", r.generated_at);
    let _ = writeln!(s, "- HEAD (working tree): {}", r.working_head_oid);
    let _ = writeln!(
        s,
        "- Index dir: {} ({}, {} docs)",
        r.index.index_dir.display(),
        format_size(r.index.size_bytes, BINARY),
        r.index.doc_count_total
    );
    if r.head_mismatch {
        let _ = writeln!(
            s,
            "- ⚠ HEAD ≠ index.head_oid ({}) — run `glc index` to refresh",
            r.index.head_oid
        );
    }
    let _ = writeln!(s);

    let _ = writeln!(s, "## Aggregate\n");
    let _ = writeln!(s, "| Metric | Value |");
    let _ = writeln!(s, "|--------|-------|");
    let _ = writeln!(s, "| MRR | {:.3} |", r.aggregate.mrr);
    let _ = writeln!(s, "| Recall@5 | {:.3} |", r.aggregate.recall_at_5);
    let _ = writeln!(s, "| Recall@10 | {:.3} |", r.aggregate.recall_at_10);
    let _ = writeln!(s, "| NDCG@10 | {:.3} |", r.aggregate.ndcg_at_10);
    let _ = writeln!(s, "| Queries | {} |", r.aggregate.n_queries);
    let _ = writeln!(s);

    let _ = writeln!(
        s,
        "## Performance (warmup={}, iters={})\n",
        r.warmup, r.iters
    );
    let _ = writeln!(s, "| p50 | p95 | p99* | mean | QPS |");
    let _ = writeln!(s, "|-----|-----|------|------|-----|");
    let _ = writeln!(
        s,
        "| {:.2} ms | {:.2} ms | {:.2} ms | {:.2} ms | {:.1} |",
        r.latency.p50_ms, r.latency.p95_ms, r.latency.p99_ms, r.latency.mean_ms, r.latency.qps
    );
    let _ = writeln!(
        s,
        "\n\\* iters={} 표본에서 p99는 표본 최댓값 근사\n",
        r.iters
    );

    let _ = writeln!(s, "## Index\n");
    let _ = writeln!(
        s,
        "- Embedding: {} ({}-dim)",
        r.index.embedding_model, r.index.embedding_dim
    );
    let _ = writeln!(s, "- BM25 tokenizer: {}", r.index.bm25_tokenizer);
    let _ = writeln!(s, "- Vector backend: {}", r.index.vector_backend);
    let _ = writeln!(
        s,
        "- HEAD: {} (indexed {})",
        r.index.head_oid, r.index.indexed_at
    );
    let _ = writeln!(
        s,
        "- Docs: Commit={}, File={}, Symbol={}",
        r.index.doc_count_commit, r.index.doc_count_file, r.index.doc_count_symbol
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "## Per-Query\n");
    let _ = writeln!(
        s,
        "| # | Query | MRR | R@5 | R@10 | NDCG@10 | Hit Rank | Hit Paths |"
    );
    let _ = writeln!(
        s,
        "|---|-------|-----|-----|------|---------|----------|-----------|"
    );
    for (i, q) in r.per_query.iter().enumerate() {
        let rank_str = q
            .first_hit_rank
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        let paths = if q.hit_paths.is_empty() {
            "—".into()
        } else {
            q.hit_paths.join(", ")
        };
        let _ = writeln!(
            s,
            "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |",
            i + 1,
            q.query,
            q.mrr,
            q.recall_at_5,
            q.recall_at_10,
            q.ndcg_at_10,
            rank_str,
            paths
        );
    }
    s
}

pub fn to_stdout(r: &Report) {
    println!("Search Quality Report");
    println!();
    println!("Generated:   {}", r.generated_at);
    println!("HEAD:        {}", r.working_head_oid);
    println!(
        "Index:       {} ({}, {} docs)",
        r.index.index_dir.display(),
        format_size(r.index.size_bytes, BINARY),
        r.index.doc_count_total
    );
    if r.head_mismatch {
        println!(
            "WARNING:     HEAD != index.head_oid ({}); run `glc index` to refresh",
            r.index.head_oid
        );
    }
    println!();

    // Aggregate
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Metric", "Value"]);
    t.add_row(vec!["MRR".into(), format!("{:.3}", r.aggregate.mrr)]);
    t.add_row(vec![
        "Recall@5".into(),
        format!("{:.3}", r.aggregate.recall_at_5),
    ]);
    t.add_row(vec![
        "Recall@10".into(),
        format!("{:.3}", r.aggregate.recall_at_10),
    ]);
    t.add_row(vec![
        "NDCG@10".into(),
        format!("{:.3}", r.aggregate.ndcg_at_10),
    ]);
    t.add_row(vec![
        "Queries".into(),
        r.aggregate.n_queries.to_string(),
    ]);
    println!("Aggregate:");
    println!("{t}");
    println!();

    // Latency
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["p50", "p95", "p99*", "mean", "QPS"]);
    t.add_row(vec![
        format!("{:.2} ms", r.latency.p50_ms),
        format!("{:.2} ms", r.latency.p95_ms),
        format!("{:.2} ms", r.latency.p99_ms),
        format!("{:.2} ms", r.latency.mean_ms),
        format!("{:.1}", r.latency.qps),
    ]);
    println!(
        "Performance (warmup={}, iters={}, * p99 is sample-max approximation):",
        r.warmup, r.iters
    );
    println!("{t}");
    println!();

    // Per-query
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            "#", "Query", "MRR", "R@5", "R@10", "NDCG@10", "HitRank", "Hit Paths",
        ]);
    for (i, q) in r.per_query.iter().enumerate() {
        let rank_str = q
            .first_hit_rank
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        let paths = if q.hit_paths.is_empty() {
            "—".into()
        } else {
            q.hit_paths.join(", ")
        };
        t.add_row(vec![
            (i + 1).to_string(),
            q.query.clone(),
            format!("{:.3}", q.mrr),
            format!("{:.3}", q.recall_at_5),
            format!("{:.3}", q.recall_at_10),
            format!("{:.3}", q.ndcg_at_10),
            rank_str,
            paths,
        ]);
    }
    println!("Per-Query:");
    println!("{t}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::report::metrics::{AggregateEval, QueryEval};
    use crate::search::report::perf::LatencyStats;
    use crate::search::report::IndexStats;
    use std::path::PathBuf;

    fn sample_report() -> Report {
        Report {
            generated_at: "2026-05-26T00:00:00Z".into(),
            working_head_oid: "abcdef1".into(),
            head_mismatch: false,
            warmup: 3,
            iters: 10,
            limit: 10,
            aggregate: AggregateEval {
                mrr: 0.75,
                recall_at_5: 0.6,
                recall_at_10: 0.8,
                ndcg_at_10: 0.7,
                n_queries: 2,
            },
            latency: LatencyStats {
                p50_ms: 20.0,
                p95_ms: 40.0,
                p99_ms: 50.0,
                mean_ms: 25.0,
                qps: 40.0,
                n_samples: 20,
            },
            index: IndexStats {
                index_dir: PathBuf::from(".glc-index"),
                size_bytes: 1_024_000,
                doc_count_total: 100,
                doc_count_commit: 10,
                doc_count_file: 30,
                doc_count_symbol: 60,
                head_oid: "abcdef1".into(),
                indexed_at: "0Z".into(),
                embedding_model: "potion".into(),
                embedding_dim: 256,
                bm25_tokenizer: "ngram_2_2".into(),
                vector_backend: "turbovec".into(),
            },
            per_query: vec![QueryEval {
                query: "q1".into(),
                mrr: 1.0,
                recall_at_5: 1.0,
                recall_at_10: 1.0,
                ndcg_at_10: 1.0,
                first_hit_rank: Some(1),
                hit_paths: vec!["src/a.rs".into()],
            }],
        }
    }

    #[test]
    fn markdown_contains_required_sections() {
        let md = to_markdown_string(&sample_report());
        assert!(md.contains("# Search Quality Report"));
        assert!(md.contains("## Aggregate"));
        assert!(md.contains("## Performance"));
        assert!(md.contains("## Index"));
        assert!(md.contains("## Per-Query"));
        assert!(md.contains("MRR"));
        assert!(md.contains("NDCG@10"));
        assert!(md.contains("src/a.rs"));
    }

    #[test]
    fn markdown_shows_head_mismatch_warning() {
        let mut r = sample_report();
        r.head_mismatch = true;
        let md = to_markdown_string(&r);
        assert!(md.contains("HEAD ≠ index.head_oid"));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib search::report::render::tests`
Expected: 2개 테스트 PASS

- [ ] **Step 4: Commit**

```bash
git add src/search/report/mod.rs src/search/report/render.rs
git commit -m "Add Report struct + markdown/stdout renderer"
```

---

## Task 7: `mod.rs` — `run()` 오케스트레이션

**Files:**
- Modify: `src/search/report/mod.rs`

- [ ] **Step 1: `ReportOptions` + `run()` 구현**

`src/search/report/mod.rs`의 끝(이전 task에서 추가한 `Report` 구조체 뒤)에 추가:

```rust
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::git::repo::GitRepo;
use crate::search::indexer::index_dir_for;
use crate::search::report::fixtures;
use crate::search::report::metrics::{aggregate, evaluate};
use crate::search::report::perf::run_perf;
use crate::search::report::render::{to_markdown_string, to_stdout};
use crate::search::{DocKind, IndexMeta, SearchEngine};

pub struct ReportOptions {
    pub fixtures_path: PathBuf,
    pub out_markdown: Option<PathBuf>,
    pub warmup: usize,
    pub iters: usize,
    pub limit: usize,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            fixtures_path: PathBuf::from("tests/fixtures/search_queries.toml"),
            out_markdown: None,
            warmup: 3,
            iters: 10,
            limit: 10,
        }
    }
}

fn now_epoch_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

fn working_head_oid(repo: &GitRepo) -> Result<String, ReportError> {
    let r = repo.repository();
    let head = r
        .head()
        .map_err(|e| ReportError::Io(std::io::Error::other(e.to_string())))?;
    let oid = head
        .peel_to_commit()
        .map_err(|e| ReportError::Io(std::io::Error::other(e.to_string())))?
        .id();
    Ok(oid.to_string())
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(read) = std::fs::read_dir(p) else { return 0 };
    for entry in read.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            total += dir_size(&entry.path());
        } else if let Ok(md) = entry.metadata() {
            total += md.len();
        }
    }
    total
}

pub fn run(repo: &GitRepo, repo_path: &Path, opts: &ReportOptions) -> Result<(), ReportError> {
    // 1. fixture 로드
    let set = fixtures::load(&opts.fixtures_path)?;

    // 2. SearchEngine open
    let index_dir = index_dir_for(repo_path);
    if !index_dir.join("meta.toml").exists() {
        return Err(ReportError::Search(
            crate::search::SearchError::IndexNotFound(index_dir.clone()),
        ));
    }
    let engine = SearchEngine::open(&index_dir)?;

    // 3. IndexMeta 로드 (인덱스 정적 정보용)
    let meta_str = std::fs::read_to_string(index_dir.join("meta.toml"))?;
    let meta: IndexMeta = toml::from_str(&meta_str)?;

    // 4. HEAD 비교
    let head = working_head_oid(repo)?;
    let head_mismatch = head != meta.head_oid;

    // 5. 성능 측정 + 마지막 iter 결과 수집
    let (latency, last_results) =
        run_perf(&engine, &set.queries, opts.warmup, opts.iters, opts.limit)?;

    // 6. 품질 평가
    let per_query: Vec<_> = set
        .queries
        .iter()
        .zip(last_results.iter())
        .map(|(q, r)| evaluate(q, r))
        .collect();
    let aggregate_eval = aggregate(&per_query);

    // 7. 인덱스 정적 정보 수집
    let mut commit_n = 0;
    let mut file_n = 0;
    let mut sym_n = 0;
    for m in engine.doc_store.values() {
        match m.kind {
            DocKind::Commit => commit_n += 1,
            DocKind::File => file_n += 1,
            DocKind::Symbol => sym_n += 1,
        }
    }
    let index_stats = IndexStats {
        index_dir: index_dir.clone(),
        size_bytes: dir_size(&index_dir),
        doc_count_total: engine.doc_store.len(),
        doc_count_commit: commit_n,
        doc_count_file: file_n,
        doc_count_symbol: sym_n,
        head_oid: meta.head_oid.clone(),
        indexed_at: meta.indexed_at.clone(),
        embedding_model: meta.embedding.model.clone(),
        embedding_dim: meta.embedding.dim,
        bm25_tokenizer: meta.bm25.tokenizer.clone(),
        vector_backend: meta.vector.backend.clone(),
    };

    let report = Report {
        generated_at: now_epoch_string(),
        working_head_oid: head,
        head_mismatch,
        warmup: opts.warmup,
        iters: opts.iters,
        limit: opts.limit,
        aggregate: aggregate_eval,
        latency,
        index: index_stats,
        per_query,
    };

    to_stdout(&report);

    if let Some(out_path) = &opts.out_markdown {
        let md = to_markdown_string(&report);
        std::fs::write(out_path, md)?;
        eprintln!("wrote markdown report to {}", out_path.display());
    }

    Ok(())
}
```

- [ ] **Step 2: 빌드 확인**

Run: `cargo build --lib`
Expected: 성공

만약 `IndexStats`가 위(task 6 step 1)에서 추가될 때 import가 잘못됐다면 컴파일 에러로 표면화됨. 그 경우 `IndexStats`를 `mod.rs` 안의 정의 그대로 두고, `render.rs`에서 `use crate::search::report::IndexStats;`로 가져온다.

- [ ] **Step 3: 기존 전체 lib 테스트가 깨지지 않는지 확인**

Run: `cargo test --lib search::report`
Expected: task 3/4/5/6에서 작성한 단위 테스트 모두 PASS

- [ ] **Step 4: Commit**

```bash
git add src/search/report/mod.rs
git commit -m "Wire run() orchestration for glc report"
```

---

## Task 8: CLI 통합 — `Commands::Report` + main 분기

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: `src/cli.rs`에 `Report` variant 추가**

`src/cli.rs`의 `pub enum Commands { Index { ... } }` 안에 `Index` 뒤로 추가:

```rust
    /// Generate search quality + performance report
    Report {
        /// Fixture TOML path
        #[arg(long, default_value = "tests/fixtures/search_queries.toml")]
        fixtures: String,

        /// Markdown output path (stdout is always shown)
        #[arg(long)]
        out: Option<String>,

        /// Warmup iterations per query
        #[arg(long, default_value = "3")]
        warmup: usize,

        /// Measurement iterations per query
        #[arg(long, default_value = "10")]
        iters: usize,

        /// top-k for search() (k=10 covers NDCG@10/Recall@10)
        #[arg(long, default_value = "10")]
        limit: usize,
    },
```

- [ ] **Step 2: `src/main.rs`에 분기 추가**

`src/main.rs:31` 부근의 `if let Some(Commands::Index { ... })` 블록 뒤에 추가:

```rust
    if let Some(Commands::Report {
        fixtures,
        out,
        warmup,
        iters,
        limit,
    }) = cli.command
    {
        let opts = gluck::search::report::ReportOptions {
            fixtures_path: PathBuf::from(fixtures),
            out_markdown: out.map(PathBuf::from),
            warmup,
            iters,
            limit,
        };
        match gluck::search::report::run(&repo, &path, &opts) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("report error: {}", e);
                if matches!(
                    e,
                    gluck::search::report::ReportError::Search(
                        gluck::search::SearchError::IndexNotFound(_)
                    )
                ) {
                    eprintln!("hint: run `glc index` first");
                }
                std::process::exit(1);
            }
        }
    }
```

기존 `Index` 분기에서 `match cli.command` 패턴이 if-let 체인이라 두 if-let이 순서대로 실행되어도 무방. `Commands::Index` 분기가 먼저 매치되면 `cli.command`가 move되어 두 번째 if-let에서 `Some(...)` 매치가 불가능 → **이 경우 cli.command 매치를 `match`로 통합한다**.

기존 `if let Some(Commands::Index { ... }) = cli.command { ... return Ok(()); }` 블록과 새 Report 블록을 다음과 같은 `match`로 합친다:

```rust
    match cli.command {
        Some(Commands::Index {
            force,
            batch_size,
            max_file_bytes,
        }) => {
            let opts = gluck::search::indexer::IndexOptions {
                force,
                batch_size,
                max_file_bytes,
            };
            gluck::search::indexer::build_index(&repo, &path, &opts, |msg| eprintln!("{}", msg))
                .map_err(|e| anyhow::anyhow!("index error: {}", e))?;
            return Ok(());
        }
        Some(Commands::Report {
            fixtures,
            out,
            warmup,
            iters,
            limit,
        }) => {
            let opts = gluck::search::report::ReportOptions {
                fixtures_path: PathBuf::from(fixtures),
                out_markdown: out.map(PathBuf::from),
                warmup,
                iters,
                limit,
            };
            match gluck::search::report::run(&repo, &path, &opts) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("report error: {}", e);
                    if matches!(
                        e,
                        gluck::search::report::ReportError::Search(
                            gluck::search::SearchError::IndexNotFound(_)
                        )
                    ) {
                        eprintln!("hint: run `glc index` first");
                    }
                    std::process::exit(1);
                }
            }
        }
        None => {}
    }
```

기존 if-let 블록은 통째로 삭제하고 위 match로 교체.

- [ ] **Step 3: 빌드 + clippy**

Run: `cargo build && cargo clippy --all-targets -D warnings`
Expected: 무경고

- [ ] **Step 4: CLI help 동작 확인**

Run: `cargo run --bin glc -- report --help 2>&1 | head -20`
Expected: 옵션 목록(`--fixtures`, `--out`, `--warmup`, `--iters`, `--limit`) 출력

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "Wire glc report subcommand"
```

---

## Task 9: 초기 fixture 작성 (`tests/fixtures/search_queries.toml`)

**Files:**
- Create: `tests/fixtures/search_queries.toml`

이 fixture는 gluck 레포 자체를 대상으로 한 dogfooding. 정답은 `src/...` 경로로 작성. 향후 PR로 추가/수정.

- [ ] **Step 1: 파일 생성**

`tests/fixtures/search_queries.toml`를 다음 내용으로 생성:

```toml
# gluck 자체 검색 회귀 추적용 fixture.
# 정답은 path 위주. Symbol 정답을 명시할 때만 kind/title을 추가.

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

[[query]]
text = "RRF reciprocal rank fusion"
expected = [
    { path = "src/search/rrf.rs" },
]

[[query]]
text = "embedding model load potion"
expected = [
    { path = "src/search/embedding.rs" },
]

[[query]]
text = "search modal state machine"
expected = [
    { path = "src/search/modal_state.rs" },
]

[[query]]
text = "tree sitter highlight configuration"
expected = [
    { path = "src/highlight/engine.rs" },
]

[[query]]
text = "git revwalk topological commit"
expected = [
    { path = "src/git/store.rs" },
]
```

- [ ] **Step 2: tests/.gitignore 확인 (있다면 fixtures 디렉토리가 ignore되지 않는지)**

Run: `git check-ignore tests/fixtures/search_queries.toml`
Expected: 빈 출력 (ignore되지 않음). 출력이 있으면 ignore 패턴 조정 필요 — 현재 레포 `.gitignore`는 `target/`, `*.log`, `.DS_Store`, `.glc-index/` 만 ignore하므로 영향 없음.

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/search_queries.toml
git commit -m "Add initial search query fixtures for glc report"
```

---

## Task 10: e2e 통합 테스트 (`#[ignore]`)

**Files:**
- Modify: `src/search/report/mod.rs` (tests 모듈 추가)

이 테스트는 임베딩 모델 로드(hf-hub 첫 실행 시 네트워크)가 필요하므로 `#[ignore]` 마커.

- [ ] **Step 1: 테스트 추가**

`src/search/report/mod.rs` 파일 끝에 추가:

```rust
#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::search::indexer::{build_index, IndexOptions};
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    #[ignore]
    fn report_runs_end_to_end_and_writes_markdown() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "alpha.rs", b"fn alpha_func() {}", "Add alpha");
        add_file_commit(&repo, "beta.rs", b"fn beta_func() {}", "Add beta");

        let git_repo = crate::git::repo::GitRepo::open(dir.path()).unwrap();
        let idx_opts = IndexOptions::default();
        build_index(&git_repo, dir.path(), &idx_opts, |_| {}).unwrap();

        // 작은 fixture 파일을 임시로 작성
        let fixtures_dir = tempdir().unwrap();
        let fixtures_path = fixtures_dir.path().join("q.toml");
        std::fs::write(
            &fixtures_path,
            r#"
[[query]]
text = "alpha_func"
expected = [{ path = "alpha.rs" }]

[[query]]
text = "beta_func"
expected = [{ path = "beta.rs" }]
"#,
        )
        .unwrap();

        let out_md = fixtures_dir.path().join("report.md");
        let opts = ReportOptions {
            fixtures_path,
            out_markdown: Some(out_md.clone()),
            warmup: 1,
            iters: 2,
            limit: 10,
        };
        run(&git_repo, dir.path(), &opts).unwrap();

        assert!(out_md.exists(), "markdown report should be written");
        let md = std::fs::read_to_string(&out_md).unwrap();
        assert!(md.contains("# Search Quality Report"));
        assert!(md.contains("## Aggregate"));
        assert!(md.contains("## Per-Query"));
        assert!(md.contains("alpha_func"));
        assert!(md.contains("beta_func"));
    }

    #[test]
    fn report_errors_when_index_missing() {
        let (dir, _repo) = init_test_repo();
        let git_repo = crate::git::repo::GitRepo::open(dir.path()).unwrap();

        let fixtures_dir = tempdir().unwrap();
        let fixtures_path = fixtures_dir.path().join("q.toml");
        std::fs::write(
            &fixtures_path,
            r#"
[[query]]
text = "x"
expected = [{ path = "a.rs" }]
"#,
        )
        .unwrap();

        let opts = ReportOptions {
            fixtures_path,
            out_markdown: None,
            warmup: 0,
            iters: 1,
            limit: 5,
        };
        let err = run(&git_repo, dir.path(), &opts).unwrap_err();
        assert!(
            matches!(err, ReportError::Search(crate::search::SearchError::IndexNotFound(_))),
            "got {:?}",
            err
        );
    }

    #[test]
    fn report_errors_when_fixtures_missing() {
        let (dir, _repo) = init_test_repo();
        let git_repo = crate::git::repo::GitRepo::open(dir.path()).unwrap();
        let opts = ReportOptions {
            fixtures_path: PathBuf::from("/nonexistent/path/q.toml"),
            out_markdown: None,
            warmup: 0,
            iters: 1,
            limit: 5,
        };
        let err = run(&git_repo, dir.path(), &opts).unwrap_err();
        assert!(matches!(err, ReportError::FixturesMissing(_)), "got {:?}", err);
    }
}
```

- [ ] **Step 2: 빠른 단위 테스트 (ignore 안 된 것) 통과 확인**

Run: `cargo test --lib search::report::e2e_tests::report_errors_when_index_missing search::report::e2e_tests::report_errors_when_fixtures_missing`
Expected: 2개 PASS

- [ ] **Step 3: ignored e2e 테스트 실행 (네트워크 + 임베딩 모델 캐시)**

Run: `cargo test --lib search::report::e2e_tests::report_runs_end_to_end_and_writes_markdown -- --ignored --nocapture`
Expected: PASS (모델 캐시되어 있으면 ~10초, 첫 다운로드 시 수십 초)

만약 임베딩 hf-hub 다운로드가 환경 제약으로 실패하면 stderr 메시지를 확인하고 `#[ignore]`인 채로 두고 task를 통과로 처리해도 됨 — 이 테스트는 dev 환경에서만 의미.

- [ ] **Step 4: Commit**

```bash
git add src/search/report/mod.rs
git commit -m "Add e2e tests for glc report"
```

---

## Task 11: 수동 smoke test on gluck repo

자동화 외 실제 동작 확인. 이 task는 gluck 레포 자체에서 명령을 돌려본다.

- [ ] **Step 1: 인덱스 준비**

Run: `cargo run --bin glc -- index 2>&1 | tail -5`
Expected: "Index is up to date." 또는 incremental update 메시지

- [ ] **Step 2: report 실행**

Run: `cargo run --bin glc -- report 2>&1 | tail -40`
Expected:
- "Search Quality Report" 헤더
- Aggregate / Performance / Per-Query 표 출력
- HEAD mismatch 경고는 없어야 함(방금 index 했으므로)

표가 깨지거나 메트릭이 모두 0이면 fixture 쿼리/정답을 조정.

- [ ] **Step 3: markdown 출력**

Run: `cargo run --bin glc -- report --out /tmp/glc-report.md && head -30 /tmp/glc-report.md`
Expected: markdown 헤더와 표

- [ ] **Step 4: fixture 부재 에러 동작 확인**

Run: `cargo run --bin glc -- report --fixtures /nonexistent.toml; echo "exit=$?"`
Expected: stderr에 "fixtures file not found" 메시지, exit=1

- [ ] **Step 5: index 부재 에러 동작 확인**

`.glc-index/`를 임시로 옮기고 실행:

Run:
```bash
mv .glc-index /tmp/glc-index-backup
cargo run --bin glc -- report 2>&1 | tail -5
echo "exit=$?"
mv /tmp/glc-index-backup .glc-index
```

Expected: "index not found" 에러 + "hint: run `glc index` first" 안내, exit=1. **`.glc-index` 복구 후 다시 `ls .glc-index/meta.toml`로 정상 복구 확인**.

이 step은 destructive(인덱스 폴더 이동)이지만 복구 가능. 실수로 두 mv 사이에 빌드/테스트가 실패해도 백업 위치(`/tmp/glc-index-backup`)에서 수동 복구.

---

## Task 12: 문서 갱신 (`CLAUDE.md`)

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Commands 섹션에 한 줄 추가**

`CLAUDE.md`의 `## Commands` 섹션, `cargo run --bin glc -- index [--force]` 라인 뒤에 추가:

```
cargo run --bin glc -- report [--out FILE.md]   # search quality + perf report
```

- [ ] **Step 2: Search systems 섹션에 fixture 위치 한 줄 추가**

`## Search systems (two separate)` 섹션의 마지막 단락(Index dir 설명 뒤)에 다음을 추가:

```
검색 품질 회귀 추적은 `glc report`가 `tests/fixtures/search_queries.toml`의 쿼리/정답으로 MRR/Recall@k/NDCG@10 + latency p50/p95/p99를 계산해 stdout(및 `--out` markdown)에 출력한다.
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "Document glc report command in CLAUDE.md"
```

---

## Self-Review Checklist

- [x] 모든 task가 exact 파일 경로 명시
- [x] 각 step에 실제 코드 또는 명령 포함 (TBD 없음)
- [x] TDD 사이클 또는 "구현+테스트 한 번에 + 결과 검증" 명시
- [x] fixture 매칭 규칙(Symbol title 정규화)이 spec + metrics.rs + 테스트 케이스에서 일치
- [x] 빈 expected 거부가 fixtures.rs와 spec의 엣지케이스 표에서 일치
- [x] CLI 옵션명(`--fixtures/--out/--warmup/--iters/--limit`)이 cli.rs와 main.rs match arm에서 일치
- [x] `IndexStats`/`Report`/`QueryEval`/`AggregateEval`/`LatencyStats` 시그니처가 mod.rs/render.rs/metrics.rs/perf.rs 사이에서 일치
- [x] `comfy-table` + `humansize` 의존성 task 1에서 추가, render.rs에서 사용
- [x] e2e 테스트가 `#[ignore]`(네트워크 의존)
- [x] HEAD mismatch 처리 — warning + 계속 진행 (spec과 일치)
- [x] 인덱스 부재 → `IndexNotFound` 에러 + main.rs에서 `hint` 출력

---

## Out of Scope (별도 plan)

1. **baseline 비교** — `glc report --baseline last_report.json` 형태로 이전 리포트 대비 회귀 표시
2. **per-query 표가 길어질 때 필터** — `--worst-only`, `--top N` 옵션
3. **JSON 출력** — 시계열 추적 스크립트용
4. **CI 자동화** — GitHub Actions에서 `report --out` 결과를 PR 코멘트로 게시
5. **graded relevance** — `expected = [{path = "...", grade = 3}]` 도입 + NDCG 의미 강화

---

## Execution Handoff

이 plan은 12개 task, 약 90~120분 추정(임베딩 e2e는 모델 캐시 여부에 따라 변동).

**1. Subagent-Driven (recommended)** — task당 fresh subagent + 리뷰
**2. Inline Execution** — 현 세션에서 batch 실행 + checkpoint

어느 방식을 쓰실지 알려주세요.
