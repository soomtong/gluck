use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, STORED};
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer};
use tantivy::{doc, Index, IndexWriter, TantivyError};

use crate::search::{DocKind, DocMeta, SearchError};

pub const TOKENIZER: &str = "ngram_2_2";
const WRITER_HEAP: usize = 50_000_000;

pub struct Bm25Fields {
    pub doc_id: Field,
    pub title: Field,
    pub body: Field,
    pub meta_json: Field,
}

pub struct Bm25Index {
    index: Index,
    fields: Bm25Fields,
}

fn make_schema() -> (Schema, Bm25Fields) {
    let mut builder = Schema::builder();

    let doc_id = builder.add_text_field("doc_id", STORED);

    let text_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let title = builder.add_text_field("title", text_opts);

    let body_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqs),
    );
    let body = builder.add_text_field("body", body_opts);

    let meta_json = builder.add_text_field("meta_json", STORED);

    let schema = builder.build();
    let fields = Bm25Fields {
        doc_id,
        title,
        body,
        meta_json,
    };
    (schema, fields)
}

fn register_tokenizer(index: &Index) {
    let tokenizer =
        TextAnalyzer::builder(NgramTokenizer::new(2, 2, false).expect("valid ngram params"))
            .filter(LowerCaser)
            .build();
    index.tokenizers().register(TOKENIZER, tokenizer);
}

impl Bm25Index {
    pub fn create(dir: &Path) -> Result<Self, SearchError> {
        std::fs::create_dir_all(dir)?;
        let (schema, fields) = make_schema();
        let index = Index::create_in_dir(dir, schema)?;
        register_tokenizer(&index);
        Ok(Self { index, fields })
    }

    pub fn open(dir: PathBuf) -> Result<Self, SearchError> {
        let index = Index::open_in_dir(&dir)?;
        let (_, fields) = make_schema();
        register_tokenizer(&index);
        Ok(Self { index, fields })
    }

    pub fn writer(&self) -> Result<IndexWriter, TantivyError> {
        self.index.writer(WRITER_HEAP)
    }

    pub fn add_doc(
        &self,
        writer: &mut IndexWriter,
        doc_id: u64,
        title: &str,
        body: &str,
        meta_json: &str,
    ) -> Result<(), TantivyError> {
        writer.add_document(doc!(
            self.fields.doc_id    => doc_id.to_string(),
            self.fields.title     => title,
            self.fields.body      => body,
            self.fields.meta_json => meta_json,
        ))?;
        Ok(())
    }

    pub fn commit(&self, mut writer: IndexWriter) -> Result<(), TantivyError> {
        writer.commit()?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(u64, f32)>, SearchError> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.fields.title, self.fields.body]);
        let tantivy_query = match parser.parse_query(query) {
            Ok(q) => q,
            Err(_) => return Ok(vec![]),
        };
        let top_docs = searcher.search(&tantivy_query, &TopDocs::with_limit(limit))?;
        let mut results = Vec::new();
        for (score, doc_addr) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(doc_addr)?;
            if let Some(tantivy::schema::OwnedValue::Str(id_str)) =
                doc.get_first(self.fields.doc_id)
            {
                if let Ok(id) = id_str.parse::<u64>() {
                    results.push((id, score));
                }
            }
        }
        Ok(results)
    }

    pub fn scan_doc_store(&self) -> Result<HashMap<u64, DocMeta>, SearchError> {
        use tantivy::query::AllQuery;
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let top_docs = searcher.search(&AllQuery, &TopDocs::with_limit(1_000_000))?;
        let mut store = HashMap::new();
        for (_score, doc_addr) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(doc_addr)?;
            let meta_str = match doc.get_first(self.fields.meta_json) {
                Some(tantivy::schema::OwnedValue::Str(s)) => s,
                _ => continue,
            };
            let Ok(meta) = serde_json::from_str::<DocMeta>(meta_str) else {
                continue;
            };
            store.insert(meta.doc_id, meta);
        }
        Ok(store)
    }
}

impl DocKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocKind::Commit => "commit",
            DocKind::File => "file",
            DocKind::Symbol => "symbol",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp_index() -> (TempDir, Bm25Index) {
        let dir = tempfile::tempdir().unwrap();
        let idx = Bm25Index::create(dir.path()).unwrap();
        (dir, idx)
    }

    fn meta_json(doc_id: u64, title: &str) -> String {
        let m = DocMeta {
            doc_id,
            kind: DocKind::Commit,
            title: title.to_string(),
            commit_oid: format!("{:040x}", doc_id),
            path: None,
            line_start: None,
            line_end: None,
        };
        serde_json::to_string(&m).unwrap()
    }

    #[test]
    fn test_create_and_search_basic() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(
            &mut w,
            1,
            "hello world",
            "greeting text",
            &meta_json(1, "hello world"),
        )
        .unwrap();
        idx.commit(w).unwrap();
        // "he" is a direct bigram from "hello" — single-term query avoids phrase matching edge cases
        let results = idx.search("he", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_search_no_results() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(
            &mut w,
            1,
            "rust programming",
            "systems language",
            &meta_json(1, "rust"),
        )
        .unwrap();
        idx.commit(w).unwrap();
        let results = idx.search("가나다라", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_korean_bigram_search() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(
            &mut w,
            42,
            "에러 처리",
            "에러 처리 방법에 대한 설명",
            &meta_json(42, "에러 처리"),
        )
        .unwrap();
        idx.commit(w).unwrap();
        let results = idx.search("에러", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 42);
    }

    #[test]
    fn test_tokenizer_name_is_ngram_2_2() {
        assert_eq!(TOKENIZER, "ngram_2_2");
    }

    #[test]
    fn test_uppercase_indexed_matches_lowercase_query() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(&mut w, 1, "Hello", "", &meta_json(1, "Hello"))
            .unwrap();
        idx.commit(w).unwrap();
        let results = idx.search("he", 10).unwrap();
        assert!(
            !results.is_empty(),
            "2-char lowercase query 'he' must match 'Hello' — requires LowerCaser on 'He'"
        );
    }

    #[test]
    fn test_scan_doc_store_preserves_metadata() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        let oid = "abcdef1234567890abcdef1234567890abcdef12";
        let meta = DocMeta {
            doc_id: 7,
            kind: DocKind::File,
            title: "src/foo.rs".into(),
            commit_oid: oid.into(),
            path: Some("src/foo.rs".into()),
            line_start: Some(10),
            line_end: Some(20),
        };
        idx.add_doc(
            &mut w,
            7,
            "src/foo.rs",
            "fn foo() {}",
            &serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
        idx.commit(w).unwrap();
        let store = idx.scan_doc_store().unwrap();
        let got = store.get(&7).expect("doc 7 exists");
        assert_eq!(got.commit_oid, oid);
        assert_eq!(got.kind, DocKind::File);
        assert_eq!(got.path.as_deref(), Some("src/foo.rs"));
        assert_eq!(got.line_start, Some(10));
    }
}
