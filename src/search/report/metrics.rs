//! 검색 품질 메트릭 — MRR, Recall@k, NDCG@k.

use crate::search::report::fixtures::{ExpectedHit, FixtureQuery};
#[cfg(test)]
use crate::search::report::fixtures::Category;
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
    let recall_at_5 = (hit_count_at_5.min(query.expected.len()) as f32) / (n_expected as f32);
    let recall_at_10 = (hit_count_at_10.min(query.expected.len()) as f32) / (n_expected as f32);

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
            category: Category::ExactIdentifier,
            expected,
            forbidden: vec![],
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
        let q = fq("x", vec![eh("src/a.rs"), eh("src/b.rs")]);
        let res = vec![
            result(1, DocKind::Symbol, "src/a.rs", "f1 (src/a.rs)"),
            result(2, DocKind::Symbol, "src/a.rs", "f2 (src/a.rs)"),
            result(3, DocKind::File, "src/b.rs", "src/b.rs"),
        ];
        let e = evaluate(&q, &res);
        assert_eq!(
            e.hit_paths,
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
