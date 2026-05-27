//! `glc diagnose <query>` 서브명령 구현.
//!
//! 한 쿼리에 대해 (1) BM25 ngram 토큰화 결과, (2) BM25 raw 점수 top-N,
//! (3) Vector cosine 유사도 top-N, (4) RRF fused top-N을 stdout에 나란히 찍어
//! 어느 단계에서 신호가 끊기는지 분리해서 보기 위한 도구.

use std::path::Path;

use crate::search::indexer::index_dir_for;
use crate::search::rrf::rrf_fuse;
use crate::search::silence::with_silenced_stdio;
use crate::search::{DocMeta, SearchEngine, SearchError};

pub fn run(repo_path: &Path, query: &str, limit: usize) -> Result<(), SearchError> {
    let index_dir = index_dir_for(repo_path);
    let engine = with_silenced_stdio(|| SearchEngine::open(&index_dir))?;

    println!("# Diagnose: {:?}", query);
    println!();
    println!("- Index dir: {}", index_dir.display());
    println!("- Docs: {}", engine.doc_store.len());
    println!();

    print_tokens(&engine, query);
    let bm25_hits = print_bm25(&engine, query, limit)?;
    let vec_hits = print_vector(&engine, query, limit)?;
    print_rrf(&engine, &bm25_hits, &vec_hits, limit);

    Ok(())
}

fn print_tokens(engine: &SearchEngine, query: &str) {
    let tokens = engine.bm25.tokenize_body(query);
    println!("## BM25 ngram_2_2 tokens (n={})", tokens.len());
    if tokens.is_empty() {
        println!("  (no tokens — query produced empty token stream)");
    } else {
        for (pos, text) in &tokens {
            println!("  [{:>3}] {:?}", pos, text);
        }
    }
    println!();
}

fn print_bm25(
    engine: &SearchEngine,
    query: &str,
    limit: usize,
) -> Result<Vec<(u64, f32)>, SearchError> {
    let hits = engine.bm25.search(query, limit)?;
    println!("## BM25 top {} (raw scores)", limit);
    if hits.is_empty() {
        println!("  (no hits)");
    } else {
        for (rank, (doc_id, score)) in hits.iter().enumerate() {
            let label = doc_label(engine.doc_store.get(doc_id));
            println!("  {:>2}. score={:>7.4}  {}", rank + 1, score, label);
        }
    }
    println!();
    Ok(hits)
}

fn print_vector(
    engine: &SearchEngine,
    query: &str,
    limit: usize,
) -> Result<Vec<(u64, f32)>, SearchError> {
    let query_vec = engine
        .embedding
        .encode_single(query)
        .map_err(|e| SearchError::Embedding(e.to_string()))?;
    let hits = engine.vector.search(&query_vec, limit);
    println!("## Vector top {} (cosine similarity)", limit);
    if hits.is_empty() {
        println!("  (no hits)");
    } else {
        for (rank, (doc_id, score)) in hits.iter().enumerate() {
            let label = doc_label(engine.doc_store.get(doc_id));
            println!("  {:>2}. score={:>7.4}  {}", rank + 1, score, label);
        }
    }
    println!();
    Ok(hits)
}

fn print_rrf(
    engine: &SearchEngine,
    bm25_hits: &[(u64, f32)],
    vec_hits: &[(u64, f32)],
    limit: usize,
) {
    let fused = rrf_fuse(bm25_hits, vec_hits, 60.0, limit);
    println!("## RRF fused top {} (k=60)", limit);
    if fused.is_empty() {
        println!("  (no hits)");
    } else {
        for (rank, (doc_id, score)) in fused.iter().enumerate() {
            let bm25_rank = bm25_hits
                .iter()
                .position(|(id, _)| id == doc_id)
                .map(|i| format!("bm25#{}", i + 1))
                .unwrap_or_else(|| "bm25--".to_string());
            let vec_rank = vec_hits
                .iter()
                .position(|(id, _)| id == doc_id)
                .map(|i| format!("vec#{}", i + 1))
                .unwrap_or_else(|| "vec--".to_string());
            let label = doc_label(engine.doc_store.get(doc_id));
            println!(
                "  {:>2}. score={:>7.4}  [{:<8} {:<8}]  {}",
                rank + 1,
                score,
                bm25_rank,
                vec_rank,
                label
            );
        }
    }
    println!();
}

fn doc_label(meta: Option<&DocMeta>) -> String {
    let Some(m) = meta else {
        return "(missing meta)".to_string();
    };
    let kind = match m.kind {
        crate::search::DocKind::Commit => "commit",
        crate::search::DocKind::File => "file  ",
        crate::search::DocKind::Symbol => "symbol",
    };
    let where_ = m.path.clone().unwrap_or_else(|| m.commit_oid.clone());
    let lines = match (m.line_start, m.line_end) {
        (Some(s), Some(e)) => format!(":{}-{}", s, e),
        _ => String::new(),
    };
    format!("{} {}{}  — {}", kind, where_, lines, m.title)
}
