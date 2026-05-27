# 검색 품질 개선 구현 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `glc report` MRR을 0.330 → ≥0.65로, Recall@5를 0.286 → ≥0.65로 끌어올린다. BM25 인덱스의 식별자/path 매칭 능력을 강화하고, 일부 파일에서 누락되는 심볼을 잡는다.

**Architecture:** (1) write-time 전처리로 identifier(snake_case + camelCase)를 단어로 분해해 SimpleTokenizer가 처리할 수 있게 한다. (2) `path_terms` 신규 BM25 필드를 추가해 path를 단어 단위로 검색한다. (3) WholeFile 임계값을 16KB로 올리고 Rust 심볼 추출에서 누락된 top-level `trait_item` / `type_item`을 추가한다. (4) `INDEX_VERSION`을 5→6으로 범프해 자동 풀 리빌드한다.

**Tech Stack:** Rust, tantivy 0.22 (SimpleTokenizer + LowerCaser), tree-sitter 0.22, tree-sitter-rust.

**Spec correction (구현 시 반영):**
- Spec 3.2.1의 "IdentifierSplit TokenFilter"는 tantivy 0.22 GAT 복잡도를 피해 **write-time 전처리 헬퍼 + SimpleTokenizer + LowerCaser** 조합으로 동등 효과 구현.
- Spec 3.3.2의 "enum/struct/trait/type 추가"는 부분 정정: enum/struct/impl-method/trait-method는 **이미 추출 중**. 진짜 누락은 **top-level `trait_item`(trait 선언 자체)** 과 **`type_item`(type alias)**.

---

## File Structure

| 파일 | 책임 | 변경 |
|---|---|---|
| `src/search/text_prep.rs` | 식별자/path 전처리 (snake_case는 SimpleTokenizer가 처리, camelCase·path-separator만 공백 변환) | **신규** |
| `src/search/bm25.rs` | BM25 스키마 + 토크나이저 + 쿼리. `path_terms` 필드 추가, title은 SimpleTokenizer+LowerCaser, body는 ngram_2_2 유지 | 수정 |
| `src/search/chunk/file.rs` | `WHOLE_FILE_THRESHOLD: 8KB → 16KB` | 1줄 수정 |
| `src/search/chunk/symbol.rs` | Rust 쿼리에 top-level `trait_item`, `type_item` 추가. `SymbolKind::TypeAlias` 신규 | 수정 |
| `src/search/chunk/mod.rs` | `SymbolKind::TypeAlias` re-export 확인 | 변경 없음 (이미 pub use) |
| `src/search/indexer.rs` | `chunk_to_meta`의 Symbol 분기에서 `TypeAlias` 매칭 | 1~2줄 수정 |
| `src/search/mod.rs` | `INDEX_VERSION: 5 → 6`, `text_prep` mod 추가 | 2줄 수정 |
| `tests/fixtures/search_queries.toml` | `incremental indexing fallback` 정답에서 `diff.rs` 제거 | 1 entry 수정 |

---

## Task 1: Fixture 정제

**Files:**
- Modify: `tests/fixtures/search_queries.toml`

- [ ] **Step 1: 정답 1건 수정**

`tests/fixtures/search_queries.toml`의 4~9번째 줄을 다음으로 교체:

```toml
[[query]]
text = "incremental indexing fallback"
expected = [
    { path = "src/search/indexer.rs", kind = "Symbol", title = "build_index_incremental" },
]
```

(기존의 `{ path = "src/search/diff.rs" }` 항목 제거. 다른 쿼리는 모두 그대로.)

- [ ] **Step 2: 변경 검증**

Run: `git diff tests/fixtures/search_queries.toml`
Expected: 첫 query만 변경, 다른 6개 query block 영향 없음.

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/search_queries.toml
git commit -m "Drop unreachable diff.rs from incremental indexing fixture answer"
```

---

## Task 2: 텍스트 전처리 헬퍼 (`text_prep.rs`)

camelCase 경계에 공백을 삽입하는 단순 함수. snake_case와 path separator(`/`, `.`, `-`)는 SimpleTokenizer가 알아서 분해하므로 별도 처리 불필요.

**Files:**
- Create: `src/search/text_prep.rs`
- Modify: `src/search/mod.rs` (mod 등록)

- [ ] **Step 1: 실패 테스트 작성**

`src/search/text_prep.rs` 파일을 다음 내용으로 생성:

```rust
/// 식별자/Path 텍스트를 SimpleTokenizer가 단어 단위로 분해할 수 있게 전처리한다.
///
/// SimpleTokenizer는 `_`, `/`, `.`, `-` 등 비-alphanumeric 문자에서 자동 분해하지만
/// camelCase는 인식하지 못한다. 이 함수는 camelCase 경계(소문자 → 대문자, 글자 → 숫자)에
/// 공백을 삽입해서 `BuildIndex` → `Build Index`, `Rev2` → `Rev 2`로 만든다.
///
/// 한글 등 비-ASCII alphabet은 case 개념이 없어 변환되지 않음.
pub fn split_camel_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_lower = false;
    let mut prev_digit = false;
    for c in s.chars() {
        let is_upper = c.is_ascii_uppercase();
        let is_digit = c.is_ascii_digit();
        let is_lower = c.is_ascii_lowercase();
        if (is_upper && prev_lower) || (is_digit && !prev_digit && (prev_lower || /* prev_upper */ false)) {
            out.push(' ');
        }
        out.push(c);
        prev_lower = is_lower || is_upper; // 둘 다 alpha
        prev_digit = is_digit;
    }
    out
}

/// Path를 단어 후보로 만들기 위해 path separator를 공백으로 치환한 뒤
/// `split_camel_case`를 적용한다.
pub fn path_to_terms(path: &str) -> String {
    let replaced: String = path
        .chars()
        .map(|c| if matches!(c, '/' | '.' | '-' | '_' | '\\') { ' ' } else { c })
        .collect();
    split_camel_case(&replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_unchanged_by_split() {
        // SimpleTokenizer가 _를 알아서 분해하므로 split_camel_case는 손대지 않음
        assert_eq!(split_camel_case("rrf_fuse"), "rrf_fuse");
        assert_eq!(split_camel_case("build_index_incremental"), "build_index_incremental");
    }

    #[test]
    fn camel_case_split() {
        assert_eq!(split_camel_case("BuildIndex"), "Build Index");
        assert_eq!(split_camel_case("ModalState"), "Modal State");
        assert_eq!(split_camel_case("HTTPServer"), "HTTPServer"); // 연속 대문자는 split 안 함
    }

    #[test]
    fn mixed_identifier() {
        assert_eq!(split_camel_case("buildIndexFor"), "build Index For");
    }

    #[test]
    fn path_terms_replaces_separators() {
        assert_eq!(path_to_terms("src/search/rrf.rs"), "src search rrf rs");
        assert_eq!(path_to_terms("src/git/store.rs"), "src git store rs");
    }

    #[test]
    fn path_terms_with_camel_case_file() {
        assert_eq!(path_to_terms("src/search/ModalState.rs"), "src search Modal State rs");
    }

    #[test]
    fn empty_string() {
        assert_eq!(split_camel_case(""), "");
        assert_eq!(path_to_terms(""), "");
    }

    #[test]
    fn korean_passthrough() {
        // 한글은 case 개념이 없어 변환되지 않음
        assert_eq!(split_camel_case("한글이름"), "한글이름");
    }
}
```

- [ ] **Step 2: mod 등록**

`src/search/mod.rs`의 module 선언 블록(`pub mod bm25;`로 시작하는 부분, 1~10번째 줄)에 추가:

```rust
pub mod text_prep;
```

알파벳 순서를 따른다면 `silence` 다음, `vector` 앞.

- [ ] **Step 3: 테스트 실행으로 정의된 동작 확인**

Run: `cargo test --lib search::text_prep`
Expected: 7개 테스트 모두 PASS.

- [ ] **Step 4: Commit**

```bash
git add src/search/text_prep.rs src/search/mod.rs
git commit -m "Add text_prep helpers for camelCase split and path tokenization"
```

---

## Task 3: BM25 스키마에 `path_terms` 필드 + title 토크나이저 변경

**Files:**
- Modify: `src/search/bm25.rs`

- [ ] **Step 1: 토크나이저 상수 추가**

`src/search/bm25.rs`의 14번째 줄(`pub const TOKENIZER: &str = "ngram_2_2";` 부근)에 인접하게 추가:

```rust
pub const TOKENIZER: &str = "ngram_2_2";
pub const WORD_TOKENIZER: &str = "word_lower";
```

- [ ] **Step 2: `Bm25Fields` 구조체에 `path_terms` 필드 추가**

기존 `Bm25Fields` (17~26번째 줄 부근):

```rust
pub struct Bm25Fields {
    pub id: Field,
    pub kind: Field,
    pub title: Field,
    pub body: Field,
    pub path: Field,
    pub commit_oid: Field,
    pub line_start: Field,
    pub line_end: Field,
}
```

다음으로 변경 (path 다음에 `path_terms` 추가):

```rust
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
```

- [ ] **Step 3: `make_schema()` 변경**

기존 `make_schema()` 함수(34~71번째 줄)를 다음으로 교체:

```rust
fn make_schema() -> (Schema, Bm25Fields) {
    let mut builder = Schema::builder();

    // Title: 식별자/path 단어 단위 매칭. SimpleTokenizer가 _/./- 등에서 분해, LowerCaser가 case 정규화.
    // 한글 부분 매칭은 body 필드의 ngram이 담당.
    let title_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(WORD_TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    // Path terms: title과 같은 토크나이저, 검색 전용 (저장 안 함).
    let path_terms_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(WORD_TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqs),
    );

    // Body: 한글/임의 텍스트 부분 매칭. 기존 ngram_2_2 유지.
    let body_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqs),
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
```

- [ ] **Step 4: `register_tokenizer()` 변경**

기존 (73~79번째 줄):

```rust
fn register_tokenizer(index: &Index) {
    let tokenizer =
        TextAnalyzer::builder(NgramTokenizer::new(2, 2, false).expect("valid ngram params"))
            .filter(LowerCaser)
            .build();
    index.tokenizers().register(TOKENIZER, tokenizer);
}
```

다음으로 교체:

```rust
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
```

그리고 파일 상단(9번째 줄)의 use 문에 `SimpleTokenizer` 추가:

```rust
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, SimpleTokenizer, TextAnalyzer};
```

- [ ] **Step 5: 컴파일 확인**

Run: `cargo build --lib`
Expected: 컴파일 성공. (이 시점에서 `add_doc`/`search`가 `path_terms`를 모르지만 필드는 schema에 있어도 add_text 안 하면 빈 채로 둠 — 컴파일 OK.)

- [ ] **Step 6: 회귀 테스트 실행**

Run: `cargo test --lib search::bm25`
Expected: 기존 테스트 9개 중:
- `test_korean_bigram_search`: body 필드 ngram_2_2 유지 → PASS
- `test_uppercase_indexed_matches_lowercase_query`: title이 WORD_TOKENIZER + LowerCaser로 바뀌었지만 "he" 쿼리는 SimpleTokenizer로는 hello 전체 매칭이 안 됨 → **FAIL 예상**. 이 테스트는 ngram 동작에 의존했음.

이 테스트는 다음 단계에서 의미를 재정의한다.

- [ ] **Step 7: 기존 `test_uppercase_indexed_matches_lowercase_query` 의미 재정의**

`src/search/bm25.rs`의 해당 테스트(`fn test_uppercase_indexed_matches_lowercase_query` 부근, ~337번째 줄)를 다음으로 교체:

```rust
#[test]
fn test_word_tokenizer_lowercases_title() {
    // Title 필드는 SimpleTokenizer + LowerCaser. "Hello" 인덱싱 후 "hello" 쿼리로 매칭.
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
```

`test_create_and_search_basic` (288~298번째 줄)에서 `idx.search("he", 10)`도 이제 단어 단위로 동작하므로 "he"는 "hello"와 매칭 안 됨. 다음으로 수정:

```rust
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
```

`test_cached_reader_sees_data_across_multiple_commits` (~351번째 줄)의 `idx.search("fi", 10)` / `idx.search("se", 10)`도 단어 매칭으로 바뀌어 fail. 다음으로 수정:

```rust
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
```

`test_delete_doc_removes_from_search` (~397번째 줄)의 `idx.search("he", 10)`도 같은 이유:

```rust
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
```

- [ ] **Step 8: 회귀 테스트 재실행**

Run: `cargo test --lib search::bm25`
Expected: 모든 테스트 PASS.

- [ ] **Step 9: Commit**

```bash
git add src/search/bm25.rs
git commit -m "Add path_terms field and word tokenizer to BM25 schema"
```

---

## Task 4: `add_doc`에서 path/title 전처리 + path_terms 채우기

**Files:**
- Modify: `src/search/bm25.rs`

- [ ] **Step 1: `add_doc`이 전처리하도록 수정**

`src/search/bm25.rs::add_doc` (117~140번째 줄):

```rust
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
```

다음으로 변경:

```rust
pub fn add_doc(
    &self,
    writer: &mut IndexWriter,
    meta: &DocMeta,
    body: &str,
) -> Result<(), TantivyError> {
    use crate::search::text_prep::{path_to_terms, split_camel_case};
    let mut doc = tantivy::TantivyDocument::default();
    doc.add_u64(self.fields.id, meta.doc_id);
    doc.add_text(self.fields.kind, meta.kind.as_str());
    // Title은 camelCase split만 전처리 — _ / . - 등은 SimpleTokenizer가 처리.
    doc.add_text(self.fields.title, &split_camel_case(&meta.title));
    doc.add_text(self.fields.body, body);
    doc.add_text(self.fields.commit_oid, &meta.commit_oid);
    if let Some(p) = &meta.path {
        doc.add_text(self.fields.path, p);
        doc.add_text(self.fields.path_terms, &path_to_terms(p));
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
```

- [ ] **Step 2: path_terms 동작 단위 테스트 추가**

`src/search/bm25.rs::tests` 모듈의 끝(`}` 직전)에 추가:

```rust
#[test]
fn test_path_terms_matches_path_segment_query() {
    let (_dir, idx) = tmp_index();
    let mut w = idx.writer().unwrap();
    let meta = DocMeta {
        doc_id: 1,
        kind: DocKind::File,
        title: "src/search/rrf.rs".into(),
        commit_oid: "a".repeat(40),
        path: Some("src/search/rrf.rs".into()),
        line_start: None,
        line_end: None,
    };
    idx.add_doc(&mut w, &meta, "fn rrf_fuse() {}").unwrap();
    idx.commit(w).unwrap();
    // path_terms 필드는 QueryParser default field에 포함되므로 "rrf" 쿼리로 매칭되어야 함.
    let results = idx.search("rrf", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, 1);
}

#[test]
fn test_camel_case_title_split_for_query() {
    let (_dir, idx) = tmp_index();
    let mut w = idx.writer().unwrap();
    let meta = DocMeta {
        doc_id: 7,
        kind: DocKind::Symbol,
        title: "ModalState (src/search/modal_state.rs)".into(),
        commit_oid: "b".repeat(40),
        path: Some("src/search/modal_state.rs".into()),
        line_start: Some(1),
        line_end: Some(10),
    };
    idx.add_doc(&mut w, &meta, "enum ModalState {}").unwrap();
    idx.commit(w).unwrap();
    // CamelCase split → "Modal State" → 소문자 매칭
    let r = idx.search("modal", 10).unwrap();
    assert!(r.iter().any(|(id, _)| *id == 7), "modal must match split ModalState");
    let r = idx.search("state", 10).unwrap();
    assert!(r.iter().any(|(id, _)| *id == 7), "state must match split ModalState");
}
```

- [ ] **Step 3: 테스트 실행**

Run: `cargo test --lib search::bm25`
Expected: 새 테스트 2개 포함 전부 PASS.

- [ ] **Step 4: Commit**

```bash
git add src/search/bm25.rs
git commit -m "Write path_terms and camelCase-split title in BM25 add_doc"
```

---

## Task 5: QueryParser에 path_terms 포함 + boost

**Files:**
- Modify: `src/search/bm25.rs`

- [ ] **Step 1: `search()` 메서드의 QueryParser 변경**

`src/search/bm25.rs::search` (153~169번째 줄)에서 QueryParser 생성 부분:

```rust
let parser = QueryParser::for_index(&self.index, vec![self.fields.title, self.fields.body]);
```

다음으로 변경 (path_terms 포함 + boost):

```rust
let mut parser = QueryParser::for_index(
    &self.index,
    vec![self.fields.title, self.fields.path_terms, self.fields.body],
);
parser.set_field_boost(self.fields.title, 2.0);
parser.set_field_boost(self.fields.path_terms, 2.0);
parser.set_field_boost(self.fields.body, 1.0);
```

(`let parser`이 아니라 `let mut parser`로 변경하는 것에 주의.)

- [ ] **Step 2: path_terms 부스트가 path 매칭 우선순위에 영향을 주는지 검증하는 테스트 추가**

`src/search/bm25.rs::tests`에 추가:

```rust
#[test]
fn test_path_match_outranks_unrelated_body_match() {
    let (_dir, idx) = tmp_index();
    let mut w = idx.writer().unwrap();

    // Doc 1: path가 정확히 매칭하지만 body에 store 단어 없음
    let target = DocMeta {
        doc_id: 1,
        kind: DocKind::File,
        title: "src/git/store.rs".into(),
        commit_oid: "a".repeat(40),
        path: Some("src/git/store.rs".into()),
        line_start: None,
        line_end: None,
    };
    idx.add_doc(&mut w, &target, "fn open() {}").unwrap();

    // Doc 2: body에 store가 흩어져 있지만 path 무관
    let distractor = DocMeta {
        doc_id: 2,
        kind: DocKind::File,
        title: "src/ui/view.rs".into(),
        commit_oid: "b".repeat(40),
        path: Some("src/ui/view.rs".into()),
        line_start: None,
        line_end: None,
    };
    idx.add_doc(&mut w, &distractor, "store store store").unwrap();

    idx.commit(w).unwrap();

    let results = idx.search("store", 10).unwrap();
    // path_terms boost 2.0이 path 매칭 doc을 상위로 끌어올려야 함.
    assert!(results.iter().any(|(id, _)| *id == 1));
    let pos_1 = results.iter().position(|(id, _)| *id == 1).unwrap();
    let pos_2 = results.iter().position(|(id, _)| *id == 2);
    if let Some(p2) = pos_2 {
        assert!(pos_1 <= p2, "path-matching doc 1 should rank ≤ body-only doc 2");
    }
}
```

- [ ] **Step 3: 테스트 실행**

Run: `cargo test --lib search::bm25`
Expected: 모든 테스트 PASS.

- [ ] **Step 4: 클리피 + 포맷 확인**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 변경한 파일에 대해 새 warning 없음.

Run: `rustfmt src/search/bm25.rs src/search/text_prep.rs`

- [ ] **Step 5: Commit**

```bash
git add src/search/bm25.rs
git commit -m "Include path_terms in query parser with boost for path matching"
```

---

## Task 6: WholeFile 임계값 상향

**Files:**
- Modify: `src/search/chunk/file.rs`

- [ ] **Step 1: 테스트 먼저 — 12KB 파일이 WholeFile로 잡혀야 함**

`src/search/chunk/file.rs::tests` 모듈 (59번째 줄 부근) 끝에 추가:

```rust
#[test]
fn twelve_kb_rust_file_stays_whole_file() {
    // 16KB 임계값 가정 — 12KB Rust 파일은 Symbol 분할이 아닌 WholeFile로 잡혀야 함.
    let big = format!(
        "fn foo() {{\n{}\n}}\n",
        "    let x = 1;\n".repeat(800),
    );
    assert!(big.len() > 8 * 1024 && big.len() < 16 * 1024, "test fixture sizing: got {}", big.len());
    let chunks = split_file("oid", "medium.rs", &big);
    assert_eq!(chunks.len(), 1, "12KB file should be single WholeFile");
    assert!(matches!(chunks[0], Chunk::WholeFile { .. }));
}
```

- [ ] **Step 2: 테스트 실행 — 기존 8KB 임계로 FAIL 예상**

Run: `cargo test --lib chunk::file::tests::twelve_kb_rust_file_stays_whole_file`
Expected: FAIL — 8KB 임계로 Symbol 분할됨.

- [ ] **Step 3: 임계값 변경**

`src/search/chunk/file.rs:6`:

```rust
const WHOLE_FILE_THRESHOLD: usize = 8 * 1024; // 8 KB
```

→

```rust
const WHOLE_FILE_THRESHOLD: usize = 16 * 1024; // 16 KB — modal_state.rs, store.rs 등을 WholeFile로 보존
```

- [ ] **Step 4: 테스트 재실행**

Run: `cargo test --lib chunk::file`
Expected: 모든 테스트 PASS (새 테스트 + 기존 5개).

- [ ] **Step 5: Commit**

```bash
git add src/search/chunk/file.rs
git commit -m "Raise WholeFile threshold to 16KB to keep mid-size files searchable by path"
```

---

## Task 7: Rust top-level `trait_item` / `type_item` 추출 추가

**Files:**
- Modify: `src/search/chunk/symbol.rs`

- [ ] **Step 1: 실패 테스트 작성**

`src/search/chunk/symbol.rs::tests` 끝(`}` 직전, ~376번째 줄)에 추가:

```rust
#[test]
fn rust_top_level_trait_extracted() {
    let src = r#"
trait Greet {
    fn name(&self) -> &str;
    fn hello(&self) -> String { String::new() }
}
"#;
    let spans = extract_symbols(src, Language::Rust).unwrap();
    let has_trait_container = spans
        .iter()
        .any(|s| s.kind == SymbolKind::Trait && s.name == "Greet");
    assert!(
        has_trait_container,
        "top-level trait declaration must be extracted as Trait, not only its methods"
    );
}

#[test]
fn rust_top_level_type_alias_extracted() {
    let src = r#"
type CommitId = String;
type Result<T> = std::result::Result<T, MyError>;
"#;
    let spans = extract_symbols(src, Language::Rust).unwrap();
    let names: Vec<_> = spans
        .iter()
        .filter(|s| s.kind == SymbolKind::TypeAlias)
        .map(|s| s.name.as_str())
        .collect();
    assert!(names.contains(&"CommitId"));
    assert!(names.contains(&"Result"));
}
```

- [ ] **Step 2: 테스트 실행 — `SymbolKind::TypeAlias` 부재로 컴파일 FAIL**

Run: `cargo test --lib chunk::symbol -- --no-run`
Expected: 컴파일 에러 — `SymbolKind::TypeAlias` not found.

- [ ] **Step 3: `SymbolKind`에 `TypeAlias` 추가**

`src/search/chunk/symbol.rs:7-16`의 `SymbolKind` enum을:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Class,
    Other,
}
```

다음으로 교체:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Class,
    Other,
}
```

- [ ] **Step 4: Rust 쿼리에 `trait_item` (top-level) + `type_item` 추가**

`src/search/chunk/symbol.rs:191-210`의 `RUST_QUERY` 상수를 다음으로 교체:

```rust
const RUST_QUERY: &str = r#"
((source_file
   (function_item name: (identifier) @name) @symbol.function))

((source_file
   (struct_item name: (type_identifier) @name) @symbol.struct))

((source_file
   (enum_item name: (type_identifier) @name) @symbol.enum))

((source_file
   (trait_item name: (type_identifier) @name) @symbol.trait))

((source_file
   (type_item name: (type_identifier) @name) @symbol.type))

((source_file
   (impl_item
     (declaration_list
       (function_item name: (identifier) @name) @symbol.method))))

((source_file
   (trait_item
     (declaration_list
       (function_item name: (identifier) @name) @symbol.method))))
"#;
```

- [ ] **Step 5: `build_symbol_span()` capture name 매칭에 `symbol.trait`, `symbol.type` 추가**

`src/search/chunk/symbol.rs:68-97`의 capture name match 블록에 추가 (`"symbol.trait"` 케이스는 이미 있지만 다시 확인, `"symbol.type"`은 신규):

```rust
"symbol.trait" => {
    symbol_node = Some(cap.node);
    kind = SymbolKind::Trait;
}
"symbol.type" => {
    symbol_node = Some(cap.node);
    kind = SymbolKind::TypeAlias;
}
```

(기존에 `"symbol.trait"` 케이스가 이미 있다면 중복 추가하지 말고 `"symbol.type"`만 신규로 추가.)

- [ ] **Step 6: 회귀 테스트 실행**

Run: `cargo test --lib chunk::symbol`
Expected:
- 새 테스트 2개 PASS
- 기존 `rust_trait_default_methods_extracted`는 trait method 추출이 그대로라 PASS
- 다른 모든 테스트 PASS

- [ ] **Step 7: `chunk_to_meta`에서 새 `SymbolKind` 처리 확인**

`src/search/indexer.rs::chunk_to_meta` (395~435번째 줄) — `Chunk::Symbol` 분기는 `kind`를 직접 사용하지 않고 `DocKind::Symbol`로 변환한다. `SymbolKind::TypeAlias` 추가는 영향 없음. 확인만:

Run: `cargo build --lib`
Expected: 컴파일 성공.

- [ ] **Step 8: Commit**

```bash
git add src/search/chunk/symbol.rs
git commit -m "Extract top-level trait_item and type_item as Rust symbols"
```

---

## Task 8: INDEX_VERSION 범프

**Files:**
- Modify: `src/search/mod.rs`

- [ ] **Step 1: 버전 상수 변경**

`src/search/mod.rs:209`:

```rust
pub const INDEX_VERSION: u32 = 5;
```

→

```rust
pub const INDEX_VERSION: u32 = 6;
```

- [ ] **Step 2: 변경된 버전이 기존 인덱스를 거부하는지 확인하는 회귀 테스트는 이미 존재**

`open_fails_on_tokenizer_mismatch`와 `IndexMeta`의 버전 검사 로직(`SearchEngine::open` 122~128번째 줄)이 이미 VersionMismatch를 던지므로 별도 테스트 추가 불필요.

Run: `cargo test --lib search`
Expected: 모든 테스트 PASS.

- [ ] **Step 3: Commit**

```bash
git add src/search/mod.rs
git commit -m "Bump INDEX_VERSION to 6 for path_terms + word tokenizer schema"
```

---

## Task 9: 전체 빌드 + 회귀

**Files:** (검증 전용, 변경 없음)

- [ ] **Step 1: 전체 테스트**

Run: `cargo test`
Expected: 모든 테스트 PASS (ignored 제외).

- [ ] **Step 2: Clippy 전체**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 변경한 파일에 새 warning 없음.

- [ ] **Step 3: 포맷 확인**

Run: `rustfmt --check src/search/text_prep.rs src/search/bm25.rs src/search/chunk/file.rs src/search/chunk/symbol.rs src/search/mod.rs`
Expected: 출력 없음 (이미 포맷됨).

만약 차이가 있다면:

Run: `rustfmt src/search/text_prep.rs src/search/bm25.rs src/search/chunk/file.rs src/search/chunk/symbol.rs src/search/mod.rs`

---

## Task 10: End-to-end 검증 (`glc report`)

**Files:** (검증 전용)

- [ ] **Step 1: 인덱스 재빌드**

Run: `cargo run --release --bin glc -- index --force`
Expected: 로그 마지막에 "Indexed N documents." 출력. INDEX_VERSION 6로 새 인덱스가 `./.glc-index/`에 생성됨.

- [ ] **Step 2: 리포트 생성**

Run: `cargo run --release --bin glc -- report --out result.md`
Expected: stdout에 aggregate 메트릭 + per-query 표 출력. `result.md` 갱신.

- [ ] **Step 3: 합격 기준 검증**

`result.md` 읽고 아래 기준 충족 여부 확인:

| 메트릭 | 베이스라인 | 목표 |
|---|---|---|
| MRR | 0.330 | ≥ 0.65 |
| Recall@5 | 0.286 | ≥ 0.65 |
| Recall@10 | 0.571 | ≥ 0.85 |
| NDCG@10 | 0.384 | ≥ 0.65 |

쿼리별:

| 쿼리 | 베이스라인 Hit Rank | 목표 |
|---|---|---|
| incremental indexing fallback | — | ≤ 5 |
| tantivy delete_term | 1 | 1 (회귀 없음) |
| RRF reciprocal rank fusion | — | ≤ 5 |
| embedding model load potion | 1 | 1 (회귀 없음) |
| search modal state machine | — | ≤ 5 |
| tree sitter highlight configuration | 6 | ≤ 5 |
| git revwalk topological commit | 7 | ≤ 5 |

- [ ] **Step 4: 결과 commit**

목표 충족 시:

```bash
git add result.md
git commit -m "Update search quality report after BM25 path_terms and word tokenizer rollout"
```

목표 미달 시: 어떤 쿼리가 미달인지 확인하고, 후속 라운드(RRF k 튜닝, embed_text 보강, 모듈 docstring 청크) 중 하나를 선택해 별도 plan 작성. 이번 plan은 부분 커밋한 채로 종료하고 사용자와 다음 단계 논의.

---

## Self-Review (작성자 체크)

- **Spec coverage:**
  - Spec 3.1 (fixture 정제) → Task 1 ✓
  - Spec 3.2.1 (code_ident 토크나이저) → Task 2 (text_prep) + Task 3 (WORD_TOKENIZER) ✓
  - Spec 3.2.2 (path_terms 필드) → Task 3 + Task 4 ✓
  - Spec 3.2.3 (title 토크나이저 교체) → Task 3 ✓
  - Spec 3.2.4 (body 그대로) → Task 3 ✓
  - Spec 3.2.5 (QueryParser boost) → Task 5 ✓
  - Spec 3.3.1 (WHOLE_FILE_THRESHOLD 16KB) → Task 6 ✓
  - Spec 3.3.2 (Symbol 확장) → Task 7 (실제 누락분만 정정 반영) ✓
  - Spec 3.4 (path → path_terms 분해 로직) → Task 2 (`path_to_terms`) + Task 4 (add_doc) ✓
  - Spec 3.5 (INDEX_VERSION 5→6) → Task 8 ✓
  - Spec 3.6 (RRF/임베딩 변경 없음) → 명시적 미작업 ✓
  - Spec 4 (테스트) → Task 2/3/4/5/6/7 인라인 ✓
  - Spec 7 (작업 순서) → Task 1~10 ✓

- **Placeholder scan:** "TBD"/"TODO" 없음. 모든 코드 블록은 완성된 형태.

- **Type consistency:**
  - `WORD_TOKENIZER` 상수는 Task 3에서 정의, 같은 Task의 register/스키마에서 사용 ✓
  - `path_terms` 필드 이름은 Task 3 정의, Task 4 add_doc, Task 5 QueryParser에서 동일하게 사용 ✓
  - `path_to_terms`/`split_camel_case` 함수 시그니처는 Task 2 정의, Task 4에서 호출 일치 ✓
  - `SymbolKind::TypeAlias`는 Task 7 Step 3에서 enum에 추가, Step 5에서 매칭 ✓
