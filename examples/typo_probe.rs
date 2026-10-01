//! Prints the typo corrections `SearchEngine` would apply to each fixture
//! query. Run after `glc index`: `cargo run --release --example typo_probe`.

use gluck::search::{typo, SearchEngine};

fn main() {
    let engine = SearchEngine::open(std::path::Path::new(".glc-index")).expect("open index");
    let fixture = std::fs::read_to_string("tests/fixtures/search_queries.toml").unwrap();
    for line in fixture.lines() {
        let Some(text) = line.strip_prefix("text = \"") else {
            continue;
        };
        let query = text.trim_end_matches('"');
        let fixes = typo::correct(&typo::query_words(query), &engine.vocab);
        if !fixes.is_empty() {
            println!("{query:<50} {fixes:?}");
        }
    }
    println!("vocab size: {}", engine.vocab.len());
}
