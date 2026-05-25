# Tantivy 한국어 토큰화 Implementation Plan

## Status

- **Type:** Implementation plan, follow-up to v2 design spec
- **Implements:** `docs/superpowers/specs/2026-05-23-semantic-search-design-v2.md` §Components.3 (BM25 / Korean tokenization)
- **Depends on:** `2026-05-24-chunking-implementation-plan` (Chunk enum)
- **Predecessor for:** `indexer-pipeline-plan`

## Context

gluck 검색 코퍼스의 이질성을 분류:

| 텍스트 종류 | 출처 | 토큰화 특성 |
|---|---|---|
| 한국어 prose | commit title / body | 형태소/ngram 모두 가능 |
| 영문 prose | commit title / body (영문 프로젝트), 주석 | 단어 분리 |
| 코드 식별자 | `parseConfig`, `error_handler` | camelCase/snake/kebab 분리 필요 |
| 코드 본문 | 함수 body, 클래스 body | 한·영 혼재 가능 |
| 파일 경로 | `src/search/error.rs` | 경로 segmenter |

이 *모든 종류*를 만족하는 단일 토큰화는 존재하지 않는다. 본 plan은 **가장 가치 있는 일부분을 단순한 방법으로 잡고, 나머지는 벡터 검색이 보정한다**는 v2 spec의 전제를 그대로 따른다.

핵심 결정 한 줄: **`NgramTokenizer(2,2) + LowerCaser`** — 영택님이 AlloyDB에서 검증한 pg_bigm 패턴의 Tantivy 이식.

---

## Key Decisions

### D1. 단일 토큰화로 출발 — `ngram_2_2`

- 한국어 retrieval: ✅ 작동 (`"에러 처리"` → `["에러", "러 ", " 처", "처리"]`)
- 영문 retrieval: ⚠️ noisy하지만 동작 (BM25 IDF가 빈번한 bigram 자동 down-weight)
- 코드 식별자 retrieval: ⚠️ degraded (camelCase 분리 안 됨)
- **의미 기반 매칭은 Model2Vec 벡터가 담당.** BM25는 *lexical* 신호에만 충실.

코드 식별자 검색은 BM25가 약하지만, 벡터 임베딩이 강하다 — `"parseConfig"` 쿼리는 함수 본문 임베딩과 cosine similarity로 매칭됨. **RRF 융합이 두 약점을 메워주는 것이 v2 architecture의 핵심.**

### D2. Cross-word bigrams 허용

`NgramTokenizer`는 *whitespace를 인식하지 않고* 전체 문자열에서 ngram을 생성한다. 즉 `"에러 처리"` → `["에러", "러 ", " 처", "처리"]` — 공백을 포함한 bigram이 섞임.

- 순수성을 원하면 *whitespace pre-tokenize → 단어별 ngram* 형태의 custom tokenizer 필요
- 그러나 cross-word bigram은 부수적으로 `"에러 처리"`와 `"에러처리"`(붙임)의 부분 매칭 효과를 만든다 — *의도된 부수 효과*로 간주
- **MVP는 cross-word 허용**, Phase 2에서 측정 후 재검토

### D3. `path` 필드는 `STRING` (토큰화 없음)

ngram이 `src/search/error.rs`에 적용되면 `["sr", "rc", "c/", "/s", ...]` 같은 nonsense. **`path`는 STRING 필드로 두고 정확 매칭만.** 별도로 *path-aware 검색*이 필요해지면 `path_tokens` 필드를 추가해서 `/`, `-`, `_`, `.`로 분리한 토큰을 인덱싱.

MVP는 `STRING`만, Phase 2에서 `path_tokens` 검토.

### D4. `title` vs `body` 분리, MVP는 동일 가중치

Tantivy `QueryParser`는 필드별 boost를 지원 (`title^2 body^1`). 일반적으로 title은 더 높은 가중치가 자연스럽지만 — MVP는 동일 가중치로 시작, 실제 검색 품질 측정 후 튜닝.

### D5. 스키마/토큰화 변경 시 강제 reindex

`meta.toml`의 `[bm25] tokenizer = "ngram_2_2"`와 코드의 현재 default가 일치하지 않으면 `Bm25Error::IncompatibleTokenizer`. `glc index --force`로 명시적 재구축 유도. *암묵적 마이그레이션 없음.*

---

## 변경 순서

### Step 1: Tantivy 의존성 추가 — `Cargo.toml`

~~~toml
[dependencies]
tantivy = "0.22"
~~~

구현 시점의 최신 stable 사용. Tantivy는 minor 버전 간 API 변경이 잦으므로 `cargo doc --open`으로 실제 시그니처 확인 권장. 본 plan의 예제 코드는 0.22 기준.

**바이너리 크기 영향:** Tantivy + 의존성으로 약 5-8MB 증가. 단일 바이너리 정체성 유지 (native dep 없음).

**수정 파일:** `Cargo.toml`

---

### Step 2: 토큰화 등록 — `src/search/bm25/tokenizer.rs` (신규)

~~~rust
// src/search/bm25/tokenizer.rs

use tantivy::tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer, TokenizerManager};

/// 토큰화 이름. meta.toml에 기록되어 호환성 검사에 사용.
pub const TOKENIZER_NAME: &str = "ngram_2_2";

/// Index 또는 IndexReader에 토큰화를 등록한다.
/// 인덱싱과 검색 양쪽에서 동일하게 호출되어야 한다.
pub fn register(manager: &TokenizerManager) {
    let tokenizer = TextAnalyzer::builder(
        NgramTokenizer::new(2, 2, false)
            .expect("ngram(2,2) parameters are valid"),
    )
    .filter(LowerCaser)
    .build();

    manager.register(TOKENIZER_NAME, tokenizer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy::tokenizer::{TextAnalyzer, Token};

    fn collect(analyzer: &mut TextAnalyzer, text: &str) -> Vec<String> {
        let mut stream = analyzer.token_stream(text);
        let mut out = Vec::new();
        while let Some(tok) = stream.next() {
            out.push(tok.text.clone());
        }
        out
    }

    fn make_analyzer() -> TextAnalyzer {
        TextAnalyzer::builder(NgramTokenizer::new(2, 2, false).unwrap())
            .filter(LowerCaser)
            .build()
    }

    #[test]
    fn korean_bigrams() {
        let mut a = make_analyzer();
        let tokens = collect(&mut a, "에러 처리");
        // cross-word bigram 포함
        assert!(tokens.contains(&"에러".to_string()));
        assert!(tokens.contains(&"처리".to_string()));
        assert!(tokens.contains(&"러 ".to_string()) || tokens.contains(&" 처".to_string()));
    }

    #[test]
    fn english_lowercased() {
        let mut a = make_analyzer();
        let tokens = collect(&mut a, "ParseConfig");
        assert!(tokens.contains(&"pa".to_string()));
        // LowerCaser 적용 — 대문자 token 없음
        assert!(!tokens.iter().any(|t| t.chars().any(|c| c.is_uppercase())));
    }

    #[test]
    fn mixed_korean_english() {
        let mut a = make_analyzer();
        let tokens = collect(&mut a, "에러 fix v2");
        assert!(tokens.contains(&"에러".to_string()));
        assert!(tokens.contains(&"fi".to_string()));
    }
}
~~~

**왜 별도 함수인가:** Tantivy는 `Index` 인스턴스 *각각*이 자체 `TokenizerManager`를 보유. 인덱싱하는 쪽과 검색하는 쪽이 *다른 프로세스*일 수 있고(예: `glc index`와 `glc` TUI), *다른 시점에* 같은 토큰화를 등록해야 한다. 함수로 분리해서 두 곳에서 호출.

**수정 파일:** `src/search/bm25/tokenizer.rs` (신규), `src/search/bm25/mod.rs` (신규)

---

### Step 3: Schema 정의 — `src/search/bm25/schema.rs` (신규)

~~~rust
// src/search/bm25/schema.rs

use tantivy::schema::{
    IndexRecordOption, Schema, SchemaBuilder, TextFieldIndexing, TextOptions,
    FAST, INDEXED, STORED, STRING,
};

use crate::search::bm25::tokenizer::TOKENIZER_NAME;

#[derive(Debug, Clone, Copy)]
pub struct Bm25Fields {
    pub id:          tantivy::schema::Field,    // u64 doc_id (FAST, STORED)
    pub kind:        tantivy::schema::Field,    // "commit" | "file" | "symbol" (STRING)
    pub title:       tantivy::schema::Field,    // 검색 대상 (TEXT, ngram)
    pub body:        tantivy::schema::Field,    // 검색 대상 (TEXT, ngram)
    pub path:        tantivy::schema::Field,    // 정확 매칭 + STORED
    pub commit_oid:  tantivy::schema::Field,    // 정확 매칭 + STORED
    pub line_start:  tantivy::schema::Field,    // u64, STORED
    pub line_end:    tantivy::schema::Field,    // u64, STORED
}

pub fn build_schema() -> (Schema, Bm25Fields) {
    let mut builder: SchemaBuilder = Schema::builder();

    // Ngram 토큰화를 사용하는 텍스트 필드 옵션
    let text_indexing = TextFieldIndexing::default()
        .set_tokenizer(TOKENIZER_NAME)
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let text_options = TextOptions::default()
        .set_indexing_options(text_indexing.clone())
        .set_stored();   // title은 결과 표시 위해 STORED
    let body_options = TextOptions::default()
        .set_indexing_options(text_indexing);
        // body는 STORED 아님 — 인덱싱만, 결과 표시는 hydrate 단계에서 commit_oid+path로

    let id         = builder.add_u64_field   ("id",         FAST | STORED);
    let kind       = builder.add_text_field  ("kind",       STRING | STORED);
    let title      = builder.add_text_field  ("title",      text_options);
    let body       = builder.add_text_field  ("body",       body_options);
    let path       = builder.add_text_field  ("path",       STRING | STORED);
    let commit_oid = builder.add_text_field  ("commit_oid", STRING | STORED);
    let line_start = builder.add_u64_field   ("line_start", STORED);
    let line_end   = builder.add_u64_field   ("line_end",   STORED);

    let schema = builder.build();
    let fields = Bm25Fields {
        id, kind, title, body, path, commit_oid, line_start, line_end,
    };
    (schema, fields)
}
~~~

**필드 선택 근거:**

- `id`: `FAST | STORED`. 검색 결과에서 doc_id를 즉시 읽어내야 함 → FAST. RRF fusion에서 vector 결과와 join할 키.
- `kind`: `STRING | STORED`. 정확 매칭만 (`"commit"` / `"file"` / `"symbol"`). 모달 UI 그룹화 시 사용.
- `title`, `body`: `TEXT` with ngram. 실질적 검색 대상.
- `path`, `commit_oid`: `STRING | STORED`. 토큰화 없음, 결과 표시에 필요.
- `line_start`, `line_end`: `STORED`. View mode 점프에 필요, 검색 대상은 아님.

**왜 `body`는 `STORED`가 아닌가:**

body는 commit message body 또는 코드 청크 본문 — *길 수 있음*. 인덱스 크기를 줄이기 위해 STORED 제외. 결과 표시 시점에 `commit_oid` + `path` + (`line_start`, `line_end`)로 원본 git blob에서 다시 읽으면 됨. **인덱스는 lookup 정보만, content는 git이 source of truth.**

**수정 파일:** `src/search/bm25/schema.rs` (신규)

---

### Step 4: `Bm25Index` — `src/search/bm25/mod.rs`

~~~rust
// src/search/bm25/mod.rs

pub mod schema;
pub mod tokenizer;

use std::path::{Path, PathBuf};
use tantivy::{Index, IndexReader, IndexWriter};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::ReloadPolicy;

use schema::{build_schema, Bm25Fields};

#[derive(Debug, thiserror::Error)]
pub enum Bm25Error {
    #[error("tantivy: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("query parse: {0}")]
    Query(String),
    #[error("doc id parse: {0}")]
    DocId(String),
    #[error("incompatible tokenizer: expected {expected}, found {found}")]
    IncompatibleTokenizer { expected: String, found: String },
}

pub struct Bm25Index {
    index: Index,
    reader: IndexReader,
    fields: Bm25Fields,
    path: PathBuf,
}

impl Bm25Index {
    /// 신규 인덱스 생성. `.glc-index/bm25/` 디렉토리에 스키마와 토큰화를 박는다.
    pub fn create(index_root: &Path) -> Result<Self, Bm25Error> {
        std::fs::create_dir_all(index_root)?;
        let (schema, fields) = build_schema();
        let index = Index::create_in_dir(index_root, schema)?;
        tokenizer::register(index.tokenizers());

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            fields,
            path: index_root.to_path_buf(),
        })
    }

    /// 기존 인덱스 열기.
    pub fn open(index_root: &Path) -> Result<Self, Bm25Error> {
        let index = Index::open_in_dir(index_root)?;
        tokenizer::register(index.tokenizers());

        // schema 일치 확인은 Tantivy가 내부적으로 처리 (스키마 불일치 시 에러)
        let (_, fields) = build_schema();

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            fields,
            path: index_root.to_path_buf(),
        })
    }

    pub fn writer(&self, heap_size_bytes: usize) -> Result<Bm25Writer, Bm25Error> {
        let inner = self.index.writer(heap_size_bytes)?;
        Ok(Bm25Writer {
            inner,
            fields: self.fields,
        })
    }

    /// 쿼리 → ranked `(doc_id, score)` 리스트.
    /// RRF fusion에서 vector 결과와 결합 가능한 형태.
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<(u64, f32)>, Bm25Error> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.fields.title, self.fields.body]);
        let parsed = parser
            .parse_query(query)
            .map_err(|e| Bm25Error::Query(e.to_string()))?;
        let top = searcher.search(&parsed, &TopDocs::with_limit(top_k))?;

        let mut out = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
            let id = doc
                .get_first(self.fields.id)
                .and_then(|v| v.as_u64())
                .ok_or_else(|| Bm25Error::DocId("id field missing".into()))?;
            out.push((id, score));
        }
        Ok(out)
    }
}
~~~

**`heap_size_bytes`:** Tantivy `IndexWriter`의 인메모리 버퍼 크기. 기본 50MB 권장. gluck 규모(<100K docs)에서는 충분.

**Query parsing 정책:**
- `QueryParser::for_index(&index, vec![title, body])` — 기본 검색 필드는 title과 body
- 사용자가 `"path:src/search"` 같은 필드 prefix 쓰면 Tantivy가 해석
- MVP는 default field 검색만, 고급 syntax 노출 안 함

**수정 파일:** `src/search/bm25/mod.rs` (신규)

---

### Step 5: `Bm25Writer` — Chunk → Tantivy document 매핑

~~~rust
// src/search/bm25/mod.rs (계속)

use tantivy::TantivyDocument;
use crate::search::chunk::Chunk;

pub struct Bm25Writer {
    inner: IndexWriter,
    fields: Bm25Fields,
}

impl Bm25Writer {
    /// 청크를 Tantivy document로 변환해서 추가.
    /// doc_id는 호출자가 결정 (turbovec과 공유하는 u64 단조 증가 카운터).
    pub fn add_chunk(&mut self, doc_id: u64, chunk: &Chunk) -> Result<(), Bm25Error> {
        let mut doc = TantivyDocument::default();
        doc.add_u64(self.fields.id, doc_id);
        doc.add_text(self.fields.kind, chunk.kind_str());

        match chunk {
            Chunk::CommitMessage { oid, title, body, .. } => {
                doc.add_text(self.fields.title, title);
                doc.add_text(self.fields.body, body);
                doc.add_text(self.fields.commit_oid, oid);
                // path, line_start, line_end는 비워둠
            }
            Chunk::WholeFile { commit_oid, path, content, .. } => {
                // path를 title로 — "src/search/error.rs"가 검색 대상이자 표시 단위
                doc.add_text(self.fields.title, path);
                doc.add_text(self.fields.body, content);
                doc.add_text(self.fields.commit_oid, commit_oid);
                doc.add_text(self.fields.path, path);
            }
            Chunk::Symbol {
                commit_oid, path, name, line_start, line_end, content, ..
            } => {
                // title = "path::symbol_name" 형태로 식별
                let title = format!("{}::{}", path, name);
                doc.add_text(self.fields.title, &title);
                doc.add_text(self.fields.body, content);
                doc.add_text(self.fields.commit_oid, commit_oid);
                doc.add_text(self.fields.path, path);
                doc.add_u64(self.fields.line_start, u64::from(*line_start));
                doc.add_u64(self.fields.line_end, u64::from(*line_end));
            }
        }

        self.inner.add_document(doc)?;
        Ok(())
    }

    pub fn commit(mut self) -> Result<(), Bm25Error> {
        self.inner.commit()?;
        Ok(())
    }
}
~~~

**디자인 결정 — `Symbol.title = "path::name"`:**

검색 쿼리 `"error.rs handle"`이 `Symbol { path: "src/error.rs", name: "handle_io_error" }`에 매칭되도록. *path가 title 안에 ngram으로 들어감* → file scope 매칭 가능. path 정확 매칭은 별도 `path` 필드가 담당.

**`WholeFile.title = path`:**

이 청크의 *식별자*는 path. body(파일 내용)와 함께 인덱싱. 검색 결과 표시 시 path만 보여줘도 사용자가 인식 가능.

**수정 파일:** `src/search/bm25/mod.rs` (계속)

---

### Step 6: 통합 테스트 — `tests/bm25_integration.rs`

~~~rust
// tests/bm25_integration.rs

use gluck::search::bm25::Bm25Index;
use gluck::search::chunk::Chunk;
use gluck::lang::Language;
use tempfile::TempDir;

fn make_commit_chunk(oid: &str, title: &str, body: &str) -> Chunk {
    Chunk::CommitMessage {
        oid: oid.to_string(),
        title: title.to_string(),
        body: body.to_string(),
        author_time: 0,
    }
}

#[test]
fn index_and_search_korean_commit_message() {
    let dir = TempDir::new().unwrap();
    let index = Bm25Index::create(dir.path()).unwrap();

    let mut writer = index.writer(50_000_000).unwrap();
    writer.add_chunk(1, &make_commit_chunk("aaa", "에러 처리 로직 수정", "buggy 코드를 고침")).unwrap();
    writer.add_chunk(2, &make_commit_chunk("bbb", "테스트 추가", "")).unwrap();
    writer.commit().unwrap();

    // 약간 지연이 필요할 수 있음 (ReloadPolicy::OnCommit)
    let results = index.search("에러", 10).unwrap();
    assert!(results.iter().any(|(id, _)| *id == 1));
    assert!(!results.iter().any(|(id, _)| *id == 2));
}

#[test]
fn cross_word_bigram_matches_glued_query() {
    let dir = TempDir::new().unwrap();
    let index = Bm25Index::create(dir.path()).unwrap();
    let mut writer = index.writer(50_000_000).unwrap();
    writer.add_chunk(1, &make_commit_chunk("aaa", "에러 처리 로직", "")).unwrap();
    writer.commit().unwrap();

    // "에러처리" (붙임)로 검색 — 부분 매칭됨
    let results = index.search("에러처리", 10).unwrap();
    assert!(!results.is_empty(), "glued Korean query should still match");
}

#[test]
fn english_query_works() {
    let dir = TempDir::new().unwrap();
    let index = Bm25Index::create(dir.path()).unwrap();
    let mut writer = index.writer(50_000_000).unwrap();
    writer.add_chunk(1, &make_commit_chunk("aaa", "Fix parseConfig bug", "")).unwrap();
    writer.add_chunk(2, &make_commit_chunk("bbb", "Add tests", "")).unwrap();
    writer.commit().unwrap();

    let results = index.search("parseConfig", 10).unwrap();
    assert!(results.iter().any(|(id, _)| *id == 1));
}

#[test]
fn open_existing_index() {
    let dir = TempDir::new().unwrap();
    {
        let index = Bm25Index::create(dir.path()).unwrap();
        let mut writer = index.writer(50_000_000).unwrap();
        writer.add_chunk(1, &make_commit_chunk("aaa", "초기 커밋", "")).unwrap();
        writer.commit().unwrap();
    }
    // 새 프로세스 시뮬레이션 — open
    let index = Bm25Index::open(dir.path()).unwrap();
    let results = index.search("초기", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, 1);
}

#[test]
fn file_and_symbol_chunks_searchable() {
    let dir = TempDir::new().unwrap();
    let index = Bm25Index::create(dir.path()).unwrap();
    let mut writer = index.writer(50_000_000).unwrap();

    writer.add_chunk(
        10,
        &Chunk::WholeFile {
            commit_oid: "abc".into(),
            path: "src/search/error.rs".into(),
            content: "fn handle_io_error() -> Result<()> { Ok(()) }".into(),
        },
    ).unwrap();
    writer.add_chunk(
        11,
        &Chunk::Symbol {
            commit_oid: "abc".into(),
            path: "src/search/error.rs".into(),
            kind: gluck::search::chunk::SymbolKind::Function,
            name: "handle_io_error".into(),
            line_start: 1,
            line_end: 1,
            content: "fn handle_io_error() -> Result<()> { Ok(()) }".into(),
        },
    ).unwrap();
    writer.commit().unwrap();

    let results = index.search("handle_io_error", 10).unwrap();
    assert!(results.iter().any(|(id, _)| *id == 11));   // Symbol 매칭
}
~~~

---

### Step 7: 검증 단계

1. `cargo build` — Tantivy + 새 모듈 컴파일 확인
2. `cargo test bm25` — Step 6의 모든 테스트 통과
3. `cargo clippy` — 경고 없음
4. **수동 sanity check:** `glc` repo 자체 commit message로 인덱스 만들고 `"semantic search"` / `"버그 수정"` 같은 쿼리 결과 합리성 확인
5. **인덱스 크기 측정:** 1000개 청크 인덱싱 후 `.glc-index/bm25/` 디렉토리 크기 — 예상치 10-30MB

---

## meta.toml 호환성

`Bm25Index::open` 시 meta.toml의 `[bm25]` 섹션 검사 (v2 spec에 정의):

~~~toml
[bm25]
tokenizer = "ngram_2_2"
~~~

읽어들인 값이 `tokenizer::TOKENIZER_NAME`과 다르면 `Bm25Error::IncompatibleTokenizer`. 호출자(`SearchEngine::open`)가 이 에러를 받으면 사용자에게 "Run `glc index --force` to rebuild." 표시.

이 검사는 본 plan에서는 *함수 시그니처만 정의*하고, meta.toml 실제 read/write는 indexer-pipeline-plan에서 통합한다.

---

## Phase 2 — Out of Scope

본 plan에서 제외, 측정 후 별도 plan으로:

1. **Whitespace pre-tokenize → 단어별 ngram.** 더 깨끗한 한국어 토큰, custom Tokenizer 구현 필요.
2. **`path_tokens` 필드.** path를 `/`, `-`, `_`, `.`로 분리한 별도 토큰 필드. *path-aware* 검색 지원 (e.g., `"search error"` → `src/search/error.rs`).
3. **camelCase / snake_case splitter.** 코드 식별자 검색 품질 향상 (`parseConfig` → `["parse", "Config"]` + 원형 보존).
4. **Lindera (mecab-ko) 옵션.** 형태소 분석 기반 정확한 한국어 어간 추출. 사전 ~50MB 추가 의존성과 trade-off.
5. **Title boost 튜닝.** `title^2.0 body^1.0` 같은 가중치. A/B 측정 후 결정.
6. **BM25 K1/B 튜닝.** Tantivy 기본값(K1=1.2, B=0.75) 외 값 실험.
7. **Stop word filtering.** 한국어 stop word ("그리고", "또는" 등) + 영어 기본 stoplist 결합.

---

## Open Questions (구현 전 확정 필요)

1. **Tantivy 버전 핀:** 0.22 vs 최신(0.24+)? API 차이 미미하지만 의존성 그래프 영향 있음.
2. **`heap_size_bytes` default:** 50MB가 적절한가? 작은 repo면 과잉, 큰 repo면 부족. 인덱서 plan에서 `--heap-size` 플래그 노출 검토.
3. **`body` STORED 정책:** 본 plan은 STORED 제외 (git blob에서 재읽기). 단점은 검색 결과 미리보기에 추가 git2 호출. STORED로 두면 인덱스 크기 ~2배. **현재 결정: STORED 제외**, 미리보기 latency가 문제되면 재검토.
4. **검색 결과 limit:** v2 spec의 `bm25_top_k` default가 무엇이어야 하는가? 50 (RRF 융합 전 후보), 또는 더 많이?
5. **Tantivy 인덱스 옵션 — `IndexRecordOption::WithFreqsAndPositions` vs `WithFreqs`:** 전자는 phrase query 지원(positional index), 후자는 단순 빈도만. ngram이 이미 인접 정보를 어느 정도 capture하므로 후자로 인덱스 크기 줄일 수 있음. **현재: WithFreqsAndPositions** (안전한 default).

---

## Design Notes

### 왜 단일 tokenizer로 출발하는가

코퍼스가 다양해도, *MVP 단계에서는* 한 가지 단순한 토큰화로 시작하고 나머지는 벡터로 보완하는 게 합리적이다. 그 이유:

1. **단순함의 가치.** 두 개의 토큰화 → 두 개의 인덱싱 경로 → 두 개의 query parsing → 검증·디버깅 표면이 폭증.
2. **벡터의 보완 능력.** Model2Vec은 *식별자 단위 어휘 매칭*은 약하지만 *의미 유사도*는 잘 잡는다. BM25가 코드 식별자에 약한 부분을 벡터가 보완.
3. **측정 우선주의.** 두 번째 토큰화의 가치는 *실제 사용 패턴*에 따라 다르다. 인덱싱 사용자가 한국어 prose 위주 검색을 한다면 영문 식별자 분리는 over-engineering. *측정 후 추가*가 정공법.

### pg_bigm 패턴이 가진 의외의 장점

영택님이 AlloyDB에서 이미 검증한 패턴 — character bigram — 의 *cross-word bigram 부수 효과*가 한국어 검색에서 의외의 가치를 만든다:

- 띄어쓰기 변형에 강함 (`"에러처리"` ≈ `"에러 처리"`)
- 조사 변형에도 부분적 강함 (`"에러를"`의 `["에러", "러를"]`는 `"에러"` 검색과 부분 매칭)
- 형태소 분석 사전 없이 작동

이는 *기능이 아니라 부수 효과로 얻는 robustness*다. semantic search가 contextual embedding의 부수 효과로 paraphrase robustness를 얻는 것과 같은 패턴.

### `Bm25Index`가 `VectorIndex`와 같은 인터페이스를 갖는 의도

| | `Bm25Index::search` | `VectorIndex::search` |
|---|---|---|
| 입력 | `&str` | `&[f32]` |
| 출력 | `Vec<(u64, f32)>` | `Vec<(u64, f32)>` |
| 의미 | BM25 score | cosine similarity |

같은 출력 타입을 갖는 의도는 v2 spec design notes에서 이미 짚었지만 — RRF fusion이 *입력 형태에 무관한 합성 함수*로 작성될 수 있다. ranked list 두 개의 합성, 그게 전부. 이 깔끔함이 향후 두 검색 백엔드 중 어느 쪽을 교체해도 fusion 코드가 무영향이라는 보장으로 이어진다.
