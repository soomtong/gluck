use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, INDEXED, STORED, STRING,
};
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, SimpleTokenizer, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyError};

use crate::search::{DocKind, DocMeta, SearchError};

pub const TOKENIZER: &str = "ngram_2_2";
pub const WORD_TOKENIZER: &str = "word_lower";
const WRITER_HEAP: usize = 50_000_000;

pub struct Bm25Fields {
    pub id: Field,
    pub kind: Field,
    pub title: Field,
    pub body: Field,
    pub path: Field,
    pub path_terms: Field,
    pub commit_oid: Field,
    pub line_start: Field,
    pub line_end: Field,
}

pub struct Bm25Index {
    index: Index,
    reader: IndexReader,
    fields: Bm25Fields,
}

fn make_schema() -> (Schema, Bm25Fields) {
    let mut builder = Schema::builder();

    // Title: SimpleTokenizer + LowerCaser. `_` / `/` / `.` / `-` 자동 분해.
    // camelCase는 add_doc에서 write-time으로 split.
    let title_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(WORD_TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    // Path terms: 검색 전용 (STORED 없음). path를 단어 단위로 매칭.
    let path_terms_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(WORD_TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqs),
    );

    // Body: ngram_2_2 유지 — 한글/임의 텍스트 부분 매칭. 멀티-토큰 쿼리(phrase)를 위해 positions 필요.
    let body_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    let id = builder.add_u64_field("id", FAST | STORED | INDEXED);
    let kind = builder.add_text_field("kind", STRING | STORED);
    let title = builder.add_text_field("title", title_opts);
    let body = builder.add_text_field("body", body_opts);
    let path = builder.add_text_field("path", STRING | STORED);
    let path_terms = builder.add_text_field("path_terms", path_terms_opts);
    let commit_oid = builder.add_text_field("commit_oid", STRING | STORED);
    let line_start = builder.add_u64_field("line_start", STORED);
    let line_end = builder.add_u64_field("line_end", STORED);

    let schema = builder.build();
    let fields = Bm25Fields {
        id,
        kind,
        title,
        body,
        path,
        path_terms,
        commit_oid,
        line_start,
        line_end,
    };
    (schema, fields)
}

fn register_tokenizer(index: &Index) {
    let ngram =
        TextAnalyzer::builder(NgramTokenizer::new(2, 2, false).expect("valid ngram params"))
            .filter(LowerCaser)
            .build();
    index.tokenizers().register(TOKENIZER, ngram);

    let word_lower = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(LowerCaser)
        .build();
    index.tokenizers().register(WORD_TOKENIZER, word_lower);
}

impl Bm25Index {
    pub fn create(dir: &Path) -> Result<Self, SearchError> {
        std::fs::create_dir_all(dir)?;
        let (schema, fields) = make_schema();
        let index = Index::create_in_dir(dir, schema)?;
        register_tokenizer(&index);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            index,
            reader,
            fields,
        })
    }

    pub fn open(dir: PathBuf) -> Result<Self, SearchError> {
        let index = Index::open_in_dir(&dir)?;
        let (_, fields) = make_schema();
        register_tokenizer(&index);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            index,
            reader,
            fields,
        })
    }

    pub fn writer(&self) -> Result<IndexWriter, TantivyError> {
        self.index.writer(WRITER_HEAP)
    }

    pub fn add_doc(
        &self,
        writer: &mut IndexWriter,
        meta: &DocMeta,
        body: &str,
    ) -> Result<(), TantivyError> {
        let mut doc = tantivy::TantivyDocument::default();
        doc.add_u64(self.fields.id, meta.doc_id);
        doc.add_text(self.fields.kind, meta.kind.as_str());
        doc.add_text(self.fields.title, &meta.title);
        doc.add_text(self.fields.body, body);
        doc.add_text(self.fields.commit_oid, &meta.commit_oid);
        if let Some(p) = &meta.path {
            doc.add_text(self.fields.path, p);
        }
        if let Some(ls) = meta.line_start {
            doc.add_u64(self.fields.line_start, u64::from(ls));
        }
        if let Some(le) = meta.line_end {
            doc.add_u64(self.fields.line_end, u64::from(le));
        }
        writer.add_document(doc)?;
        Ok(())
    }

    pub fn delete_doc(&self, writer: &mut IndexWriter, doc_id: u64) {
        let term = tantivy::Term::from_field_u64(self.fields.id, doc_id);
        writer.delete_term(term);
    }

    pub fn commit(&self, mut writer: IndexWriter) -> Result<(), TantivyError> {
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(u64, f32)>, SearchError> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.fields.title, self.fields.body]);
        let tantivy_query = match parser.parse_query(query) {
            Ok(q) => q,
            Err(_) => return Ok(vec![]),
        };
        let top_docs = searcher.search(&tantivy_query, &TopDocs::with_limit(limit))?;
        let mut results = Vec::new();
        for (score, doc_addr) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(doc_addr)?;
            if let Some(id) = doc.get_first(self.fields.id).and_then(value_as_u64) {
                results.push((id, score));
            }
        }
        Ok(results)
    }

    pub fn scan_doc_store(&self) -> Result<HashMap<u64, DocMeta>, SearchError> {
        use tantivy::query::AllQuery;
        let searcher = self.reader.searcher();
        let top_docs = searcher.search(&AllQuery, &TopDocs::with_limit(1_000_000))?;
        let mut store = HashMap::new();
        for (_score, doc_addr) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(doc_addr)?;
            let Some(doc_id) = doc.get_first(self.fields.id).and_then(value_as_u64) else {
                continue;
            };
            let Some(kind) = doc
                .get_first(self.fields.kind)
                .and_then(value_as_str)
                .and_then(DocKind::parse)
            else {
                continue;
            };
            let Some(title) = doc
                .get_first(self.fields.title)
                .and_then(value_as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            let Some(commit_oid) = doc
                .get_first(self.fields.commit_oid)
                .and_then(value_as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            let path = doc
                .get_first(self.fields.path)
                .and_then(value_as_str)
                .map(str::to_owned);
            let line_start = doc
                .get_first(self.fields.line_start)
                .and_then(value_as_u64)
                .map(|v| v as u32);
            let line_end = doc
                .get_first(self.fields.line_end)
                .and_then(value_as_u64)
                .map(|v| v as u32);
            store.insert(
                doc_id,
                DocMeta {
                    doc_id,
                    kind,
                    title,
                    commit_oid,
                    path,
                    line_start,
                    line_end,
                },
            );
        }
        Ok(store)
    }
}

fn value_as_u64(v: &tantivy::schema::OwnedValue) -> Option<u64> {
    match v {
        tantivy::schema::OwnedValue::U64(n) => Some(*n),
        _ => None,
    }
}

fn value_as_str(v: &tantivy::schema::OwnedValue) -> Option<&str> {
    match v {
        tantivy::schema::OwnedValue::Str(s) => Some(s.as_str()),
        _ => None,
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

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "commit" => Some(DocKind::Commit),
            "file" => Some(DocKind::File),
            "symbol" => Some(DocKind::Symbol),
            _ => None,
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

    fn commit_meta(doc_id: u64, title: &str) -> DocMeta {
        DocMeta {
            doc_id,
            kind: DocKind::Commit,
            title: title.to_string(),
            commit_oid: format!("{:040x}", doc_id),
            path: None,
            line_start: None,
            line_end: None,
        }
    }

    #[test]
    fn test_create_and_search_basic() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(&mut w, &commit_meta(1, "hello world"), "greeting text")
            .unwrap();
        idx.commit(w).unwrap();
        let results = idx.search("hello", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_search_no_results() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(
            &mut w,
            &commit_meta(1, "rust programming"),
            "systems language",
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
            &commit_meta(42, "에러 처리"),
            "에러 처리 방법에 대한 설명",
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
    fn test_word_tokenizer_lowercases_title() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(&mut w, &commit_meta(1, "Hello"), "").unwrap();
        idx.commit(w).unwrap();
        let results = idx.search("hello", 10).unwrap();
        assert!(
            !results.is_empty(),
            "lowercase query 'hello' must match title 'Hello' — requires LowerCaser on word tokenizer"
        );
    }

    #[test]
    fn test_cached_reader_sees_data_across_multiple_commits() {
        let (_dir, idx) = tmp_index();

        let mut w = idx.writer().unwrap();
        idx.add_doc(&mut w, &commit_meta(1, "first doc"), "")
            .unwrap();
        idx.commit(w).unwrap();
        let r1 = idx.search("first", 10).unwrap();
        assert_eq!(r1.len(), 1, "first commit visible");

        let mut w = idx.writer().unwrap();
        idx.add_doc(&mut w, &commit_meta(2, "second doc"), "")
            .unwrap();
        idx.commit(w).unwrap();
        let r2 = idx.search("second", 10).unwrap();
        assert!(
            r2.iter().any(|(id, _)| *id == 2),
            "second commit must be visible via cached reader"
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
        idx.add_doc(&mut w, &meta, "fn foo() {}").unwrap();
        idx.commit(w).unwrap();
        let store = idx.scan_doc_store().unwrap();
        let got = store.get(&7).expect("doc 7 exists");
        assert_eq!(got.commit_oid, oid);
        assert_eq!(got.kind, DocKind::File);
        assert_eq!(got.path.as_deref(), Some("src/foo.rs"));
        assert_eq!(got.line_start, Some(10));
        assert_eq!(got.line_end, Some(20));
    }

    #[test]
    fn test_delete_doc_removes_from_search() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        idx.add_doc(&mut w, &commit_meta(1, "hello world"), "greeting")
            .unwrap();
        idx.add_doc(&mut w, &commit_meta(2, "hello again"), "second")
            .unwrap();
        idx.commit(w).unwrap();
        assert_eq!(idx.search("hello", 10).unwrap().len(), 2);

        let mut w = idx.writer().unwrap();
        idx.delete_doc(&mut w, 1);
        idx.commit(w).unwrap();
        let r = idx.search("hello", 10).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, 2);
    }

    #[test]
    fn test_path_field_exact_match_query() {
        let (_dir, idx) = tmp_index();
        let mut w = idx.writer().unwrap();
        let meta1 = DocMeta {
            doc_id: 1,
            kind: DocKind::File,
            title: "src/search/error.rs".into(),
            commit_oid: "a".repeat(40),
            path: Some("src/search/error.rs".into()),
            line_start: None,
            line_end: None,
        };
        let meta2 = DocMeta {
            doc_id: 2,
            kind: DocKind::File,
            title: "src/ui/view.rs".into(),
            commit_oid: "b".repeat(40),
            path: Some("src/ui/view.rs".into()),
            line_start: None,
            line_end: None,
        };
        idx.add_doc(&mut w, &meta1, "fn handle_error() {}").unwrap();
        idx.add_doc(&mut w, &meta2, "fn render() {}").unwrap();
        idx.commit(w).unwrap();

        // Field-prefixed query targeting path STRING field.
        let results = idx.search("path:\"src/search/error.rs\"", 10).unwrap();
        assert_eq!(
            results.len(),
            1,
            "only the matching path should be returned"
        );
        assert_eq!(results[0].0, 1);
    }
}
