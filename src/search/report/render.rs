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
    println!("Aggregate:");
    println!("{t}");
    println!();

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

    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            "#",
            "Query",
            "MRR",
            "R@5",
            "R@10",
            "NDCG@10",
            "HitRank",
            "Hit Paths",
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
