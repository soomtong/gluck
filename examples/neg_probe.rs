//! Negative-query signal probe: per fixture query, prints word-level BM25
//! (title + path_terms) hits/top score, ngram BM25 top score, and vector
//! top-1 / top-1 gap over the top-10 mean. `fix_hits` repeats `word_hits`
//! with typo corrections appended. Run after `glc index`:
//! `cargo run --release --example neg_probe`
use gluck::search::{bm25::Bm25Index, embedding::EmbeddingModel, typo, vector::VectorIndex};
use std::path::Path;

fn main() {
    let dir = Path::new(".glc-index");
    let bm25 = Bm25Index::open(dir.join("bm25")).unwrap();
    let vec = VectorIndex::load(dir.join("vectors").join("index.tvim")).unwrap();
    let model = EmbeddingModel::load().unwrap();
    let vocab = bm25.word_vocab().unwrap();
    let fx: toml::Value =
        toml::from_str(&std::fs::read_to_string("tests/fixtures/search_queries.toml").unwrap())
            .unwrap();
    println!("cat\tword_hits\tfix_hits\tword_top\tngram_top\tvec1\tvec_gap\tquery");
    for q in fx["query"].as_array().unwrap() {
        let text = q["text"].as_str().unwrap();
        let cat = q["category"].as_str().unwrap();
        let word = bm25.search_path_title_only(text, 1000).unwrap();
        let fixes = typo::correct(&typo::query_words(text), &vocab);
        let fixed = bm25
            .search_path_title_only(&typo::append_corrections(text, &fixes), 1000)
            .unwrap();
        let ngram = bm25.search(text, 1).unwrap();
        let v = model.encode_single(text).unwrap();
        let vh = vec.search(&v, 10);
        let mean: f32 = vh.iter().map(|h| h.1).sum::<f32>() / vh.len() as f32;
        println!(
            "{}\t{}\t{}\t{:.2}\t{:.2}\t{:.3}\t{:.3}\t{}",
            cat,
            word.len(),
            fixed.len(),
            word.first().map(|h| h.1).unwrap_or(0.0),
            ngram.first().map(|h| h.1).unwrap_or(0.0),
            vh[0].1,
            vh[0].1 - mean,
            text
        );
    }
}
