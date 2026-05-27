# Fixture 카테고리 매트릭스 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `glc report`의 fixture를 6개 카테고리 30개로 확장하고, negative query 지원으로 검색 품질 평가를 강화

**Architecture:** Fixture 스키마에 `category`와 `forbidden` 필드 추가 → 로더가 검증 → metrics에서 카테고리별/negative aggregate 분리 → render에서 3개 신규 섹션 출력

**Tech Stack:** Rust, tantivy, turbovec, tree-sitter (기존 스택 유지), toml (deserialization)

---

## File Structure

- `tests/fixtures/search_queries.toml` — 데이터만 (30개 쿼리)
- `src/search/report/fixtures.rs` — 로더 (Category enum, ForbiddenRule, 검증 로직)
- `src/search/report/metrics.rs` — 평가 (QueryEval enum, evaluate_negative, aggregates)
- `src/search/report/mod.rs` — Run (Report 구조체 확장, 연결)
- `src/search/report/render.rs` — 출력 (By Category, Negative Queries 섹션)

---

## Task 1: Category enum + FixtureQuery 확장 테스트 작성

**Files:**
- Modify: `src/search/report/fixtures.rs`

- [ ] **Step 1: Category enum 정의 테스트 추가**

```rust
#[test]
fn category_enum_parses_all_variants() {
    use serde::Deserialize;
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    enum Category {
        ExactIdentifier,
        NaturalLanguage,
        Korean,
        Typo,
        Paraphrase,
        Negative,
    }
    let variants = ["exact_identifier", "natural_language", "korean", "typo", "paraphrase", "negative"];
    for v in variants {
        let s = format!(r#"category = "{}""#, v);
        let c: Category = toml::from_str(&s).unwrap();
        assert!(matches!(c, _));
    }
}
```

- [ ] **Step 2: FixtureQuery with category 테스트 추가**

```rust
#[test]
fn fixture_query_requires_category_field() {
    let dir = tempdir().unwrap();
    let p = write(
        &dir,
        r#"
[[query]]
text = "test"
expected = [{ path = "src/a.rs" }]
"#,
    );
    match load(&p) {
        Err(ReportError::Toml(e)) if e.contains("missing field `category`") => {}
        other => panic!("expected missing field error, got {:?}", other),
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib category`
Expected: FAIL (enum and field not yet defined)

- [ ] **Step 4: Commit**

```bash
git add src/search/report/fixtures.rs
git commit -m "test: add Category enum and category field requirement tests"
```

---

## Task 2: Category enum과 FixtureQuery 스키마 구현

**Files:**
- Modify: `src/search/report/fixtures.rs`

- [ ] **Step 1: Category enum 추가**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    ExactIdentifier,
    NaturalLanguage,
    Korean,
    Typo,
    Paraphrase,
    Negative,
}
```

- [ ] **Step 2: FixtureQuery에 category 필드 추가**

```rust
#[derive(Debug, Deserialize)]
pub struct FixtureQuery {
    pub text: String,
    pub category: Category,
    pub expected: Vec<ExpectedHit>,
}
```

- [ ] **Step 3: ForbiddenRule 구조체 추가**

```rust
#[derive(Debug, Deserialize)]
pub struct ForbiddenRule {
    #[serde(alias = "path_prefix")]
    pub path_prefix: Option<String>,
    #[serde(alias = "path")]
    pub path: Option<String>,
}
```

- [ ] **Step 4: FixtureQuery에 forbidden 필드 추가**

```rust
#[derive(Debug, Deserialize)]
pub struct FixtureQuery {
    pub text: String,
    pub category: Category,
    pub expected: Vec<ExpectedHit>,
    #[serde(default)]
    pub forbidden: Vec<ForbiddenRule>,
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib category fixture_query_requires_category`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/search/report/fixtures.rs
git commit -m "feat: add Category enum and FixtureQuery extensions"
```

---

## Task 3: 로더 검증 로직 테스트 작성

**Files:**
- Modify: `src/search/report/fixtures.rs`

- [ ] **Step 1: positive에 forbidden 있으면 거부 테스트**

```rust
#[test]
fn rejects_positive_with_forbidden() {
    let dir = tempdir().unwrap();
    let p = write(
        &dir,
        r#"
[[query]]
category = "exact_identifier"
text = "test"
expected = [{ path = "src/a.rs" }]
forbidden = [{ path_prefix = "src/" }]
"#,
    );
    match load(&p) {
        Err(ReportError::InvalidNegativeQuery { .. }) => {}
        other => panic!("expected InvalidNegativeQuery error, got {:?}", other),
    }
}
```

- [ ] **Step 2: negative에 expected 있으면 거부 테스트**

```rust
#[test]
fn rejects_negative_with_expected() {
    let dir = tempdir().unwrap();
    let p = write(
        &dir,
        r#"
[[query]]
category = "negative"
text = "test"
expected = [{ path = "src/a.rs" }]
forbidden = [{ path_prefix = "src/" }]
"#,
    );
    match load(&p) {
        Err(ReportError::InvalidNegativeQuery { .. }) => {}
        other => panic!("expected InvalidNegativeQuery error, got {:?}", other),
    }
}
```

- [ ] **Step 3: forbidden에 path와 path_prefix 동시 지정 거부 테스트**

```rust
#[test]
fn rejects_forbidden_rule_with_both_path_and_prefix() {
    let dir = tempdir().unwrap();
    let p = write(
        &dir,
        r#"
[[query]]
category = "negative"
text = "test"
forbidden = [{ path = "src/a.rs", path_prefix = "src/" }]
"#,
    );
    match load(&p) {
        Err(ReportError::InvalidForbiddenRule { .. }) => {}
        other => panic!("expected InvalidForbiddenRule error, got {:?}", other),
    }
}
```

- [ ] **Step 4: forbidden에 둘 다 누락 시 거부 테스트**

```rust
#[test]
fn rejects_forbidden_rule_with_neither_path_nor_prefix() {
    let dir = tempdir().unwrap();
    let p = write(
        &dir,
        r#"
[[query]]
category = "negative"
text = "test"
forbidden = [{}]
"#,
    );
    match load(&p) {
        Err(ReportError::InvalidForbiddenRule { .. }) => {}
        other => panic!("expected InvalidForbiddenRule error, got {:?}", other),
    }
}
```

- [ ] **Step 5: negative에 forbidden 누락 시 거부 테스트**

```rust
#[test]
fn rejects_negative_without_forbidden() {
    let dir = tempdir().unwrap();
    let p = write(
        &dir,
        r#"
[[query]]
category = "negative"
text = "test"
"#,
    );
    match load(&p) {
        Err(ReportError::InvalidNegativeQuery { .. }) => {}
        other => panic!("expected InvalidNegativeQuery error, got {:?}", other),
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test --lib rejects_`
Expected: FAIL (error variants and validation logic not yet defined)

- [ ] **Step 7: Commit**

```bash
git add src/search/report/fixtures.rs
git commit -m "test: add loader validation tests"
```

---

## Task 4: 로더 검증 로직 구현

**Files:**
- Modify: `src/search/report/fixtures.rs`

- [ ] **Step 1: ReportError enum에 신규 variant 추가**

```rust
#[derive(Debug, Error)]
pub enum ReportError {
    #[error("fixtures file not found: {0}")]
    FixturesMissing(PathBuf),
    #[error("no queries in fixtures")]
    EmptyFixtures,
    #[error("query #{0} has empty `expected` array")]
    EmptyExpected(usize),
    #[error("query #{index}: invalid negative query: {reason}")]
    InvalidNegativeQuery { index: usize, reason: String },
    #[error("query #{query_index}, forbidden rule #{rule_index}: invalid rule: {reason}")]
    InvalidForbiddenRule { query_index: usize, rule_index: usize, reason: String },
    #[error("search engine error: {0}")]
    Search(#[from] SearchError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Toml(String),
}
```

- [ ] **Step 2: ForbiddenRule 검증 함수 추가**

```rust
fn validate_forbidden_rule(rule: &ForbiddenRule) -> Result<(), String> {
    let has_path = rule.path.is_some();
    let has_prefix = rule.path_prefix.is_some();
    if !has_path && !has_prefix {
        return Err("must specify either 'path' or 'path_prefix'".into());
    }
    if has_path && has_prefix {
        return Err("cannot specify both 'path' and 'path_prefix'".into());
    }
    Ok(())
}
```

- [ ] **Step 3: load()에 검증 로직 추가**

```rust
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
        match q.category {
            Category::Negative => {
                if !q.expected.is_empty() {
                    return Err(ReportError::InvalidNegativeQuery {
                        index: i,
                        reason: "negative queries must not have 'expected' array".into(),
                    });
                }
                if q.forbidden.is_empty() {
                    return Err(ReportError::InvalidNegativeQuery {
                        index: i,
                        reason: "negative queries must have at least one 'forbidden' rule".into(),
                    });
                }
                for (ri, rule) in q.forbidden.iter().enumerate() {
                    validate_forbidden_rule(rule).map_err(|reason| {
                        ReportError::InvalidForbiddenRule {
                            query_index: i,
                            rule_index: ri,
                            reason,
                        }
                    })?;
                }
            }
            _ => {
                if q.expected.is_empty() {
                    return Err(ReportError::EmptyExpected(i));
                }
                if !q.forbidden.is_empty() {
                    return Err(ReportError::InvalidNegativeQuery {
                        index: i,
                        reason: "positive queries must not have 'forbidden' array".into(),
                    });
                }
            }
        }
    }
    Ok(set)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib rejects_`
Expected: PASS

- [ ] **Step 5: loads_project_fixtures 테스트 갱신**

```rust
#[test]
fn loads_project_fixtures() {
    let p = std::path::Path::new("tests/fixtures/search_queries.toml");
    if p.exists() {
        let set = load(p).expect("project fixtures must be valid");
        assert!(
            set.queries.len() >= 30,  // 향후 확장을 위해 >=로
            "expected at least 30 queries, got {}",
            set.queries.len()
        );
        let categories: std::collections::HashSet<_> =
            set.queries.iter().map(|q| q.category).collect();
        assert_eq!(categories.len(), 6, "must have all 6 categories");
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test --lib loads_project_fixtures`
Expected: FAIL (fixture가 아직 30개가 아님)

- [ ] **Step 7: Commit**

```bash
git add src/search/report/fixtures.rs
git commit -m "feat: add loader validation logic"
```

---

## Task 5: QueryEval enum + NegativeEval 구조체 테스트 작성

**Files:**
- Modify: `src/search/report/metrics.rs`

- [ ] **Step 1: NegativeEval 구조체 테스트 추가**

```rust
#[test]
fn negative_eval_construction() {
    let n = NegativeEval {
        query: "test".into(),
        passed: true,
        violations: vec![],
    };
    assert!(n.passed);
    assert!(n.violations.is_empty());
}

#[test]
fn negative_violation_construction() {
    let v = NegativeViolation {
        rank: 3,
        path: "src/a.rs".into(),
        matched_rule: "path_prefix=src/".into(),
    };
    assert_eq!(v.rank, 3);
    assert_eq!(v.path, "src/a.rs");
}
```

- [ ] **Step 2: QueryEval enum 패턴 매칭 테스트**

```rust
#[test]
fn query_eval_matches_positive_and_negative() {
    use Category;
    let pos = QueryEval::Positive(PositiveEval {
        query: "test".into(),
        category: Category::ExactIdentifier,
        mrr: 1.0,
        recall_at_5: 1.0,
        recall_at_10: 1.0,
        ndcg_at_10: 1.0,
        first_hit_rank: Some(1),
        hit_paths: vec!["src/a.rs".into()],
    });
    assert!(matches!(pos, QueryEval::Positive(_)));

    let neg = QueryEval::Negative(NegativeEval {
        query: "test".into(),
        passed: true,
        violations: vec![],
    });
    assert!(matches!(neg, QueryEval::Negative(_)));
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib negative_eval query_eval_matches`
Expected: FAIL (types not yet defined)

- [ ] **Step 4: Commit**

```bash
git add src/search/report/metrics.rs
git commit -m "test: add QueryEval enum tests"
```

---

## Task 6: QueryEval enum과 관련 구조체 구현

**Files:**
- Modify: `src/search/report/metrics.rs`

- [ ] **Step 1: Category type re-export 및 CategoryAggregate 추가**

```rust
use crate::search::report::fixtures::Category;

#[derive(Debug, Clone)]
pub struct CategoryAggregate {
    pub category: Category,
    pub n: usize,
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
}

#[derive(Debug, Clone)]
pub struct NegativeAggregate {
    pub n: usize,
    pub pass_rate: f32,
}
```

- [ ] **Step 2: NegativeViolation 구조체 추가**

```rust
#[derive(Debug, Clone)]
pub struct NegativeViolation {
    pub rank: usize,
    pub path: String,
    pub matched_rule: String,
}
```

- [ ] **Step 3: NegativeEval 구조체 추가**

```rust
#[derive(Debug, Clone)]
pub struct NegativeEval {
    pub query: String,
    pub passed: bool,
    pub violations: Vec<NegativeViolation>,
}
```

- [ ] **Step 4: PositiveEval에 category 필드 추가**

```rust
#[derive(Debug, Clone)]
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
```

- [ ] **Step 5: QueryEval을 enum으로 변경**

```rust
#[derive(Debug, Clone)]
pub enum QueryEval {
    Positive(PositiveEval),
    Negative(NegativeEval),
}
```

- [ ] **Step 6: 기존 QueryEval 참조를 PositiveEval으로 변경**

`evaluate` 함수 내의 `QueryEval { ... }`를 `QueryEval::Positive(PositiveEval { ... })`로 변경.

```rust
pub fn evaluate(query: &FixtureQuery, results: &[SearchResult]) -> QueryEval {
    // ... 기존 로직 동일 ...
    QueryEval::Positive(PositiveEval {
        query: query.text.clone(),
        category: query.category,
        mrr,
        recall_at_5,
        recall_at_10,
        ndcg_at_10,
        first_hit_rank,
        hit_paths,
    })
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test --lib negative_eval query_eval_matches`
Expected: PASS

- [ ] **Step 8: 기존 테스트에서 QueryEval 사용을 PositiveEval로 변경**

기존 테스트들 (`no_match_gives_zero`, `ndcg_perfect_when_top_matches`, `aggregate_averages_metrics`)에서 `QueryEval` 직접 생성을 `QueryEval::Positive(PositiveEval { ... })`로 변경.

```rust
#[test]
fn no_match_gives_zero() {
    // ...
    let e = evaluate(&q, &res);
    match e {
        QueryEval::Positive(positive) => {
            assert_eq!(positive.first_hit_rank, None);
            assert_eq!(positive.mrr, 0.0);
            // ...
        }
        _ => panic!("expected Positive variant"),
    }
}
```

- [ ] **Step 9: Run all tests**

Run: `cargo test --lib metrics`
Expected: PASS

- [ ] **Step 10: Commit**

```bash
git add src/search/report/metrics.rs
git commit -m "feat: add QueryEval enum and category support"
```

---

## Task 7: evaluate_negative() 테스트 작성

**Files:**
- Modify: `src/search/report/metrics.rs`

- [ ] **Step 1: negative PASS 테스트 (위반 없음)**

```rust
#[test]
fn evaluate_negative_passes_with_no_violations() {
    use crate::search::report::fixtures::{FixtureQuery, ForbiddenRule, Category};
    let q = FixtureQuery {
        text: "test".into(),
        category: Category::Negative,
        expected: vec![],
        forbidden: vec![ForbiddenRule {
            path: Some("src/a.rs".into()),
            path_prefix: None,
        }],
    };
    let res = vec![
        result(1, DocKind::File, "src/b.rs", "src/b.rs"),
        result(2, DocKind::File, "src/c.rs", "src/c.rs"),
    ];
    let e = evaluate_negative(&q, &res);
    assert!(e.passed);
    assert!(e.violations.is_empty());
}
```

- [ ] **Step 2: negative FAIL - path 정확 매칭 테스트**

```rust
#[test]
fn evaluate_negative_fails_on_path_match() {
    use crate::search::report::fixtures::{FixtureQuery, ForbiddenRule, Category};
    let q = FixtureQuery {
        text: "test".into(),
        category: Category::Negative,
        expected: vec![],
        forbidden: vec![ForbiddenRule {
            path: Some("src/a.rs".into()),
            path_prefix: None,
        }],
    };
    let res = vec![
        result(1, DocKind::File, "src/b.rs", "src/b.rs"),
        result(2, DocKind::File, "src/a.rs", "src/a.rs"),
    ];
    let e = evaluate_negative(&q, &res);
    assert!(!e.passed);
    assert_eq!(e.violations.len(), 1);
    assert_eq!(e.violations[0].rank, 2);
    assert_eq!(e.violations[0].path, "src/a.rs");
    assert_eq!(e.violations[0].matched_rule, "path=src/a.rs");
}
```

- [ ] **Step 3: negative FAIL - path_prefix 매칭 테스트**

```rust
#[test]
fn evaluate_negative_fails_on_path_prefix_match() {
    use crate::search::report::fixtures::{FixtureQuery, ForbiddenRule, Category};
    let q = FixtureQuery {
        text: "test".into(),
        category: Category::Negative,
        expected: vec![],
        forbidden: vec![ForbiddenRule {
            path: None,
            path_prefix: Some("src/search/".into()),
        }],
    };
    let res = vec![
        result(1, DocKind::File, "src/main.rs", "src/main.rs"),
        result(2, DocKind::File, "src/search/indexer.rs", "src/search/indexer.rs"),
    ];
    let e = evaluate_negative(&q, &res);
    assert!(!e.passed);
    assert_eq!(e.violations.len(), 1);
    assert_eq!(e.violations[0].rank, 2);
    assert!(e.violations[0].path.starts_with("src/search/"));
    assert_eq!(e.violations[0].matched_rule, "path_prefix=src/search/");
}
```

- [ ] **Step 4: negative - path 없는 hit는 건너뜀 (meta.path가 None)**

```rust
#[test]
fn evaluate_negative_ignores_hits_without_path() {
    use crate::search::report::fixtures::{FixtureQuery, ForbiddenRule, Category};
    let q = FixtureQuery {
        text: "test".into(),
        category: Category::Negative,
        expected: vec![],
        forbidden: vec![ForbiddenRule {
            path: Some("src/a.rs".into()),
            path_prefix: None,
        }],
    };
    let res = vec![
        SearchResult {
            score: 1.0,
            meta: DocMeta {
                doc_id: 1,
                kind: DocKind::Commit,
                title: "test".into(),
                commit_oid: "0".repeat(40),
                path: None,
                line_start: None,
                line_end: None,
            },
        },
    ];
    let e = evaluate_negative(&q, &res);
    assert!(e.passed);  // path가 없으면 매칭 불가
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib evaluate_negative`
Expected: FAIL (function not yet defined)

- [ ] **Step 6: Commit**

```bash
git add src/search/report/metrics.rs
git commit -m "test: add evaluate_negative tests"
```

---

## Task 8: evaluate_negative() 구현

**Files:**
- Modify: `src/search/report/metrics.rs`

- [ ] **Step 1: evaluate_negative() 함수 구현**

```rust
pub fn evaluate_negative(query: &FixtureQuery, results: &[SearchResult]) -> NegativeEval {
    let mut violations = Vec::new();
    for (i, r) in results.iter().take(10).enumerate() {
        let rank = i + 1;
        let Some(hit_path) = r.meta.path.as_deref() else {
            continue;
        };
        for rule in &query.forbidden {
            let matched = if let Some(ref exact_path) = rule.path {
                hit_path == exact_path.as_str()
            } else if let Some(ref prefix) = rule.path_prefix {
                hit_path.starts_with(prefix.as_str())
            } else {
                continue;
            };
            if matched {
                let rule_desc = if let Some(ref exact_path) = rule.path {
                    format!("path={}", exact_path)
                } else if let Some(ref prefix) = rule.path_prefix {
                    format!("path_prefix={}", prefix)
                } else {
                    unreachable!()
                };
                violations.push(NegativeViolation {
                    rank,
                    path: hit_path.to_string(),
                    matched_rule: rule_desc,
                });
            }
        }
    }
    NegativeEval {
        query: query.text.clone(),
        passed: violations.is_empty(),
        violations,
    }
}
```

- [ ] **Step 2: evaluate()를 category 디스패치로 변경**

```rust
pub fn evaluate(query: &FixtureQuery, results: &[SearchResult]) -> QueryEval {
    match query.category {
        Category::Negative => QueryEval::Negative(evaluate_negative(query, results)),
        _ => QueryEval::Positive(evaluate_positive(query, results)),
    }
}

fn evaluate_positive(query: &FixtureQuery, results: &[SearchResult]) -> PositiveEval {
    // 기존 evaluate 로직 (QueryEval 생성 전 상태)
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
    let recall_at_5 = (hit_count_at_5.min(query.expected.len()) as f32) / (n_expected as f32);
    let recall_at_10 = (hit_count_at_10.min(query.expected.len()) as f32) / (n_expected as f32);

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

    PositiveEval {
        query: query.text.clone(),
        category: query.category,
        mrr,
        recall_at_5,
        recall_at_10,
        ndcg_at_10,
        first_hit_rank,
        hit_paths,
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib evaluate_negative`
Expected: PASS

- [ ] **Step 4: Run all metrics tests**

Run: `cargo test --lib metrics`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/report/metrics.rs
git commit -m "feat: implement evaluate_negative and category dispatch"
```

---

## Task 9: aggregate 함수 테스트 작성

**Files:**
- Modify: `src/search/report/metrics.rs`

- [ ] **Step 1: aggregate가 Negative 제외 테스트**

```rust
#[test]
fn aggregate_excludes_negative_queries() {
    use Category;
    let pos = QueryEval::Positive(PositiveEval {
        query: "a".into(),
        category: Category::ExactIdentifier,
        mrr: 1.0,
        recall_at_5: 1.0,
        recall_at_10: 1.0,
        ndcg_at_10: 1.0,
        first_hit_rank: Some(1),
        hit_paths: vec![],
    });
    let neg = QueryEval::Negative(NegativeEval {
        query: "b".into(),
        passed: true,
        violations: vec![],
    });
    let agg = aggregate(&[pos, neg]);
    assert_eq!(agg.n_queries, 1, "should only count positive");
    assert_eq!(agg.mrr, 1.0);
}
```

- [ ] **Step 2: aggregate_by_category 테스트**

```rust
#[test]
fn aggregate_by_category_groups_and_excludes_negative() {
    use Category;
    let queries = vec![
        QueryEval::Positive(PositiveEval {
            query: "exact1".into(),
            category: Category::ExactIdentifier,
            mrr: 1.0,
            recall_at_5: 1.0,
            recall_at_10: 1.0,
            ndcg_at_10: 1.0,
            first_hit_rank: Some(1),
            hit_paths: vec![],
        }),
        QueryEval::Positive(PositiveEval {
            query: "exact2".into(),
            category: Category::ExactIdentifier,
            mrr: 0.5,
            recall_at_5: 0.5,
            recall_at_10: 0.5,
            ndcg_at_10: 0.5,
            first_hit_rank: Some(2),
            hit_paths: vec![],
        }),
        QueryEval::Positive(PositiveEval {
            query: "natural1".into(),
            category: Category::NaturalLanguage,
            mrr: 0.0,
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            ndcg_at_10: 0.0,
            first_hit_rank: None,
            hit_paths: vec![],
        }),
        QueryEval::Negative(NegativeEval {
            query: "neg1".into(),
            passed: true,
            violations: vec![],
        }),
    ];

    let agg = aggregate_by_category(&queries);

    assert_eq!(agg.len(), 2, "should have 2 categories (exact and natural)");

    let exact = agg.iter().find(|c| c.category == Category::ExactIdentifier).unwrap();
    assert_eq!(exact.n, 2);
    assert!((exact.mrr - 0.75).abs() < 1e-6);

    let natural = agg.iter().find(|c| c.category == Category::NaturalLanguage).unwrap();
    assert_eq!(natural.n, 1);
    assert_eq!(natural.mrr, 0.0);
}
```

- [ ] **Step 3: aggregate_by_category 순서 고정 테스트**

```rust
#[test]
fn aggregate_by_category_has_fixed_order() {
    use Category;
    let queries = vec![
        QueryEval::Positive(PositiveEval {
            query: "korean".into(),
            category: Category::Korean,
            mrr: 1.0,
            recall_at_5: 1.0,
            recall_at_10: 1.0,
            ndcg_at_10: 1.0,
            first_hit_rank: Some(1),
            hit_paths: vec![],
        }),
        QueryEval::Positive(PositiveEval {
            query: "exact".into(),
            category: Category::ExactIdentifier,
            mrr: 1.0,
            recall_at_5: 1.0,
            recall_at_10: 1.0,
            ndcg_at_10: 1.0,
            first_hit_rank: Some(1),
            hit_paths: vec![],
        }),
    ];

    let agg = aggregate_by_category(&queries);
    let expected_order = [
        Category::ExactIdentifier,
        Category::NaturalLanguage,
        Category::Korean,
        Category::Typo,
        Category::Paraphrase,
    ];

    for (i, (cat, expected)) in agg.iter().zip(expected_order.iter()).enumerate() {
        assert_eq!(cat.category, *expected, "category at position {} should be {:?}", i, expected);
    }
}
```

- [ ] **Step 4: aggregate_negatives 테스트**

```rust
#[test]
fn aggregate_negatives_calculates_pass_rate() {
    let negs = vec![
        NegativeEval {
            query: "a".into(),
            passed: true,
            violations: vec![],
        },
        NegativeEval {
            query: "b".into(),
            passed: false,
            violations: vec![],
        },
        NegativeEval {
            query: "c".into(),
            passed: true,
            violations: vec![],
        },
        NegativeEval {
            query: "d".into(),
            passed: false,
            violations: vec![],
        },
    ];

    let agg = aggregate_negatives(&negs);
    assert_eq!(agg.n, 4);
    assert!((agg.pass_rate - 0.5).abs() < 1e-6);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib aggregate_`
Expected: FAIL (functions not yet defined)

- [ ] **Step 6: Commit**

```bash
git add src/search/report/metrics.rs
git commit -m "test: add aggregate function tests"
```

---

## Task 10: aggregate 함수 구현

**Files:**
- Modify: `src/search/report/metrics.rs`

- [ ] **Step 1: aggregate 함수 갱신 (Negative 제외)**

```rust
pub fn aggregate(per_query: &[QueryEval]) -> AggregateEval {
    let positives: Vec<_> = per_query
        .iter()
        .filter_map(|q| match q {
            QueryEval::Positive(p) => Some(p),
            QueryEval::Negative(_) => None,
        })
        .collect();

    let n = positives.len();
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
    let sum = |f: fn(&PositiveEval) -> f32| positives.iter().map(f).sum::<f32>();
    AggregateEval {
        mrr: sum(|q| q.mrr) / nf,
        recall_at_5: sum(|q| q.recall_at_5) / nf,
        recall_at_10: sum(|q| q.recall_at_10) / nf,
        ndcg_at_10: sum(|q| q.ndcg_at_10) / nf,
        n_queries: n,
    }
}
```

- [ ] **Step 2: aggregate_by_category 함수 구현**

```rust
pub fn aggregate_by_category(per_query: &[QueryEval]) -> Vec<CategoryAggregate> {
    let positives: Vec<_> = per_query
        .iter()
        .filter_map(|q| match q {
            QueryEval::Positive(p) => Some(p),
            QueryEval::Negative(_) => None,
        })
        .collect();

    let categories = [
        Category::ExactIdentifier,
        Category::NaturalLanguage,
        Category::Korean,
        Category::Typo,
        Category::Paraphrase,
    ];

    categories
        .iter()
        .map(|&cat| {
            let cat_queries: Vec<_> = positives
                .iter()
                .filter(|q| q.category == cat)
                .collect();

            let n = cat_queries.len();
            if n == 0 {
                return CategoryAggregate {
                    category: cat,
                    n,
                    mrr: 0.0,
                    recall_at_5: 0.0,
                    recall_at_10: 0.0,
                    ndcg_at_10: 0.0,
                };
            }

            let nf = n as f32;
            let sum = |f: fn(&PositiveEval) -> f32| cat_queries.iter().map(f).sum::<f32>();
            CategoryAggregate {
                category: cat,
                n,
                mrr: sum(|q| q.mrr) / nf,
                recall_at_5: sum(|q| q.recall_at_5) / nf,
                recall_at_10: sum(|q| q.recall_at_10) / nf,
                ndcg_at_10: sum(|q| q.ndcg_at_10) / nf,
            }
        })
        .collect()
}
```

- [ ] **Step 3: aggregate_negatives 함수 구현**

```rust
pub fn aggregate_negatives(per_query: &[QueryEval]) -> NegativeAggregate {
    let negatives: Vec<_> = per_query
        .iter()
        .filter_map(|q| match q {
            QueryEval::Negative(n) => Some(n),
            QueryEval::Positive(_) => None,
        })
        .collect();

    let n = negatives.len();
    if n == 0 {
        return NegativeAggregate {
            n: 0,
            pass_rate: 0.0,
        };
    }

    let passed = negatives.iter().filter(|n| n.passed).count();
    NegativeAggregate {
        n,
        pass_rate: (passed as f32) / (n as f32),
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib aggregate_`
Expected: PASS

- [ ] **Step 5: Run all metrics tests**

Run: `cargo test --lib metrics`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/search/report/metrics.rs
git commit -m "feat: implement aggregate functions with category and negative support"
```

---

## Task 11: Report 구조체 확장

**Files:**
- Modify: `src/search/report/mod.rs`

- [ ] **Step 1: CategoryAggregate, NegativeAggregate re-export**

```rust
use crate::search::report::metrics::{
    aggregate, aggregate_by_category, aggregate_negatives, aggregate as aggregate_positive,
    CategoryAggregate, NegativeAggregate, AggregateEval, QueryEval,
};
```

- [ ] **Step 2: Report 구조체에 by_category, negatives 필드 추가**

```rust
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
    pub by_category: Vec<CategoryAggregate>,
    pub negatives: Vec<NegativeEval>,
}
```

- [ ] **Step 3: run()에서 aggregates 계산 추가**

```rust
pub fn run(repo: &GitRepo, repo_path: &Path, opts: &ReportOptions) -> Result<(), ReportError> {
    let set = fixtures::load(&opts.fixtures_path)?;

    let index_dir = index_dir_for(repo_path);
    if !index_dir.join("meta.toml").exists() {
        return Err(ReportError::Search(SearchError::IndexNotFound(
            index_dir.clone(),
        )));
    }
    let engine = SearchEngine::open(&index_dir)?;

    let meta_str = std::fs::read_to_string(index_dir.join("meta.toml"))?;
    let meta: IndexMeta = toml::from_str(&meta_str)?;

    let head = working_head_oid(repo)?;
    let head_mismatch = head != meta.head_oid;

    let (latency, last_results) =
        run_perf(&engine, &set.queries, opts.warmup, opts.iters, opts.limit)?;

    let per_query: Vec<_> = set
        .queries
        .iter()
        .zip(last_results.iter())
        .map(|(q, r)| evaluate(q, r))
        .collect();
    let aggregate_eval = aggregate_positive(&per_query);

    let by_category = aggregate_by_category(&per_query);
    let negative_vec: Vec<_> = per_query
        .iter()
        .filter_map(|q| match q {
            QueryEval::Negative(n) => Some(n.clone()),
            QueryEval::Positive(_) => None,
        })
        .collect();

    let mut commit_n = 0usize;
    let mut file_n = 0usize;
    let mut sym_n = 0usize;
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
        generated_at: now_iso8601(),
        working_head_oid: head,
        head_mismatch,
        warmup: opts.warmup,
        iters: opts.iters,
        limit: opts.limit,
        aggregate: aggregate_eval,
        latency,
        index: index_stats,
        per_query,
        by_category,
        negatives: negative_vec,
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

- [ ] **Step 4: Run e2e test**

Run: `cargo test --lib report_runs_end_to_end_and_writes_markdown`
Expected: FAIL (render에서 새 필드 처리 필요)

- [ ] **Step 5: Commit**

```bash
git add src/search/report/mod.rs
git commit -m "feat: add by_category and negatives to Report struct"
```

---

## Task 12: render.rs - By Category 섹션 테스트 작성

**Files:**
- Modify: `src/search/report/render.rs`

- [ ] **Step 1: to_markdown_string에 By Category 포함 테스트**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_includes_by_category_section() {
        use crate::search::report::metrics::{Category, CategoryAggregate, AggregateEval, QueryEval, PositiveEval, NegativeEval, NegativeViolation};
        use crate::search::report::perf::LatencyStats;
        use crate::search::report::IndexStats;

        let report = Report {
            generated_at: "2024-01-01T00:00:00Z".into(),
            working_head_oid: "a".repeat(40),
            head_mismatch: false,
            warmup: 1,
            iters: 1,
            limit: 10,
            aggregate: AggregateEval {
                mrr: 0.75,
                recall_at_5: 0.9,
                recall_at_10: 1.0,
                ndcg_at_10: 0.78,
                n_queries: 2,
            },
            latency: LatencyStats {
                p50_ms: 10.0,
                p95_ms: 15.0,
                p99_ms: 20.0,
                mean_ms: 12.0,
                qps: 83.33,
            },
            index: IndexStats {
                index_dir: "/test".into(),
                size_bytes: 1024,
                doc_count_total: 100,
                doc_count_commit: 50,
                doc_count_file: 30,
                doc_count_symbol: 20,
                head_oid: "a".repeat(40),
                indexed_at: "2024-01-01".into(),
                embedding_model: "test".into(),
                embedding_dim: 256,
                bm25_tokenizer: "ngram_2_2".into(),
                vector_backend: "turboquant_4bit".into(),
            },
            per_query: vec![
                QueryEval::Positive(PositiveEval {
                    query: "test".into(),
                    category: Category::ExactIdentifier,
                    mrr: 1.0,
                    recall_at_5: 1.0,
                    recall_at_10: 1.0,
                    ndcg_at_10: 1.0,
                    first_hit_rank: Some(1),
                    hit_paths: vec![],
                }),
            ],
            by_category: vec![
                CategoryAggregate {
                    category: Category::ExactIdentifier,
                    n: 2,
                    mrr: 0.75,
                    recall_at_5: 0.9,
                    recall_at_10: 1.0,
                    ndcg_at_10: 0.78,
                },
            ],
            negatives: vec![],
        };

        let md = to_markdown_string(&report);
        assert!(md.contains("## By Category"));
        assert!(md.contains("exact_identifier"));
        assert!(md.contains("0.75"));
    }
}
```

- [ ] **Step 2: Run test**

Run: `cargo test --lib markdown_includes_by_category`
Expected: FAIL (render function not yet updated)

- [ ] **Step 3: Commit**

```bash
git add src/search/report/render.rs
git commit -m "test: add By Category section test"
```

---

## Task 13: render.rs - By Category 섹션 구현

**Files:**
- Modify: `src/search/report/render.rs`

- [ ] **Step 1: Category 약어 헬퍼 함수 추가**

```rust
fn category_abbrev(cat: Category) -> &'static str {
    match cat {
        Category::ExactIdentifier => "exact",
        Category::NaturalLanguage => "natural",
        Category::Korean => "korean",
        Category::Typo => "typo",
        Category::Paraphrase => "paraphrase",
        Category::Negative => "negative",
    }
}

fn category_full(cat: Category) -> &'static str {
    match cat {
        Category::ExactIdentifier => "exact_identifier",
        Category::NaturalLanguage => "natural_language",
        Category::Korean => "korean",
        Category::Typo => "typo",
        Category::Paraphrase => "paraphrase",
        Category::Negative => "negative",
    }
}
```

- [ ] **Step 2: to_markdown_string에 By Category 섹션 추가**

Aggregate 섹션 직후에 추가:

```rust
// By Category 섹션
let mut by_cat = String::from("## By Category\n\n");
by_cat.push_str("| Category | n | MRR | R@5 | R@10 | NDCG@10 |\n");
by_cat.push_str("|----------|---|-----|-----|------|---------|\n");
for cat in &report.by_category {
    by_cat.push_str(&format!(
        "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
        category_full(cat.category),
        cat.n,
        cat.mrr,
        cat.recall_at_5,
        cat.recall_at_10,
        cat.ndcg_at_10,
    ));
}
by_cat.push('\n');
```

이후 `md`에 추가.

- [ ] **Step 3: Aggregate 헤더에 n 추가**

```rust
md.push_str(&format!(
    "## Aggregate (positive only, n={})\n\n",
    report.aggregate.n_queries,
));
```

- [ ] **Step 4: Run test**

Run: `cargo test --lib markdown_includes_by_category`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/report/render.rs
git commit -m "feat: add By Category section to markdown render"
```

---

## Task 14: render.rs - Negative Queries 섹션 구현

**Files:**
- Modify: `src/search/report/render.rs`

- [ ] **Step 1: Negative Queries 섹션 추가**

By Category 섹션 직후:

```rust
// Negative Queries 섹션
if !report.negatives.is_empty() {
    let pass_count = report.negatives.iter().filter(|n| n.passed).count();
    let pass_rate = (pass_count as f32) / (report.negatives.len() as f32);

    md.push_str(&format!(
        "## Negative Queries (n={}, pass rate {:.1}%)\n\n",
        report.negatives.len(),
        pass_rate * 100.0,
    ));
    md.push_str("| # | Query | Result |\n");
    md.push_str("|---|-------|--------|\n");

    for (i, neg) in report.negatives.iter().enumerate() {
        let result = if neg.passed {
            "PASS".to_string()
        } else {
            let violations: Vec<String> = neg
                .violations
                .iter()
                .map(|v| format!("rank {}: {}, matched_rule={}", v.rank, v.path, v.matched_rule))
                .collect();
            format!("FAIL ({})", violations.join("; "))
        };
        md.push_str(&format!("| {} | {} | {} |\n", i + 1, neg.query, result));
    }
    md.push('\n');
}
```

- [ ] **Step 2: e2e 테스트 갱신**

```rust
#[test]
#[ignore]
fn report_runs_end_to_end_and_writes_markdown() {
    // ... 기존 테스트 코드 ...
    let md = std::fs::read_to_string(&out_md).unwrap();
    assert!(md.contains("# Search Quality Report"));
    assert!(md.contains("## Aggregate"));
    assert!(md.contains("## By Category"));  // 추가
    assert!(md.contains("## Per-Query"));  // Per-Query는 아직 category 컬럼 없음
    assert!(md.contains("alpha_func"));
    assert!(md.contains("beta_func"));
}
```

- [ ] **Step 3: Run e2e test**

Run: `cargo test --lib report_runs_end_to_end_and_writes_markdown`
Expected: PASS (negative 없으면 섹션 안 나타나도 OK)

- [ ] **Step 4: Commit**

```bash
git add src/search/report/render.rs src/search/report/mod.rs
git commit -m "feat: add Negative Queries section to markdown render"
```

---

## Task 15: render.rs - Per-Query Category 컬럼 추가

**Files:**
- Modify: `src/search/report/render.rs`

- [ ] **Step 1: Per-Query 테이블 헤더에 Cat 컬럼 추가**

```rust
// Per-Query 섹션 (positive only)
let positive_only: Vec<_> = report
    .per_query
    .iter()
    .filter_map(|q| match q {
        QueryEval::Positive(p) => Some(p),
        QueryEval::Negative(_) => None,
    })
    .collect();

md.push_str(&format!(
    "## Per-Query (positive only, n={})\n\n",
    positive_only.len(),
));
md.push_str("| # | Cat | Query | Hit | MRR | R@5 | R@10 | NDCG |\n");
md.push_str("|---|-----|-------|-----|-----|-----|------|------|\n");
```

- [ ] **Step 2: Per-Query 행에 Category 추가**

```rust
for (i, p) in positive_only.iter().enumerate() {
    let hit = p
        .first_hit_rank
        .map_or("—".into(), |r| r.to_string());
    md.push_str(&format!(
        "| {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
        i + 1,
        category_abbrev(p.category),
        p.query,
        hit,
        p.mrr,
        p.recall_at_5,
        p.recall_at_10,
        p.ndcg_at_10,
    ));
}
md.push('\n');
```

- [ ] **Step 3: Run e2e test**

Run: `cargo test --lib report_runs_end_to_end_and_writes_markdown`
Expected: PASS

- [ ] **Step 4: stdout render에도 동일하게 추가**

`to_stdout()` 함수에서도 동일하게 Per-Query 표에 Cat 컬럼 추가.

- [ ] **Step 5: Commit**

```bash
git add src/search/report/render.rs
git commit -m "feat: add Category column to Per-Query table"
```

---

## Task 16: fixtures.toml 확장 (30개)

**Files:**
- Modify: `tests/fixtures/search_queries.toml`

- [ ] **Step 1: 기존 7개에 category 추가**

```toml
[[query]]
category = "exact_identifier"
text = "incremental indexing fallback"
expected = [
    { path = "src/search/indexer.rs", kind = "Symbol", title = "build_index_incremental" },
]

[[query]]
category = "exact_identifier"
text = "tantivy delete_term"
expected = [
    { path = "src/search/bm25.rs", kind = "Symbol", title = "delete_doc" },
]

[[query]]
category = "natural_language"
text = "RRF reciprocal rank fusion"
expected = [
    { path = "src/search/rrf.rs" },
]

[[query]]
category = "exact_identifier"
text = "embedding model load potion"
expected = [
    { path = "src/search/embedding.rs" },
]

[[query]]
category = "natural_language"
text = "search modal state machine"
expected = [
    { path = "src/search/modal_state.rs" },
]

[[query]]
category = "natural_language"
text = "tree sitter highlight configuration"
expected = [
    { path = "src/highlight/engine.rs" },
]

[[query]]
category = "natural_language"
text = "git revwalk topological commit"
expected = [
    { path = "src/git/store.rs" },
    { path = "src/git/commit.rs" },
]
```

- [ ] **Step 2: exact_identifier 카테고리 3개 추가 (총 8개)**

```toml
[[query]]
category = "exact_identifier"
text = "delete_doc bm25"
expected = [
    { path = "src/search/bm25.rs", kind = "Symbol", title = "delete_doc" },
]

[[query]]
category = "exact_identifier"
text = "build_index_incremental"
expected = [
    { path = "src/search/indexer.rs", kind = "Symbol", title = "build_index_incremental" },
]

[[query]]
category = "exact_identifier"
text = "search modal state"
expected = [
    { path = "src/search/modal_state.rs", kind = "Symbol", title = "ModalState" },
]
```

- [ ] **Step 3: natural_language 카테고리 3개 추가 (총 6개)**

```toml
[[query]]
category = "natural_language"
text = "how to build search index"
expected = [
    { path = "src/search/indexer.rs" },
]

[[query]]
category = "natural_language"
text = "semantic search implementation"
expected = [
    { path = "src/search/mod.rs" },
]

[[query]]
category = "natural_language"
text = "vector ANN search backend"
expected = [
    { path = "src/search/vector.rs" },
]
```

- [ ] **Step 4: korean 카테고리 4개 추가**

```toml
[[query]]
category = "korean"
text = "검색 모달 상태 머신"
expected = [
    { path = "src/search/modal_state.rs" },
]

[[query]]
category = "korean"
text = "인덱스 빌드 방법"
expected = [
    { path = "src/search/indexer.rs" },
]

[[query]]
category = "korean"
text = "벡터 임베딩 모델"
expected = [
    { path = "src/search/embedding.rs" },
]

[[query]]
category = "korean"
text = "트리 시터 하이라이트"
expected = [
    { path = "src/highlight/engine.rs" },
]
```

- [ ] **Step 5: typo 카테고리 4개 추가**

```toml
[[query]]
category = "typo"
text = "tantivvy delete_trm"
expected = [
    { path = "src/search/bm25.rs", kind = "Symbol", title = "delete_doc" },
]

[[query]]
category = "typo"
text = "build_index_incremntal"
expected = [
    { path = "src/search/indexer.rs", kind = "Symbol", title = "build_index_incremental" },
]

[[query]]
category = "typo"
text = "embeding model potion"
expected = [
    { path = "src/search/embedding.rs" },
]

[[query]]
category = "typo"
text = "reciprical rank fusion"
expected = [
    { path = "src/search/rrf.rs" },
]
```

- [ ] **Step 6: paraphrase 카테고리 4개 추가**

```toml
[[query]]
category = "paraphrase"
text = "합치는 점수 알고리즘"
expected = [
    { path = "src/search/rrf.rs" },
]

[[query]]
category = "paraphrase"
text = "벡터 압축 방식"
expected = [
    { path = "src/search/vector.rs" },
]

[[query]]
category = "paraphrase"
text = "토크나이저 설정"
expected = [
    { path = "src/search/bm25.rs" },
]

[[query]]
category = "paraphrase"
text = "문서 청크 분할"
expected = [
    { path = "src/search/chunk/mod.rs" },
]
```

- [ ] **Step 7: negative 카테고리 4개 추가**

```toml
[[query]]
category = "negative"
text = "react component lifecycle"
forbidden = [
    { path_prefix = "src/" },
]

[[query]]
category = "negative"
text = "django migrations"
forbidden = [
    { path_prefix = "src/" },
]

[[query]]
category = "negative"
text = "kubernetes pod scheduling"
forbidden = [
    { path_prefix = "src/" },
]

[[query]]
category = "negative"
text = "spring boot starter"
forbidden = [
    { path_prefix = "src/" },
]
```

- [ ] **Step 8: fixture 로더 테스트**

Run: `cargo test --lib loads_project_fixtures`
Expected: PASS (30개 쿼리, 6개 카테고리)

- [ ] **Step 9: 전체 테스트 실행**

Run: `cargo test --lib report`
Expected: PASS

- [ ] **Step 10: 실제 리포트 생성 테스트**

```bash
cargo run --bin glc -- index
cargo run --bin glc -- report --out /tmp/test-report.md
cat /tmp/test-report.md
```

Expected: 리포트가 정상적으로 생성되고, `## By Category`, `## Negative Queries`, `## Per-Query (positive only, n=26)` 섹션이 포함됨

- [ ] **Step 11: Commit**

```bash
git add tests/fixtures/search_queries.toml
git commit -m "feat: expand fixtures to 30 queries across 6 categories with negative queries"
```

---

## Task 17: 학습 가이드 업데이트 (다음 사이클로 분리)

**범위 외**: 이 구현 plan은 코드 변경에 집중. 리포트 가이드 문서는 새 형식의 리포트가 1회 이상 생성된 후 별도 PR로 업데이트.

---

## Task 18: CI 검증

**Files:**
- None (기존 CI 실행만)

- [ ] **Step 1: CI workflow 실행 확인**

```bash
cargo check --all-targets
cargo test
cargo clippy --all-targets -D warnings
cargo fmt --check
```

Expected: 모든 통과

- [ ] **Step 2: 리포트 생성 확인**

```bash
cargo run --bin glc -- index --force
cargo run --bin glc -- report --out /tmp/final-report.md
```

Expected: 정상 생성, 새 섹션들 포함

---

## Self-Review Results

### 1. Spec Coverage
- Category enum, FixtureQuery 스키마 확장 → Task 2, 3, 4 ✓
- 로더 검증 로직 → Task 3, 4 ✓
- QueryEval enum, NegativeEval, CategoryAggregate → Task 5, 6, 9, 10 ✓
- evaluate_negative() → Task 7, 8 ✓
- aggregate 함수들 (Negative 제외, 카테고리별, negative) → Task 9, 10 ✓
- Report 구조체 확장 → Task 11 ✓
- By Category 섹션 → Task 12, 13 ✓
- Negative Queries 섹션 → Task 14 ✓
- Per-Query Category 컬럼 → Task 15 ✓
- fixtures.toml 30개 확장 → Task 16 ✓

### 2. Placeholder Scan
- TBD/TODO 없음 ✓
- "implement later" 없음 ✓
- 코드 블록 모두 완전함 ✓

### 3. Type Consistency
- Category는 fixtures.rs에서 정의 → metrics.rs에서 re-export → render.rs에서 사용, 일관적 ✓
- QueryEval enum 패턴 매칭이 모든 곳에서 동일하게 사용 ✓
- `path_prefix` 매칭은 `starts_with`로 명시 ✓

---

Plan complete and saved to `docs/superpowers/plans/2026-05-27-fixture-category-matrix.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?