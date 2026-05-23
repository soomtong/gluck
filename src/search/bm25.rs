use std::collections::HashMap;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use tantivy::{Index, IndexWriter, TantivyDocument};

use super::{DocKind, DocMeta, SearchError};

// ── Bigram tokenizer (Korean + general CJK support) ──────────────────────────
//
// English/ASCII: whitespace-split + lowercase (standard BM25)
// Korean/CJK: character bigrams to handle 조사 attachment
// "에러 핸들링" → ["에러", "러", "핸들", "들링", "핸들링", ...]
// Together they cover both BM25 token exact match and substring overlap.

#[derive(Clone)]
pub struct BigramTokenizer;

pub struct BigramTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl TokenStream for BigramTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

impl Tokenizer for BigramTokenizer {
    type TokenStream<'a> = BigramTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let mut tokens = Vec::new();
        let mut offset = 0usize;

        for word in text.split_whitespace() {
            let word_lower = word.to_lowercase();
            let word_start = offset;
            offset += word.len() + 1; // +1 for the space

            // Emit the full word token
            tokens.push(Token {
                offset_from: word_start,
                offset_to: word_start + word.len(),
                position: tokens.len(),
                text: word_lower.clone(),
                position_length: 1,
            });

            // Emit character bigrams for non-ASCII words (Korean/CJK)
            let chars: Vec<char> = word_lower.chars().collect();
            let is_multibyte = chars.iter().any(|c| *c as u32 > 127);
            if is_multibyte {
                for window in chars.windows(2) {
                    let bigram: String = window.iter().collect();
                    tokens.push(Token {
                        offset_from: word_start,
                        offset_to: word_start + word.len(),
                        position: tokens.len(),
                        text: bigram,
                        position_length: 1,
                    });
                }
            }
        }

        BigramTokenStream { tokens, index: 0 }
    }
}

// ── Schema ────────────────────────────────────────────────────────────────────

pub fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("doc_id_str", STRING | STORED);  // doc_id.to_string()
    builder.add_text_field("kind", STRING | STORED);        // "commit" | "file"
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT);
    builder.add_text_field("path", STRING | STORED);
    builder.add_text_field("commit_oid", STRING | STORED);
    builder.build()
}

pub struct Bm25Index {
    index: Index,
}

impl Bm25Index {
    const TOKENIZER_NAME: &'static str = "bigram";

    pub fn build(index_path: &Path, chunks: &[super::chunk::Chunk]) -> tantivy::Result<()> {
        std::fs::create_dir_all(index_path).map_err(|e| {
            tantivy::TantivyError::SystemError(format!("create dir: {e}"))
        })?;

        let schema = build_schema();
        let index = Index::create_in_dir(index_path, schema.clone())?;
        Self::register_tokenizer(&index);

        let mut writer: IndexWriter = index.writer(50_000_000)?;
        let doc_id_field = schema.get_field("doc_id_str").unwrap();
        let kind_field = schema.get_field("kind").unwrap();
        let title_field = schema.get_field("title").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let path_field = schema.get_field("path").unwrap();
        let commit_oid_field = schema.get_field("commit_oid").unwrap();

        for chunk in chunks {
            let kind_str = match chunk.kind {
                DocKind::Commit => "commit",
                DocKind::File => "file",
            };
            let mut doc = TantivyDocument::new();
            doc.add_text(doc_id_field, &chunk.doc_id.to_string());
            doc.add_text(kind_field, kind_str);
            doc.add_text(title_field, &chunk.title);
            doc.add_text(body_field, &chunk.body);
            doc.add_text(path_field, chunk.path.as_deref().unwrap_or(""));
            doc.add_text(commit_oid_field, chunk.commit_oid.as_deref().unwrap_or(""));
            writer.add_document(doc)?;
        }

        writer.commit()?;
        Ok(())
    }

    pub fn open(index_path: &Path) -> tantivy::Result<Self> {
        let index = Index::open_in_dir(index_path)?;
        Self::register_tokenizer(&index);
        Ok(Self { index })
    }

    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<(u64, f32)>, SearchError> {
        let schema = self.index.schema();
        let title_field = schema.get_field("title").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let doc_id_field = schema.get_field("doc_id_str").unwrap();

        let reader = self.index.reader()
            .map_err(|e| SearchError::Tantivy(e.to_string()))?;
        let searcher = reader.searcher();

        let mut parser = QueryParser::for_index(&self.index, vec![title_field, body_field]);
        parser.set_field_fuzzy(title_field, false, 1, true);

        let parsed = match parser.parse_query(query) {
            Ok(q) => q,
            Err(_) => return Ok(vec![]),
        };

        let top_docs = searcher.search(&parsed, &TopDocs::with_limit(top_k))
            .map_err(|e| SearchError::Tantivy(e.to_string()))?;

        let results = top_docs
            .into_iter()
            .filter_map(|(score, addr)| {
                let doc: TantivyDocument = searcher.doc(addr).ok()?;
                let id_str = doc.get_first(doc_id_field)?.as_str()?;
                let doc_id: u64 = id_str.parse().ok()?;
                Some((doc_id, score))
            })
            .collect();

        Ok(results)
    }

    /// Scan all stored docs to build doc_id → DocMeta map (used by SearchEngine::hydrate)
    pub fn scan_doc_store(&self) -> tantivy::Result<HashMap<u64, DocMeta>> {
        let schema = self.index.schema();
        let doc_id_field = schema.get_field("doc_id_str").unwrap();
        let kind_field = schema.get_field("kind").unwrap();
        let title_field = schema.get_field("title").unwrap();
        let path_field = schema.get_field("path").unwrap();
        let commit_oid_field = schema.get_field("commit_oid").unwrap();

        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let mut store = HashMap::new();

        for segment_reader in searcher.segment_readers() {
            let store_reader = segment_reader.get_store_reader(100)?;
            for doc_id in 0..segment_reader.num_docs() {
                if segment_reader.is_deleted(doc_id) {
                    continue;
                }
                let Ok(doc) = store_reader.get::<TantivyDocument>(doc_id) else { continue };
                let id_str = doc.get_first(doc_id_field).and_then(|v| v.as_str()).unwrap_or("");
                let Ok(id) = id_str.parse::<u64>() else { continue };
                let kind_str = doc.get_first(kind_field).and_then(|v| v.as_str()).unwrap_or("");
                let kind = if kind_str == "commit" { DocKind::Commit } else { DocKind::File };
                let title = doc.get_first(title_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let path = doc.get_first(path_field).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
                let commit_oid = doc.get_first(commit_oid_field).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
                store.insert(id, DocMeta { kind, title, path, commit_oid });
            }
        }

        Ok(store)
    }

    fn register_tokenizer(index: &Index) {
        index.tokenizers().register(Self::TOKENIZER_NAME, BigramTokenizer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::chunk::Chunk;
    use tempfile::TempDir;

    fn sample_chunks() -> Vec<Chunk> {
        vec![
            Chunk {
                doc_id: 1,
                kind: DocKind::Commit,
                title: "Fix error handling in parser".to_string(),
                body: "Refactored error handling logic to use Result types".to_string(),
                path: None,
                commit_oid: Some("abc1234".to_string()),
            },
            Chunk {
                doc_id: 2,
                kind: DocKind::File,
                title: "src/parser.rs".to_string(),
                body: "fn parse_input() -> Result<AST, ParseError> { todo!() }".to_string(),
                path: Some("src/parser.rs".to_string()),
                commit_oid: None,
            },
            Chunk {
                doc_id: 3,
                kind: DocKind::Commit,
                title: "에러 핸들링 로직 수정".to_string(),
                body: "에러 처리를 개선하여 panic 대신 Result 반환하도록 변경".to_string(),
                path: None,
                commit_oid: Some("def5678".to_string()),
            },
        ]
    }

    #[test]
    fn test_build_and_search_english() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bm25");
        Bm25Index::build(&path, &sample_chunks()).unwrap();
        let idx = Bm25Index::open(&path).unwrap();

        let results = idx.search("error handling", 10).unwrap();
        assert!(!results.is_empty(), "should find error handling");
        assert!(results.iter().any(|(id, _)| *id == 1 || *id == 2));
    }

    #[test]
    fn test_build_and_search_korean() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bm25");
        Bm25Index::build(&path, &sample_chunks()).unwrap();
        let idx = Bm25Index::open(&path).unwrap();

        let results = idx.search("에러 핸들링", 10).unwrap();
        assert!(!results.is_empty(), "bigram tokenizer should match Korean");
        assert!(results.iter().any(|(id, _)| *id == 3));
    }

    #[test]
    fn test_search_no_results() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bm25");
        Bm25Index::build(&path, &sample_chunks()).unwrap();
        let idx = Bm25Index::open(&path).unwrap();

        let results = idx.search("zzzznonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_returns_u64_ids() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bm25");
        Bm25Index::build(&path, &sample_chunks()).unwrap();
        let idx = Bm25Index::open(&path).unwrap();

        let results = idx.search("parser", 10).unwrap();
        assert!(!results.is_empty());
        for (id, _) in &results {
            assert!(*id >= 1 && *id <= 3);
        }
    }

    #[test]
    fn test_scan_doc_store() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bm25");
        Bm25Index::build(&path, &sample_chunks()).unwrap();
        let idx = Bm25Index::open(&path).unwrap();

        let store = idx.scan_doc_store().unwrap();
        assert_eq!(store.len(), 3);
        assert!(store.contains_key(&1));
        assert_eq!(store[&1].kind, DocKind::Commit);
        assert!(store[&2].path.as_deref() == Some("src/parser.rs"));
    }

    #[test]
    fn test_bigram_tokenizer_emits_korean_bigrams() {
        let mut t = BigramTokenizer;
        let mut stream = t.token_stream("에러 핸들링");
        let mut texts = vec![];
        while stream.advance() {
            texts.push(stream.token().text.clone());
        }
        // Should contain "에러", "핸들링", and bigrams of "핸들링"
        assert!(texts.contains(&"에러".to_string()));
        assert!(texts.contains(&"핸들링".to_string()));
        assert!(texts.iter().any(|t| t == "핸들" || t == "들링"));
    }
}
