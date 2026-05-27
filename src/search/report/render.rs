//! Report → stdout(comfy-table) / markdown 문자열.

use std::fmt::Write;

use comfy_table::presets::UTF8_FULL;
use comfy_table::{ContentArrangement, Table};
use humansize::{format_size, BINARY};

use crate::search::report::fixtures::Category;
use crate::search::report::metrics::{NegativeEval, PositiveEval, QueryEval};
use crate::search::report::Report;

fn category_abbrev(c: Category) -> &'static str {
    match c {
        Category::ExactIdentifier => "exact",
        Category::NaturalLanguage => "natural",
        Category::Korean => "korean",
        Category::Typo => "typo",
        Category::Paraphrase => "paraphrase",
        Category::Negative => "negative",
    }
}

fn category_full(c: Category) -> &'static str {
    match c {
        Category::ExactIdentifier => "exact_identifier",
        Category::NaturalLanguage => "natural_language",
        Category::Korean => "korean",
        Category::Typo => "typo",
        Category::Paraphrase => "paraphrase",
        Category::Negative => "negative",
    }
}

fn split_positive_negative(qs: &[QueryEval]) -> (Vec<&PositiveEval>, Vec<&NegativeEval>) {
    let mut pos = Vec::new();
    let mut neg = Vec::new();
    for q in qs {
        match q {
            QueryEval::Positive(p) => pos.push(p),
            QueryEval::Negative(n) => neg.push(n),
        }
    }
    (pos, neg)
}

fn negative_result_str(n: &NegativeEval) -> String {
    if n.passed {
        "PASS".into()
    } else {
        let parts: Vec<String> = n
            .violations
            .iter()
            .map(|v| format!("rank {}: {} ({})", v.rank, v.path, v.matched_rule))
            .collect();
        format!("FAIL ({})", parts.join("; "))
    }
}

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

    let _ = writeln!(
        s,
        "## Aggregate (positive only, n={})\n",
        r.aggregate.n_queries
    );
    let _ = writeln!(s, "| Metric | Value |");
    let _ = writeln!(s, "|--------|-------|");
    let _ = writeln!(s, "| MRR | {:.3} |", r.aggregate.mrr);
    let _ = writeln!(s, "| Recall@5 | {:.3} |", r.aggregate.recall_at_5);
    let _ = writeln!(s, "| Recall@10 | {:.3} |", r.aggregate.recall_at_10);
    let _ = writeln!(s, "| NDCG@10 | {:.3} |", r.aggregate.ndcg_at_10);
    let _ = writeln!(s, "| Queries | {} |", r.aggregate.n_queries);
    let _ = writeln!(s);

    let _ = writeln!(s, "## By Category\n");
    let _ = writeln!(s, "| Category | n | MRR | R@5 | R@10 | NDCG@10 |");
    let _ = writeln!(s, "|----------|---|-----|-----|------|---------|");
    for cat in &r.by_category {
        let _ = writeln!(
            s,
            "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} |",
            category_full(cat.category),
            cat.n,
            cat.mrr,
            cat.recall_at_5,
            cat.recall_at_10,
            cat.ndcg_at_10,
        );
    }
    let _ = writeln!(s);

    if !r.negatives.is_empty() {
        let pass_count = r.negatives.iter().filter(|n| n.passed).count();
        let pass_rate = (pass_count as f32) / (r.negatives.len() as f32);
        let _ = writeln!(
            s,
            "## Negative Queries (n={}, pass {:.1}%)\n",
            r.negatives.len(),
            pass_rate * 100.0,
        );
        let _ = writeln!(s, "| # | Query | Result |");
        let _ = writeln!(s, "|---|-------|--------|");
        for (i, n) in r.negatives.iter().enumerate() {
            let _ = writeln!(
                s,
                "| {} | {} | {} |",
                i + 1,
                n.query,
                negative_result_str(n),
            );
        }
        let _ = writeln!(s);
    }

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

    let (positives, _) = split_positive_negative(&r.per_query);
    let _ = writeln!(s, "## Per-Query (positive only, n={})\n", positives.len());
    let _ = writeln!(
        s,
        "| # | Cat | Query | MRR | R@5 | R@10 | NDCG@10 | Hit Rank | Hit Paths |"
    );
    let _ = writeln!(
        s,
        "|---|-----|-------|-----|-----|------|---------|----------|-----------|"
    );
    for (i, p) in positives.iter().enumerate() {
        let rank_str = p
            .first_hit_rank
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        let paths = if p.hit_paths.is_empty() {
            "—".into()
        } else {
            p.hit_paths.join(", ")
        };
        let _ = writeln!(
            s,
            "| {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |",
            i + 1,
            category_abbrev(p.category),
            p.query,
            p.mrr,
            p.recall_at_5,
            p.recall_at_10,
            p.ndcg_at_10,
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
    t.add_row(vec!["Queries".into(), r.aggregate.n_queries.to_string()]);
    println!("Aggregate (positive only, n={}):", r.aggregate.n_queries);
    println!("{t}");
    println!();

    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Category", "n", "MRR", "R@5", "R@10", "NDCG@10"]);
    for cat in &r.by_category {
        t.add_row(vec![
            category_full(cat.category).into(),
            cat.n.to_string(),
            format!("{:.3}", cat.mrr),
            format!("{:.3}", cat.recall_at_5),
            format!("{:.3}", cat.recall_at_10),
            format!("{:.3}", cat.ndcg_at_10),
        ]);
    }
    println!("By Category:");
    println!("{t}");
    println!();

    if !r.negatives.is_empty() {
        let pass_count = r.negatives.iter().filter(|n| n.passed).count();
        let pass_rate = (pass_count as f32) / (r.negatives.len() as f32);
        let mut t = Table::new();
        t.load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec!["#", "Query", "Result"]);
        for (i, n) in r.negatives.iter().enumerate() {
            t.add_row(vec![
                (i + 1).to_string(),
                n.query.clone(),
                negative_result_str(n),
            ]);
        }
        println!(
            "Negative Queries (n={}, pass {:.1}%):",
            r.negatives.len(),
            pass_rate * 100.0,
        );
        println!("{t}");
        println!();
    }

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

    let (positives, _) = split_positive_negative(&r.per_query);
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            "#",
            "Cat",
            "Query",
            "MRR",
            "R@5",
            "R@10",
            "NDCG@10",
            "HitRank",
            "Hit Paths",
        ]);
    for (i, p) in positives.iter().enumerate() {
        let rank_str = p
            .first_hit_rank
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        let paths = if p.hit_paths.is_empty() {
            "—".into()
        } else {
            p.hit_paths.join(", ")
        };
        t.add_row(vec![
            (i + 1).to_string(),
            category_abbrev(p.category).into(),
            p.query.clone(),
            format!("{:.3}", p.mrr),
            format!("{:.3}", p.recall_at_5),
            format!("{:.3}", p.recall_at_10),
            format!("{:.3}", p.ndcg_at_10),
            rank_str,
            paths,
        ]);
    }
    println!("Per-Query (positive only, n={}):", positives.len());
    println!("{t}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::report::metrics::{
        AggregateEval, CategoryAggregate, NegativeEval, NegativeViolation, PositiveEval, QueryEval,
    };
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
                n_queries: 1,
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
            per_query: vec![QueryEval::Positive(PositiveEval {
                query: "q1".into(),
                category: Category::ExactIdentifier,
                mrr: 1.0,
                recall_at_5: 1.0,
                recall_at_10: 1.0,
                ndcg_at_10: 1.0,
                first_hit_rank: Some(1),
                hit_paths: vec!["src/a.rs".into()],
            })],
            by_category: vec![CategoryAggregate {
                category: Category::ExactIdentifier,
                n: 1,
                mrr: 1.0,
                recall_at_5: 1.0,
                recall_at_10: 1.0,
                ndcg_at_10: 1.0,
            }],
            negatives: vec![],
        }
    }

    #[test]
    fn markdown_contains_required_sections() {
        let md = to_markdown_string(&sample_report());
        assert!(md.contains("# Search Quality Report"));
        assert!(md.contains("## Aggregate"));
        assert!(md.contains("## By Category"));
        assert!(md.contains("## Performance"));
        assert!(md.contains("## Index"));
        assert!(md.contains("## Per-Query"));
        assert!(md.contains("MRR"));
        assert!(md.contains("NDCG@10"));
        assert!(md.contains("src/a.rs"));
        assert!(md.contains("exact_identifier"));
        assert!(md.contains("exact "));
    }

    #[test]
    fn markdown_shows_head_mismatch_warning() {
        let mut r = sample_report();
        r.head_mismatch = true;
        let md = to_markdown_string(&r);
        assert!(md.contains("HEAD ≠ index.head_oid"));
    }

    #[test]
    fn markdown_includes_negative_section_when_present() {
        let mut r = sample_report();
        r.negatives = vec![
            NegativeEval {
                query: "react component".into(),
                passed: true,
                violations: vec![],
            },
            NegativeEval {
                query: "django migrations".into(),
                passed: false,
                violations: vec![NegativeViolation {
                    rank: 3,
                    path: "src/main.rs".into(),
                    matched_rule: "path_prefix=src/".into(),
                }],
            },
        ];
        let md = to_markdown_string(&r);
        assert!(md.contains("## Negative Queries"));
        assert!(md.contains("PASS"));
        assert!(md.contains("FAIL"));
        assert!(md.contains("path_prefix=src/"));
    }

    #[test]
    fn markdown_skips_negative_section_when_empty() {
        let md = to_markdown_string(&sample_report());
        assert!(!md.contains("## Negative Queries"));
    }
}
