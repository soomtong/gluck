# BM25 Schema & Reader Upgrade Plan

## Status

- **Type:** Implementation plan
- **Predecessor:** `2026-05-25-tantivy-korean-tokenizer-plan.md` (items 1–3 완료)
- **Implements items:** #4 Tokenizer 호환성 검사, #5 IndexReader 캐싱, #6 doc_id u64 FAST + 별도 path 필드

## 범위 요약

| # | 항목 | 난이도 | 영향 |
|---|---|---|---|
| 4 | Tokenizer 호환성 검사 | 중 | 안전망 |
| 5 | IndexReader 캐싱 | 중 | UI latency |
| 6 | doc_id u64 FAST + path STRING 필드 분리 | 높음 | 스키마 변경 → INDEX_VERSION bump 필요 |

---

## 현재 상태 (2026-05-25 기준)

```
src/search/bm25.rs     — 단일 파일, TOKENIZER = "ngram_2_2", LowerCaser 적용됨
src/search/mod.rs      — SearchEngine::open: version만 비교, tokenizer 불검사
src/search/indexer.rs  — INDEX_VERSION = 3
```

현재 스키마 필드:
- `doc_id`: `STORED` (TEXT로 저장된 u64)
- `title`: `TEXT`, ngram_2_2, STORED
- `body`: `TEXT`, ngram_2_2, not STORED
- `meta_json`: `STORED` (DocMeta 전체를 JSON 직렬화)

---

## Item 4: Tokenizer 호환성 검사

### 목표

`SearchEngine::open` 시 meta.toml의 `[bm25] tokenizer` 값이 현재 코드의 `TOKENIZER` 상수와 다르면
즉시 에러 반환 → 사용자에게 `glc index --force` 안내.

### 변경 파일

- `src/search/mod.rs`

### 구현

`SearchEngine::open` 내부:

```rust
// mod.rs
let meta_str = std::fs::read_to_string(&meta_path)?;
let meta: IndexMeta = toml::from_str(&meta_str)?;

if meta.version != INDEX_VERSION {
    return Err(SearchError::VersionMismatch { ... });
}
// 추가:
if meta.bm25.tokenizer != crate::search::bm25::TOKENIZER {
    return Err(SearchError::IncompatibleTokenizer {
        expected: crate::search::bm25::TOKENIZER.to_string(),
        found: meta.bm25.tokenizer.clone(),
    });
}
```

`SearchError` 에 variant 추가:

```rust
#[error("BM25 tokenizer mismatch: expected '{expected}', found '{found}' — run `glc index --force`")]
IncompatibleTokenizer { expected: String, found: String },
```

### 테스트

```rust
// src/search/mod.rs (또는 indexer tests)
#[test]
fn open_fails_on_tokenizer_mismatch() {
    // meta.toml에 tokenizer = "old_tokenizer" 기록 후 open 시도
    // SearchError::IncompatibleTokenizer 반환 확인
}
```

---

## Item 5: IndexReader 캐싱

### 목표

현재 `search()` 호출마다 `self.index.reader()` 로 새 reader 생성. `ReloadPolicy::OnCommit`으로
reader를 `Bm25Index` 구조체에 캐싱하면 인터랙티브 타이핑 환경에서 latency 감소.

### 변경 파일

- `src/search/bm25.rs`

### 구현

```rust
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyError};

pub struct Bm25Index {
    index: Index,
    reader: IndexReader,   // 추가
    fields: Bm25Fields,
}

impl Bm25Index {
    pub fn create(dir: &Path) -> Result<Self, SearchError> {
        ...
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()?;
        Ok(Self { index, reader, fields })
    }

    pub fn open(dir: PathBuf) -> Result<Self, SearchError> {
        ...
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()?;
        Ok(Self { index, reader, fields })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(u64, f32)>, SearchError> {
        let searcher = self.reader.searcher();  // ← self.index.reader() 제거
        ...
    }
}
```

### 테스트

```rust
#[test]
fn reader_sees_committed_data_without_recreation() {
    // create → write → commit → search → should find doc
    // 동일 인스턴스로 두 번 search → 두 번 모두 같은 결과
    let (_dir, idx) = tmp_index();
    let mut w = idx.writer().unwrap();
    idx.add_doc(&mut w, 1, "에러", "", &meta_json(1, "에러")).unwrap();
    idx.commit(w).unwrap();
    let r1 = idx.search("에러", 10).unwrap();
    let r2 = idx.search("에러", 10).unwrap();
    assert_eq!(r1.len(), r2.len());
    assert!(!r1.is_empty());
}
```

---

## Item 6: doc_id u64 FAST + path STRING 필드 분리

### 목표

현재 `doc_id`가 TEXT로 저장되어 매 결과마다 `parse::<u64>()` 필요.
`path`가 `meta_json` 안에 직렬화되어 있어 필드 쿼리 (`path:src/search`) 불가.

변경 후:
- `id`: `FAST | STORED` u64 — RRF fusion join key
- `kind`: `STRING | STORED` — 필드 필터 쿼리 가능
- `path`: `STRING | STORED` — `path:src/search` 쿼리 가능
- `commit_oid`: `STRING | STORED` — 결과 hydrate용
- `line_start`, `line_end`: `STORED` u64
- `meta_json` **제거** — 모든 정보가 구조화 필드에 있으므로 불필요

### 스키마 변경 → INDEX_VERSION 4로 bump

스키마가 바뀌면 기존 인덱스와 호환 불가. `INDEX_VERSION` 4로 올리면
`SearchEngine::open`이 자동으로 `VersionMismatch` 에러 반환 → 사용자에게 재인덱싱 안내.

### 변경 파일

- `src/search/bm25.rs` — `make_schema()`, `add_doc()`, `search()`, `scan_doc_store()`
- `src/search/mod.rs` — `INDEX_VERSION = 4`
- `src/search/indexer.rs` — `add_doc()` 호출부 (파라미터 변경 반영)

### 신규 스키마 설계

```rust
// make_schema() 변경 후
let id         = builder.add_u64_field  ("id",         FAST | STORED);
let kind       = builder.add_text_field ("kind",       STRING | STORED);
let title      = builder.add_text_field ("title",      text_options);      // ngram, STORED
let body       = builder.add_text_field ("body",       body_options);      // ngram, not STORED
let path       = builder.add_text_field ("path",       STRING | STORED);
let commit_oid = builder.add_text_field ("commit_oid", STRING | STORED);
let line_start = builder.add_u64_field  ("line_start", STORED);
let line_end   = builder.add_u64_field  ("line_end",   STORED);
```

### add_doc 시그니처 변경

현재:
```rust
pub fn add_doc(&self, writer, doc_id, title, body, meta_json) -> ...
```

변경 후 — `meta_json` 대신 구조화 파라미터:
```rust
pub fn add_doc(
    &self,
    writer: &mut IndexWriter,
    doc_id: u64,
    kind: &str,
    title: &str,
    body: &str,
    path: Option<&str>,
    commit_oid: &str,
    line_start: Option<u32>,
    line_end: Option<u32>,
) -> Result<(), TantivyError>
```

### scan_doc_store 변경

현재: AllQuery → JSON parse. 변경 후: 구조화 필드에서 직접 읽기.

```rust
pub fn scan_doc_store(&self) -> Result<HashMap<u64, DocMeta>, SearchError> {
    let searcher = self.reader.searcher();
    let top_docs = searcher.search(&AllQuery, &TopDocs::with_limit(1_000_000))?;
    let mut store = HashMap::new();
    for (_score, addr) in top_docs {
        let doc: TantivyDocument = searcher.doc(addr)?;
        let id         = doc.get_first(self.fields.id)?.as_u64()?;
        let kind_str   = doc.get_first(self.fields.kind)?.as_str()?;
        let title      = doc.get_first(self.fields.title)?.as_str()?;
        let commit_oid = doc.get_first(self.fields.commit_oid)?.as_str()?;
        let path       = doc.get_first(self.fields.path).and_then(|v| v.as_str()).map(str::to_owned);
        let line_start = doc.get_first(self.fields.line_start).and_then(|v| v.as_u64()).map(|v| v as u32);
        let line_end   = doc.get_first(self.fields.line_end).and_then(|v| v.as_u64()).map(|v| v as u32);
        ...
    }
    Ok(store)
}
```

### 테스트

```rust
#[test]
fn doc_id_retrieved_as_u64_without_string_parse() {
    // add_doc 후 search 결과의 id가 u64 그대로 반환됨을 확인
}

#[test]
fn path_field_exact_match_query() {
    // path = "src/search/error.rs" 로 인덱싱 후
    // parser.parse_query("path:\"src/search/error.rs\"") 로 검색
    // 해당 doc만 반환됨을 확인
}

#[test]
fn scan_doc_store_reads_structured_fields() {
    // meta_json 없이 구조화 필드에서 DocMeta 재구성 확인
}
```

---

## 변경 순서

1. **Item 5** (reader 캐싱) — 스키마 변경 없음, 최소 침습적. 먼저 적용.
2. **Item 4** (tokenizer 호환성 검사) — `SearchError` variant 추가 + `open` 로직 수정.
3. **Item 6** (스키마 재설계) — 마지막. `INDEX_VERSION = 4` bump, `add_doc` 시그니처 변경,
   `indexer.rs` 호출부 대대적 수정. 이전 단계가 안정된 후 별도 커밋으로 진행 권장.

---

## Open Questions

1. **`scan_doc_store` 성능**: 현재 AllQuery + limit(1_000_000). 대규모 레포에서 병목 가능.
   Item 6 적용 시 같이 검토 필요.
2. **`add_doc` 시그니처 변경 범위**: indexer.rs만 호출하므로 영향 범위 좁음. 하지만
   미래에 CLI나 테스트에서 직접 호출할 경우를 고려해 타입을 명시적으로.
3. **doc_id 생성 책임**: 현재 `indexer.rs`의 `doc_counter`. Item 6 이후에도 동일. 
   BM25와 vector index가 같은 `doc_id` 공간을 공유하는 불변식은 유지.
