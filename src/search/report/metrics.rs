//! 검색 품질 메트릭 — MRR, Recall@k, NDCG@k + negative-query pass/fail.

use crate::search::report::fixtures::{Category, ExpectedHit, FixtureQuery};
use crate::search::SearchResult;

#[derive(Debug, Clone)]
pub enum QueryEval {
    Positive(PositiveEval),
    Negative(NegativeEval),
}

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

#[derive(Debug, Clone)]
pub struct NegativeEval {
    pub query: String,
    pub passed: bool,
    pub violations: Vec<NegativeViolation>,
}

#[derive(Debug, Clone)]
pub struct NegativeViolation {
    pub rank: usize,
    pub path: String,
    pub matched_rule: String,
}

#[derive(Debug, Clone)]
pub struct AggregateEval {
    pub mrr: f32,
    pub recall_at_5: f32,
    pub recall_at_10: f32,
    pub ndcg_at_10: f32,
    pub n_queries: usize,
}

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

const POSITIVE_CATEGORIES: [Category; 5] = [
    Category::ExactIdentifier,
    Category::NaturalLanguage,
    Category::Korean,
    Category::Typo,
    Category::Paraphrase,
];

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

fn is_relevant(expected: &[ExpectedHit], hit: &SearchResult) -> bool {
    expected.iter().any(|e| matches(e, hit))
}

fn evaluate_positive(query: &FixtureQuery, results: &[SearchResult]) -> PositiveEval {
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

fn evaluate_negative(query: &FixtureQuery, results: &[SearchResult]) -> NegativeEval {
    let mut violations = Vec::new();
    for (i, r) in results.iter().take(10).enumerate() {
        let rank = i + 1;
        let Some(hit_path) = r.meta.path.as_deref() else {
            continue;
        };
        for rule in &query.forbidden {
            let (matched, rule_desc) = if let Some(exact) = rule.path.as_deref() {
                (hit_path == exact, format!("path={exact}"))
            } else if let Some(prefix) = rule.path_prefix.as_deref() {
                (
                    hit_path.starts_with(prefix),
                    format!("path_prefix={prefix}"),
                )
            } else {
                continue;
            };
            if matched {
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

pub fn evaluate(query: &FixtureQuery, results: &[SearchResult]) -> QueryEval {
    match query.category {
        Category::Negative => QueryEval::Negative(evaluate_negative(query, results)),
        _ => QueryEval::Positive(evaluate_positive(query, results)),
    }
}

pub fn aggregate(per_query: &[QueryEval]) -> AggregateEval {
    let positives: Vec<&PositiveEval> = per_query
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
    let sum = |f: fn(&PositiveEval) -> f32| positives.iter().copied().map(f).sum::<f32>();
    AggregateEval {
        mrr: sum(|q| q.mrr) / nf,
        recall_at_5: sum(|q| q.recall_at_5) / nf,
        recall_at_10: sum(|q| q.recall_at_10) / nf,
        ndcg_at_10: sum(|q| q.ndcg_at_10) / nf,
        n_queries: n,
    }
}

pub fn aggregate_by_category(per_query: &[QueryEval]) -> Vec<CategoryAggregate> {
    let positives: Vec<&PositiveEval> = per_query
        .iter()
        .filter_map(|q| match q {
            QueryEval::Positive(p) => Some(p),
            QueryEval::Negative(_) => None,
        })
        .collect();

    POSITIVE_CATEGORIES
        .iter()
        .map(|&cat| {
            let bucket: Vec<&&PositiveEval> =
                positives.iter().filter(|q| q.category == cat).collect();
            let n = bucket.len();
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
            let sum = |f: fn(&PositiveEval) -> f32| bucket.iter().map(|q| f(q)).sum::<f32>();
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

pub fn aggregate_negatives(per_query: &[QueryEval]) -> NegativeAggregate {
    let negatives: Vec<&NegativeEval> = per_query
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
    let passed = negatives.iter().filter(|x| x.passed).count();
    NegativeAggregate {
        n,
        pass_rate: (passed as f32) / (n as f32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::report::fixtures::ForbiddenRule;
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
            category: Category::ExactIdentifier,
            expected,
            forbidden: vec![],
        }
    }

    fn fq_cat(text: &str, category: Category, expected: Vec<ExpectedHit>) -> FixtureQuery {
        FixtureQuery {
            text: text.to_string(),
            category,
            expected,
            forbidden: vec![],
        }
    }

    fn fq_negative(text: &str, forbidden: Vec<ForbiddenRule>) -> FixtureQuery {
        FixtureQuery {
            text: text.to_string(),
            category: Category::Negative,
            expected: vec![],
            forbidden,
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

    fn unwrap_positive(e: QueryEval) -> PositiveEval {
        match e {
            QueryEval::Positive(p) => p,
            QueryEval::Negative(_) => panic!("expected Positive variant"),
        }
    }

    fn unwrap_negative(e: QueryEval) -> NegativeEval {
        match e {
            QueryEval::Negative(n) => n,
            QueryEval::Positive(_) => panic!("expected Negative variant"),
        }
    }

    #[test]
    fn first_rank_and_mrr() {
        let q = fq("x", vec![eh("src/a.rs")]);
        let res = vec![
            result(1, DocKind::File, "src/b.rs", "src/b.rs"),
            result(2, DocKind::File, "src/a.rs", "src/a.rs"),
            result(3, DocKind::File, "src/c.rs", "src/c.rs"),
        ];
        let p = unwrap_positive(evaluate(&q, &res));
        assert_eq!(p.first_hit_rank, Some(2));
        assert!((p.mrr - 0.5).abs() < 1e-6);
    }

    #[test]
    fn no_match_gives_zero() {
        let q = fq("x", vec![eh("src/a.rs")]);
        let res = vec![result(1, DocKind::File, "src/b.rs", "src/b.rs")];
        let p = unwrap_positive(evaluate(&q, &res));
        assert_eq!(p.first_hit_rank, None);
        assert_eq!(p.mrr, 0.0);
        assert_eq!(p.recall_at_5, 0.0);
        assert_eq!(p.recall_at_10, 0.0);
        assert_eq!(p.ndcg_at_10, 0.0);
    }

    #[test]
    fn recall_at_k_counts_distinct_expected_hits() {
        let q = fq("x", vec![eh("src/a.rs"), eh("src/b.rs")]);
        let res = vec![
            result(1, DocKind::File, "src/a.rs", "src/a.rs"),
            result(2, DocKind::File, "src/b.rs", "src/b.rs"),
            result(3, DocKind::File, "src/c.rs", "src/c.rs"),
        ];
        let p = unwrap_positive(evaluate(&q, &res));
        assert!((p.recall_at_10 - 1.0).abs() < 1e-6);
        assert!((p.recall_at_5 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn same_path_multiple_chunks_counts_once() {
        let q = fq("x", vec![eh("src/a.rs"), eh("src/b.rs")]);
        let res = vec![
            result(1, DocKind::Symbol, "src/a.rs", "f1 (src/a.rs)"),
            result(2, DocKind::Symbol, "src/a.rs", "f2 (src/a.rs)"),
            result(3, DocKind::File, "src/b.rs", "src/b.rs"),
        ];
        let p = unwrap_positive(evaluate(&q, &res));
        assert_eq!(
            p.hit_paths,
            vec!["src/a.rs".to_string(), "src/b.rs".to_string()]
        );
    }

    #[test]
    fn matches_symbol_title_strips_path_suffix() {
        let q = fq(
            "x",
            vec![eh_full(
                "src/a.rs",
                DocKind::Symbol,
                "build_index_incremental",
            )],
        );
        let res = vec![result(
            1,
            DocKind::Symbol,
            "src/a.rs",
            "build_index_incremental (src/a.rs)",
        )];
        let p = unwrap_positive(evaluate(&q, &res));
        assert_eq!(p.first_hit_rank, Some(1));
    }

    #[test]
    fn kind_filter_rejects_wrong_kind() {
        let q = fq("x", vec![eh_kind("src/a.rs", DocKind::Symbol)]);
        let res = vec![result(1, DocKind::File, "src/a.rs", "src/a.rs")];
        let p = unwrap_positive(evaluate(&q, &res));
        assert_eq!(p.first_hit_rank, None);
    }

    #[test]
    fn ndcg_perfect_when_top_matches() {
        let q = fq("x", vec![eh("src/a.rs"), eh("src/b.rs")]);
        let res = vec![
            result(1, DocKind::File, "src/a.rs", "src/a.rs"),
            result(2, DocKind::File, "src/b.rs", "src/b.rs"),
        ];
        let p = unwrap_positive(evaluate(&q, &res));
        assert!((p.ndcg_at_10 - 1.0).abs() < 1e-6, "got {}", p.ndcg_at_10);
    }

    #[test]
    fn aggregate_averages_metrics_excluding_negatives() {
        let q1 = QueryEval::Positive(PositiveEval {
            query: "a".into(),
            category: Category::ExactIdentifier,
            mrr: 1.0,
            recall_at_5: 1.0,
            recall_at_10: 1.0,
            ndcg_at_10: 1.0,
            first_hit_rank: Some(1),
            hit_paths: vec![],
        });
        let q2 = QueryEval::Positive(PositiveEval {
            query: "b".into(),
            category: Category::NaturalLanguage,
            mrr: 0.0,
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            ndcg_at_10: 0.0,
            first_hit_rank: None,
            hit_paths: vec![],
        });
        let q3 = QueryEval::Negative(NegativeEval {
            query: "n".into(),
            passed: true,
            violations: vec![],
        });
        let agg = aggregate(&[q1, q2, q3]);
        assert_eq!(agg.n_queries, 2, "negatives must be excluded");
        assert!((agg.mrr - 0.5).abs() < 1e-6);
        assert!((agg.ndcg_at_10 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn evaluate_negative_passes_without_violations() {
        let q = fq_negative(
            "react component lifecycle",
            vec![ForbiddenRule {
                path: None,
                path_prefix: Some("src/".into()),
            }],
        );
        let res = vec![result(
            1,
            DocKind::Commit,
            "irrelevant",
            "commit message irrelevant",
        )]
        .into_iter()
        .map(|mut r| {
            r.meta.path = None;
            r
        })
        .collect::<Vec<_>>();
        let n = unwrap_negative(evaluate(&q, &res));
        assert!(n.passed);
        assert!(n.violations.is_empty());
    }

    #[test]
    fn evaluate_negative_fails_on_path_prefix() {
        let q = fq_negative(
            "react component lifecycle",
            vec![ForbiddenRule {
                path: None,
                path_prefix: Some("src/".into()),
            }],
        );
        let res = vec![
            result(1, DocKind::File, "README.md", "README.md"),
            result(2, DocKind::File, "src/search/indexer.rs", "indexer.rs"),
        ];
        let n = unwrap_negative(evaluate(&q, &res));
        assert!(!n.passed);
        assert_eq!(n.violations.len(), 1);
        assert_eq!(n.violations[0].rank, 2);
        assert_eq!(n.violations[0].matched_rule, "path_prefix=src/");
    }

    #[test]
    fn evaluate_negative_fails_on_exact_path() {
        let q = fq_negative(
            "django migrations",
            vec![ForbiddenRule {
                path: Some("src/main.rs".into()),
                path_prefix: None,
            }],
        );
        let res = vec![result(1, DocKind::File, "src/main.rs", "main.rs")];
        let n = unwrap_negative(evaluate(&q, &res));
        assert!(!n.passed);
        assert_eq!(n.violations[0].matched_rule, "path=src/main.rs");
    }

    #[test]
    fn aggregate_by_category_groups_correctly() {
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
                query: "neg".into(),
                passed: true,
                violations: vec![],
            }),
        ];
        let agg = aggregate_by_category(&queries);
        assert_eq!(agg.len(), POSITIVE_CATEGORIES.len());
        let exact = agg
            .iter()
            .find(|c| c.category == Category::ExactIdentifier)
            .unwrap();
        assert_eq!(exact.n, 2);
        assert!((exact.mrr - 0.75).abs() < 1e-6);
        let nat = agg
            .iter()
            .find(|c| c.category == Category::NaturalLanguage)
            .unwrap();
        assert_eq!(nat.n, 1);
        let korean = agg.iter().find(|c| c.category == Category::Korean).unwrap();
        assert_eq!(korean.n, 0);
    }

    #[test]
    fn aggregate_by_category_preserves_order() {
        let queries = vec![QueryEval::Positive(PositiveEval {
            query: "k".into(),
            category: Category::Korean,
            mrr: 1.0,
            recall_at_5: 1.0,
            recall_at_10: 1.0,
            ndcg_at_10: 1.0,
            first_hit_rank: Some(1),
            hit_paths: vec![],
        })];
        let agg = aggregate_by_category(&queries);
        let order: Vec<Category> = agg.iter().map(|c| c.category).collect();
        assert_eq!(order, POSITIVE_CATEGORIES.to_vec());
    }

    #[test]
    fn aggregate_negatives_pass_rate() {
        let queries = vec![
            QueryEval::Negative(NegativeEval {
                query: "a".into(),
                passed: true,
                violations: vec![],
            }),
            QueryEval::Negative(NegativeEval {
                query: "b".into(),
                passed: false,
                violations: vec![NegativeViolation {
                    rank: 1,
                    path: "src/a.rs".into(),
                    matched_rule: "path_prefix=src/".into(),
                }],
            }),
            QueryEval::Negative(NegativeEval {
                query: "c".into(),
                passed: true,
                violations: vec![],
            }),
            QueryEval::Negative(NegativeEval {
                query: "d".into(),
                passed: false,
                violations: vec![],
            }),
            QueryEval::Positive(PositiveEval {
                query: "p".into(),
                category: Category::ExactIdentifier,
                mrr: 1.0,
                recall_at_5: 1.0,
                recall_at_10: 1.0,
                ndcg_at_10: 1.0,
                first_hit_rank: Some(1),
                hit_paths: vec![],
            }),
        ];
        let agg = aggregate_negatives(&queries);
        assert_eq!(agg.n, 4);
        assert!((agg.pass_rate - 0.5).abs() < 1e-6);
    }

    #[test]
    fn category_field_propagates_through_evaluate() {
        let q = fq_cat("x", Category::Korean, vec![eh("src/a.rs")]);
        let res = vec![result(1, DocKind::File, "src/a.rs", "a.rs")];
        let p = unwrap_positive(evaluate(&q, &res));
        assert_eq!(p.category, Category::Korean);
    }
}
