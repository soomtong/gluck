//! Commit-category probe: for each `category = "commit"` fixture query, the
//! rank of the expected commit in the raw BM25 and vector lists, over all docs
//! and among commit docs only. Run after `glc index`:
//! `cargo run --release --example commit_probe`
use gluck::search::{DocKind, SearchEngine};
use std::path::Path;

fn rank(hits: &[(u64, f32)], target: u64) -> String {
    hits.iter()
        .position(|h| h.0 == target)
        .map(|r| (r + 1).to_string())
        .unwrap_or_else(|| "-".into())
}

fn main() {
    let engine = SearchEngine::open(Path::new(".glc-index")).unwrap();
    let fx: toml::Value =
        toml::from_str(&std::fs::read_to_string("tests/fixtures/search_queries.toml").unwrap())
            .unwrap();
    let n = engine.doc_store.len();
    let is_commit = |id: &u64| {
        engine
            .doc_store
            .get(id)
            .is_some_and(|m| matches!(m.kind, DocKind::Commit))
    };
    println!("bm25\tbm25_c\tvec\tvec_c\tfused\tquery");
    for q in fx["query"].as_array().unwrap() {
        if q["category"].as_str() != Some("commit") {
            continue;
        }
        let text = q["text"].as_str().unwrap();
        let title = q["expected"][0]["title"].as_str().unwrap();
        let Some(target) = engine
            .doc_store
            .values()
            .find(|m| m.kind == DocKind::Commit && m.title == title)
            .map(|m| m.doc_id)
        else {
            println!("?\t?\t?\t?\t?\t{text} (expected commit not indexed)");
            continue;
        };
        let bm25 = if gluck::search::text_prep::is_korean_query(text) {
            engine.bm25.search_path_title_only(text, n).unwrap()
        } else {
            engine.bm25.search(text, n).unwrap()
        };
        let v = engine.embedding.encode_single(text).unwrap();
        let vec = engine.vector.search(&v, n);
        let only = |h: &[(u64, f32)]| {
            h.iter()
                .copied()
                .filter(|x| is_commit(&x.0))
                .collect::<Vec<_>>()
        };
        let fused = engine.search(text, n).unwrap();
        let fused_rank = fused
            .iter()
            .position(|r| r.meta.doc_id == target)
            .map(|r| (r + 1).to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            rank(&bm25, target),
            rank(&only(&bm25), target),
            rank(&vec, target),
            rank(&only(&vec), target),
            fused_rank,
            text
        );
    }
}
