# Turbovec Semantic Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** gluck에 하이브리드 시맨틱 검색(BM25 + turbovec 4-bit vector + RRF)을 추가한다. `S` 키로 통합 모달을 열고, `glc index` 서브커맨드로 인덱스를 빌드한다.

**Architecture:** SearchEngine이 BM25(tantivy)와 VectorIndex(turbovec IdMapIndex)를 오케스트레이션. 두 백엔드 모두 `Vec<(u64, f32)>` 포맷으로 결과를 반환하여 RRF fusion이 단순한 list merge로 축소됨. fastembed-rs로 임베딩 생성(첫 실행 시 모델 자동 다운로드, 이후 offline). tree-sitter로 Rust 함수 단위 청킹, 미지원 언어는 fixed-size fallback. 한국어 커밋 메시지를 위해 character bigram tokenizer를 Tantivy에 등록.

**Tech Stack:** Rust, turbovec 0.1.3, tantivy 0.22, fastembed 4, tree-sitter-rust 0.23 (기존), ratatui (기존)

**Review decisions incorporated:**
- ~~CodeBERT/ort~~ → fastembed (native dep 없음, 첫 실행 시 모델 자동 다운로드)
- 한국어 BM25: character bigram tokenizer (pg_bigm 스타일, pure Rust)
- 청킹: tree-sitter Rust 함수 단위 + fixed-size fallback
- 통합 모달 (Files + Commits 동시 표시, Tab으로 섹션 전환)
- `u64` doc_id 공유 (Tantivy TEXT stored + turbovec IdMapIndex)
- `meta.toml version = 2`, `vector_backend = "turboquant_4bit"`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | fastembed, turbovec, tantivy 추가 |
| `src/lib.rs` | Modify | `pub mod search;` 추가 |
| `src/search/mod.rs` | Create | 코어 타입, SearchEngine facade, Meta, SearchError |
| `src/search/chunk.rs` | Create | Chunk enum, tree-sitter 함수 청킹, fixed-size fallback |
| `src/search/embedding.rs` | Create | EmbeddingModel (fastembed wrapper + test용 stub) |
| `src/search/vector.rs` | Create | VectorIndex (turbovec IdMapIndex wrapper), l2_normalize |
| `src/search/bm25.rs` | Create | Bm25Index + BigramTokenizer, `Vec<(u64, f32)>` 반환 |
| `src/search/rrf.rs` | Create | rrf_fuse: `&[(u64, f32)]` × 2 → `Vec<(u64, f32)>` |
| `src/search/indexer.rs` | Create | 인덱서 파이프라인 (repo walk → chunks → tantivy + turbovec) |
| `src/search/modal.rs` | Create | SemanticSearchModal 상태머신 + ModalAction |
| `src/ui/search_modal.rs` | Create | ratatui overlay 렌더러 |
| `src/ui/mod.rs` | Modify | `pub mod search_modal;` 추가 |
| `src/mode.rs` | Modify | `Action::SemanticSearch` 추가 |
| `src/cli.rs` | Modify | `Commands::Index { force, ... }` 추가 |
| `src/main.rs` | Modify | 서브커맨드 라우팅, `run_tui` 분리 |
| `src/app.rs` | Modify | search_engine + search_modal 필드, S 키 핸들러 |
| `src/config.rs` | Modify | SearchConfig 추가 |

---

### Task 1: 의존성 추가 + 모듈 스켈레톤

**Files:**
- Modify: `Cargo.toml`
- Create: `src/search/mod.rs`, `src/search/chunk.rs`, `src/search/embedding.rs`, `src/search/vector.rs`, `src/search/bm25.rs`, `src/search/rrf.rs`, `src/search/indexer.rs`, `src/search/modal.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Cargo.toml에 의존성 추가**

`dirs = "6"` 줄 다음에 추가:

```toml
turbovec = "0.1.3"
tantivy = "0.22"
fastembed = "4"
```

- [ ] **Step 2: src/search/mod.rs 생성 — 코어 타입**

```rust
pub mod bm25;
pub mod chunk;
pub mod embedding;
pub mod indexer;
pub mod modal;
pub mod rrf;
pub mod vector;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DocKind {
    Commit,
    File,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub doc_id: u64,
    pub kind: DocKind,
    pub title: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct DocMeta {
    pub kind: DocKind,
    pub title: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Meta {
    pub version: u32,
    pub head_oid: String,
    pub doc_count: u64,
    pub indexed_at: String,
    pub model_name: String,
    pub vector_dim: usize,
    pub vector_backend: String,
}

impl Meta {
    pub const CURRENT_VERSION: u32 = 2;

    pub fn verify_version(&self) -> Result<(), SearchError> {
        if self.version != Self::CURRENT_VERSION {
            Err(SearchError::IncompatibleIndex { version: self.version })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("No search index found. Run `glc index` to build one.")]
    NoIndex,
    #[error("Index format version {version} is incompatible. Run `glc index --force` to rebuild.")]
    IncompatibleIndex { version: u32 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Tantivy error: {0}")]
    Tantivy(String),
    #[error("Vector error: {0}")]
    Vector(String),
    #[error("Embedding error: {0}")]
    Embedding(String),
    #[error("Meta parse error: {0}")]
    MetaParse(String),
}

pub struct SearchEngine {
    index_root: PathBuf,
    bm25: Option<bm25::Bm25Index>,
    vectors: Option<vector::VectorIndex>,
    embedding: Option<embedding::EmbeddingModel>,
    doc_store: HashMap<u64, DocMeta>,
    pub config: SearchConfig,
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub bm25_top_k: usize,
    pub vector_top_k: usize,
    pub rrf_k: f32,
    pub result_limit: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            bm25_top_k: 50,
            vector_top_k: 50,
            rrf_k: 60.0,
            result_limit: 20,
        }
    }
}

impl SearchEngine {
    pub fn new(index_root: PathBuf) -> Self {
        Self {
            index_root,
            bm25: None,
            vectors: None,
            embedding: None,
            doc_store: HashMap::new(),
            config: SearchConfig::default(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.index_root.join("meta.toml").exists()
    }

    pub fn is_stale(&self, current_head: &str) -> bool {
        let meta_path = self.index_root.join("meta.toml");
        match std::fs::read_to_string(&meta_path) {
            Ok(content) => !content.contains(current_head),
            Err(_) => true,
        }
    }

    pub fn open(&mut self) -> Result<(), SearchError> {
        let meta = self.read_meta()?;
        meta.verify_version()?;

        self.bm25 = Some(bm25::Bm25Index::open(&self.index_root.join("bm25"))
            .map_err(|e| SearchError::Tantivy(e.to_string()))?);

        self.vectors = Some(vector::VectorIndex::load(&self.index_root.join("vectors/index.tvim"))
            .map_err(|e| SearchError::Vector(e.to_string()))?);

        self.embedding = Some(embedding::EmbeddingModel::new()
            .map_err(|e| SearchError::Embedding(e.to_string()))?);

        self.doc_store = self.load_doc_store()?;
        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let bm25 = self.bm25.as_ref().ok_or(SearchError::NoIndex)?;
        let vectors = self.vectors.as_ref().ok_or(SearchError::NoIndex)?;
        let embedding = self.embedding.as_ref().ok_or(SearchError::NoIndex)?;

        let bm25_hits = bm25.search(query, self.config.bm25_top_k)?;
        let query_emb = embedding.embed(query)
            .map_err(|e| SearchError::Embedding(e.to_string()))?;
        let vec_hits = vectors.search(&query_emb, self.config.vector_top_k);

        let fused = rrf::rrf_fuse(&bm25_hits, &vec_hits, self.config.rrf_k, self.config.result_limit);
        Ok(self.hydrate(fused))
    }

    fn hydrate(&self, hits: Vec<(u64, f32)>) -> Vec<SearchResult> {
        hits.into_iter()
            .filter_map(|(doc_id, score)| {
                let meta = self.doc_store.get(&doc_id)?;
                Some(SearchResult {
                    doc_id,
                    kind: meta.kind.clone(),
                    title: meta.title.clone(),
                    path: meta.path.clone(),
                    commit_oid: meta.commit_oid.clone(),
                    score,
                })
            })
            .collect()
    }

    pub fn read_meta(&self) -> Result<Meta, SearchError> {
        let path = self.index_root.join("meta.toml");
        if !path.exists() {
            return Err(SearchError::NoIndex);
        }
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).map_err(|e| SearchError::MetaParse(e.to_string()))
    }

    fn load_doc_store(&self) -> Result<HashMap<u64, DocMeta>, SearchError> {
        let bm25 = self.bm25.as_ref().ok_or(SearchError::NoIndex)?;
        bm25.scan_doc_store().map_err(|e| SearchError::Tantivy(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_verify_version_current() {
        let meta = Meta {
            version: 2,
            head_oid: "abc".into(),
            doc_count: 0,
            indexed_at: "".into(),
            model_name: "test".into(),
            vector_dim: 384,
            vector_backend: "turboquant_4bit".into(),
        };
        assert!(meta.verify_version().is_ok());
    }

    #[test]
    fn test_meta_verify_version_old() {
        let meta = Meta {
            version: 1,
            head_oid: "abc".into(),
            doc_count: 0,
            indexed_at: "".into(),
            model_name: "test".into(),
            vector_dim: 768,
            vector_backend: "brute_force".into(),
        };
        assert!(matches!(meta.verify_version(), Err(SearchError::IncompatibleIndex { version: 1 })));
    }

    #[test]
    fn test_search_engine_not_available_on_missing_index() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let engine = SearchEngine::new(dir.path().join("nonexistent"));
        assert!(!engine.is_available());
    }

    #[test]
    fn test_search_engine_stale_on_different_head() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let index_root = dir.path().join(".glc-index");
        std::fs::create_dir_all(&index_root).unwrap();
        std::fs::write(index_root.join("meta.toml"), "head_oid = \"abc123\"\nversion = 2\ndoc_count = 0\nindexed_at = \"\"\nmodel_name = \"\"\nvector_dim = 384\nvector_backend = \"turboquant_4bit\"\n").unwrap();

        let engine = SearchEngine::new(index_root);
        assert!(!engine.is_stale("abc123"));
        assert!(engine.is_stale("def456"));
    }
}
```

- [ ] **Step 3: 플레이스홀더 서브모듈 생성 (컴파일용)**

`src/search/chunk.rs`:
```rust
pub struct Chunk {
    pub doc_id: u64,
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
    pub kind: super::DocKind,
}
```

`src/search/embedding.rs`:
```rust
pub struct EmbeddingModel;
impl EmbeddingModel {
    pub fn new() -> Result<Self, String> { Ok(Self) }
    pub fn embed(&self, _text: &str) -> Result<Vec<f32>, String> { Ok(vec![]) }
    pub fn dim(&self) -> usize { 384 }
}
```

`src/search/vector.rs`:
```rust
use std::path::Path;
pub struct VectorIndex;
impl VectorIndex {
    pub fn load(_path: &Path) -> Result<Self, String> { Ok(Self) }
    pub fn search(&self, _query: &[f32], _k: usize) -> Vec<(u64, f32)> { vec![] }
}
```

`src/search/bm25.rs`:
```rust
use std::path::Path;
use std::collections::HashMap;
use super::DocMeta;
pub struct Bm25Index;
impl Bm25Index {
    pub fn open(_path: &Path) -> tantivy::Result<Self> { Ok(Self) }
    pub fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<(u64, f32)>, super::SearchError> { Ok(vec![]) }
    pub fn scan_doc_store(&self) -> tantivy::Result<HashMap<u64, DocMeta>> { Ok(HashMap::new()) }
}
```

`src/search/rrf.rs`:
```rust
pub fn rrf_fuse(_bm25: &[(u64, f32)], _vec: &[(u64, f32)], _k: f32, _limit: usize) -> Vec<(u64, f32)> { vec![] }
```

`src/search/indexer.rs`:
```rust
use std::path::Path;
pub fn build_index(_repo: &Path, _output: &Path, _batch_size: usize, _max_file_size: usize, _force: bool) -> anyhow::Result<()> { Ok(()) }
```

`src/search/modal.rs`:
```rust
pub struct SemanticSearchModal { pub active: bool }
impl SemanticSearchModal {
    pub fn new() -> Self { Self { active: false } }
}
```

- [ ] **Step 4: lib.rs에 search 모듈 등록**

`src/lib.rs` 끝에 추가:
```rust
pub mod search;
```

- [ ] **Step 5: 컴파일 확인**

```bash
cargo check
```
Expected: 컴파일 성공 (unused import 경고는 무시)

- [ ] **Step 6: 커밋**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/search/
git commit -m "Add search module skeleton with turbovec, tantivy, fastembed"
```

---

### Task 2: VectorIndex — turbovec IdMapIndex 래퍼

**Files:**
- Modify: `src/search/vector.rs`

- [ ] **Step 1: 테스트 먼저 작성**

`src/search/vector.rs`를 아래로 교체:

```rust
use std::path::Path;
use turbovec::IdMapIndex;

#[derive(Debug)]
pub enum VectorError {
    DimensionMismatch { expected: usize, actual: usize },
    Turbovec(String),
}

impl std::fmt::Display for VectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DimensionMismatch { expected, actual } =>
                write!(f, "dimension mismatch: expected {expected}, got {actual}"),
            Self::Turbovec(e) => write!(f, "turbovec: {e}"),
        }
    }
}

pub struct VectorIndex {
    inner: IdMapIndex,
    dim: usize,
}

impl VectorIndex {
    pub const BIT_WIDTH: usize = 4;

    pub fn new(dim: usize) -> Self {
        Self {
            inner: IdMapIndex::new(dim, Self::BIT_WIDTH),
            dim,
        }
    }

    /// vectors: flat row-major [v0_0, v0_1, ..., v0_dim, v1_0, ...]
    /// ids: parallel array of u64 doc_ids
    pub fn add(&mut self, vectors: &[f32], ids: &[u64]) -> Result<(), VectorError> {
        if vectors.len() != ids.len() * self.dim {
            return Err(VectorError::DimensionMismatch {
                expected: ids.len() * self.dim,
                actual: vectors.len(),
            });
        }
        let normalized = l2_normalize_batch(vectors, self.dim);
        // turbovec IdMapIndex uses i64 IDs (FAISS convention); cast from u64
        let ids_i64: Vec<i64> = ids.iter().map(|&x| x as i64).collect();
        self.inner.add_with_ids(&normalized, &ids_i64);
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(u64, f32)> {
        let normalized = l2_normalize(query);
        let (scores, ids) = self.inner.search(&normalized, k);
        ids.into_iter()
            .zip(scores.into_iter())
            .map(|(id, score)| (id as u64, score))
            .collect()
    }

    pub fn write(&self, path: &Path) -> Result<(), VectorError> {
        let path_str = path.to_str().expect("non-utf8 path");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VectorError::Turbovec(e.to_string()))?;
        }
        self.inner.write(path_str)
            .map_err(|e| VectorError::Turbovec(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self, VectorError> {
        let path_str = path.to_str().expect("non-utf8 path");
        let inner = IdMapIndex::load(path_str)
            .map_err(|e| VectorError::Turbovec(e.to_string()))?;
        // If IdMapIndex doesn't expose dim(), store it in a sidecar file.
        // turbovec 0.1.x exposes dim() — if compilation fails, add a dim field to the file.
        let dim = inner.dim();
        Ok(Self { inner, dim })
    }

    pub fn dim(&self) -> usize {
        self.dim
    }
}

pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / norm).collect()
}

pub fn l2_normalize_batch(vectors: &[f32], dim: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(vectors.len());
    for chunk in vectors.chunks(dim) {
        out.extend(l2_normalize(chunk));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn unit_vec(dim: usize, hot: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; dim];
        v[hot] = 1.0;
        v
    }

    #[test]
    fn test_l2_normalize_unit_vector() {
        let v = vec![1.0f32, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert!((n[0] - 1.0).abs() < 1e-6);
        assert!(n[1].abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_non_unit() {
        let v = vec![3.0f32, 4.0];
        let n = l2_normalize(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let v = vec![0.0f32, 0.0, 0.0];
        let n = l2_normalize(&v);
        // Should not panic, norm clamped to 1e-12
        assert_eq!(n.len(), 3);
    }

    #[test]
    fn test_l2_normalize_batch() {
        let dim = 3;
        let vectors = vec![3.0f32, 4.0, 0.0, 0.0, 0.0, 5.0];
        let out = l2_normalize_batch(&vectors, dim);
        assert_eq!(out.len(), 6);
        for chunk in out.chunks(dim) {
            let norm: f32 = chunk.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-5, "not normalized: {norm}");
        }
    }

    #[test]
    fn test_add_and_self_search() {
        let dim = 8;
        let mut idx = VectorIndex::new(dim);
        let v0 = unit_vec(dim, 0);
        let v1 = unit_vec(dim, 1);
        let v2 = unit_vec(dim, 2);

        let vectors: Vec<f32> = [v0.clone(), v1.clone(), v2.clone()].concat();
        idx.add(&vectors, &[100u64, 200u64, 300u64]).unwrap();

        let results = idx.search(&v0, 1);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 100u64);
    }

    #[test]
    fn test_dimension_mismatch_error() {
        let mut idx = VectorIndex::new(8);
        let err = idx.add(&[1.0, 2.0], &[1u64, 2u64]); // 2 vals for 2 ids × dim=8 is wrong
        assert!(matches!(err, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_write_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vectors/index.tvim");

        let dim = 8;
        let mut idx = VectorIndex::new(dim);
        let v0 = unit_vec(dim, 0);
        let v1 = unit_vec(dim, 7);
        let vectors: Vec<f32> = [v0.clone(), v1.clone()].concat();
        idx.add(&vectors, &[42u64, 99u64]).unwrap();
        idx.write(&path).unwrap();

        let loaded = VectorIndex::load(&path).unwrap();
        assert_eq!(loaded.dim(), dim);

        let results = loaded.search(&v0, 1);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 42u64);
    }
}
```

- [ ] **Step 2: 테스트 실행 — 실패 확인**

```bash
cargo test search::vector
```
Expected: FAIL (VectorIndex가 플레이스홀더)

- [ ] **Step 3: 테스트 실행 — 통과 확인**

```bash
cargo test search::vector 2>&1
```
Expected: 모든 테스트 PASS

> **주의:** turbovec IdMapIndex가 `i64`가 아닌 `u64`를 사용한다면 `ids_i64` 변환 및 결과의 `id as u64` 캐스팅을 제거. `inner.dim()` 메서드가 없으면 `dim` 필드를 tvim 파일 옆 `.dim` 사이드카 파일에 u64로 저장.

- [ ] **Step 4: 커밋**

```bash
git add src/search/vector.rs
git commit -m "Implement VectorIndex with turbovec IdMapIndex 4-bit quantization"
```

---

### Task 3: Bm25Index — Korean bigram tokenizer + u64 doc_ids

**Files:**
- Modify: `src/search/bm25.rs`

- [ ] **Step 1: 테스트 먼저 작성**

`src/search/bm25.rs`를 아래로 교체:

```rust
use std::collections::HashMap;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::tokenizer::{BoxTokenStream, Token, TokenStream, Tokenizer};
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
                for (i, window) in chars.windows(2).enumerate() {
                    let bigram: String = window.iter().collect();
                    tokens.push(Token {
                        offset_from: word_start,
                        offset_to: word_start + word.len(),
                        position: tokens.len(),
                        text: bigram,
                        position_length: 1,
                    });
                    let _ = i;
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
                let Ok(doc) = store_reader.get(doc_id) else { continue };
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
        // All IDs should be valid u64 values from our sample
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
```

- [ ] **Step 2: chunk.rs의 Chunk 구조체 완성 (bm25 테스트용)**

`src/search/chunk.rs`:
```rust
use super::DocKind;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub doc_id: u64,
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
    pub kind: DocKind,
}
```

- [ ] **Step 3: 테스트 실행**

```bash
cargo test search::bm25
```
Expected: 모든 테스트 PASS

- [ ] **Step 4: 커밋**

```bash
git add src/search/bm25.rs src/search/chunk.rs
git commit -m "Implement Bm25Index with bigram tokenizer for Korean support"
```

---

### Task 4: RRF fusion

**Files:**
- Modify: `src/search/rrf.rs`

- [ ] **Step 1: 테스트 먼저 작성**

`src/search/rrf.rs`:

```rust
use std::collections::HashMap;

/// Reciprocal Rank Fusion over two ranked lists of (doc_id, score).
/// k: smoothing constant (typically 60.0)
/// limit: maximum results to return
pub fn rrf_fuse(
    bm25: &[(u64, f32)],
    vec: &[(u64, f32)],
    k: f32,
    limit: usize,
) -> Vec<(u64, f32)> {
    let mut scores: HashMap<u64, f32> = HashMap::new();

    for (rank, (id, _)) in bm25.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vec.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }

    let mut out: Vec<(u64, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_both_empty() {
        assert!(rrf_fuse(&[], &[], 60.0, 10).is_empty());
    }

    #[test]
    fn test_single_source_bm25() {
        let bm25 = vec![(1u64, 1.0), (2u64, 0.5)];
        let out = rrf_fuse(&bm25, &[], 60.0, 10);
        assert_eq!(out.len(), 2);
        assert!(out[0].1 > out[1].1, "should be sorted descending");
        assert_eq!(out[0].0, 1u64);
    }

    #[test]
    fn test_overlap_boosts_score() {
        let bm25 = vec![(1u64, 1.0), (2u64, 0.5)];
        let vec  = vec![(1u64, 0.9)];
        let out = rrf_fuse(&bm25, &vec, 60.0, 10);
        let id1 = out.iter().find(|(id, _)| *id == 1).unwrap();
        let id2 = out.iter().find(|(id, _)| *id == 2).unwrap();
        // id=1 appears in both lists → higher RRF score than id=2 (single list only)
        assert!(id1.1 > id2.1, "overlap should boost score");
    }

    #[test]
    fn test_sorted_descending() {
        let bm25 = vec![(1u64, 1.0), (2u64, 0.8), (3u64, 0.3)];
        let vec  = vec![(1u64, 0.9), (3u64, 0.7)];
        let out = rrf_fuse(&bm25, &vec, 60.0, 10);
        for w in out.windows(2) {
            assert!(w[0].1 >= w[1].1);
        }
    }

    #[test]
    fn test_limit_respected() {
        let bm25: Vec<(u64, f32)> = (0..20).map(|i| (i, 1.0 / (i as f32 + 1.0))).collect();
        let out = rrf_fuse(&bm25, &[], 60.0, 5);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn test_vec_only() {
        let vec = vec![(10u64, 0.9), (20u64, 0.7)];
        let out = rrf_fuse(&[], &vec, 60.0, 10);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, 10u64);
    }
}
```

- [ ] **Step 2: 테스트 실행**

```bash
cargo test search::rrf
```
Expected: 모든 테스트 PASS

- [ ] **Step 3: 커밋**

```bash
git add src/search/rrf.rs
git commit -m "Implement RRF fusion with u64 doc_ids"
```

---

### Task 5: Chunk 타입 + tree-sitter 기반 청킹

**Files:**
- Modify: `src/search/chunk.rs`

- [ ] **Step 1: 테스트 먼저 작성**

`src/search/chunk.rs` 전체 교체:

```rust
use super::DocKind;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub doc_id: u64,
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
    pub kind: DocKind,
}

/// Split `content` into indexable chunks.
/// For Rust files: tree-sitter function/impl level.
/// For everything else: fixed-size character windows with overlap.
pub fn split_file(
    path: &str,
    content: &str,
    commit_oid: &str,
    base_doc_id: u64,
    max_chunk_chars: usize,
) -> Vec<Chunk> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let raw = match ext {
        "rs" => split_rust(path, content, commit_oid, base_doc_id, max_chunk_chars),
        _ => split_fixed_size(path, content, commit_oid, base_doc_id, max_chunk_chars),
    };

    if raw.is_empty() {
        // Fallback: whole file as single chunk
        vec![Chunk {
            doc_id: base_doc_id,
            kind: DocKind::File,
            title: path.to_string(),
            body: content.chars().take(max_chunk_chars).collect(),
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        }]
    } else {
        raw
    }
}

/// Split a commit message into a single Chunk.
pub fn commit_chunk(
    commit_oid: &str,
    short_id: &str,
    message: &str,
    doc_id: u64,
) -> Chunk {
    let title = message.lines().next().unwrap_or("").to_string();
    Chunk {
        doc_id,
        kind: DocKind::Commit,
        title,
        body: message.to_string(),
        path: None,
        commit_oid: Some(commit_oid.to_string()),
    }
}

// ── Rust: tree-sitter function/impl extraction ────────────────────────────────

fn split_rust(
    path: &str,
    content: &str,
    commit_oid: &str,
    base_doc_id: u64,
    max_chunk_chars: usize,
) -> Vec<Chunk> {
    use tree_sitter::Parser;

    let raw_fn = tree_sitter_rust::LANGUAGE.into_raw();
    let raw_ptr = unsafe { raw_fn() };
    let language = unsafe { tree_sitter::Language::from_raw(raw_ptr as *const _) };

    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return vec![];
    }

    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return vec![],
    };

    let root = tree.root_node();
    let mut chunks = Vec::new();
    let mut counter = 0u64;
    extract_rust_functions(&root, content, path, commit_oid, base_doc_id, &mut counter, max_chunk_chars, &mut chunks);
    chunks
}

fn extract_rust_functions(
    node: &tree_sitter::Node,
    source: &str,
    path: &str,
    commit_oid: &str,
    base_doc_id: u64,
    counter: &mut u64,
    max_chunk_chars: usize,
    chunks: &mut Vec<Chunk>,
) {
    let kind = node.kind();

    if kind == "function_item" || kind == "impl_item" {
        let start = node.start_byte();
        let end = node.end_byte().min(source.len());
        let body: String = source[start..end].chars().take(max_chunk_chars).collect();

        let title = if kind == "function_item" {
            // extract `fn name` from node
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|name| format!("{path}::{name}"))
                .unwrap_or_else(|| path.to_string())
        } else {
            // impl block: use path as title
            path.to_string()
        };

        chunks.push(Chunk {
            doc_id: base_doc_id + *counter,
            kind: DocKind::File,
            title,
            body,
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        });
        *counter += 1;
        return; // don't recurse into functions
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        extract_rust_functions(&child, source, path, commit_oid, base_doc_id, counter, max_chunk_chars, chunks);
    }
}

// ── Fixed-size: 512-char windows with 64-char overlap ─────────────────────────

fn split_fixed_size(
    path: &str,
    content: &str,
    commit_oid: &str,
    base_doc_id: u64,
    max_chunk_chars: usize,
) -> Vec<Chunk> {
    const OVERLAP: usize = 64;

    if content.len() <= max_chunk_chars {
        return vec![Chunk {
            doc_id: base_doc_id,
            kind: DocKind::File,
            title: path.to_string(),
            body: content.to_string(),
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        }];
    }

    let chars: Vec<char> = content.chars().collect();
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut counter = 0u64;

    while start < chars.len() {
        let end = (start + max_chunk_chars).min(chars.len());
        let body: String = chars[start..end].iter().collect();
        chunks.push(Chunk {
            doc_id: base_doc_id + counter,
            kind: DocKind::File,
            title: format!("{path}:{counter}"),
            body,
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        });
        counter += 1;
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(OVERLAP);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_chunk() {
        let c = commit_chunk("abc1234def", "abc1234", "Fix the bug\n\nLonger description.", 42);
        assert_eq!(c.doc_id, 42);
        assert_eq!(c.kind, DocKind::Commit);
        assert_eq!(c.title, "Fix the bug");
        assert!(c.body.contains("Longer description"));
    }

    #[test]
    fn test_split_rust_extracts_functions() {
        let src = r#"
fn hello() {
    println!("hello");
}

fn world() -> i32 {
    42
}
"#;
        let chunks = split_file("src/lib.rs", src, "abc", 0, 4096);
        assert!(!chunks.is_empty(), "should extract rust functions");
        assert!(chunks.iter().any(|c| c.title.contains("hello") || c.title.contains("world")));
    }

    #[test]
    fn test_split_rust_impl_block() {
        let src = r#"
impl Foo {
    pub fn bar(&self) -> i32 { 1 }
    fn baz(&self) {}
}
"#;
        let chunks = split_file("src/foo.rs", src, "abc", 0, 4096);
        // Should produce at least one chunk for the impl block
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_split_fixed_size_short_file() {
        let content = "hello world";
        let chunks = split_file("readme.txt", content, "abc", 0, 512);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].body, content);
    }

    #[test]
    fn test_split_fixed_size_long_file() {
        let content: String = "x".repeat(2000);
        let chunks = split_file("data.txt", &content, "abc", 0, 512);
        assert!(chunks.len() > 1, "long file should produce multiple chunks");
    }

    #[test]
    fn test_split_fixed_size_overlap() {
        let content: String = "abcdefgh".repeat(100); // 800 chars
        let chunks = split_file("data.txt", &content, "abc", 0, 512);
        assert!(chunks.len() >= 2);
        // Adjacent chunks should overlap by ~64 chars
        let end_of_first: String = chunks[0].body.chars().rev().take(64).collect::<String>().chars().rev().collect();
        let start_of_second: String = chunks[1].body.chars().take(64).collect();
        assert!(end_of_first == start_of_second || !end_of_first.is_empty());
    }

    #[test]
    fn test_doc_ids_are_unique_within_file() {
        let src = r#"fn a() {} fn b() {} fn c() {}"#;
        let chunks = split_file("src/lib.rs", src, "abc", 1000, 4096);
        let ids: std::collections::HashSet<u64> = chunks.iter().map(|c| c.doc_id).collect();
        assert_eq!(ids.len(), chunks.len(), "all doc_ids should be unique");
    }
}
```

- [ ] **Step 2: 테스트 실행**

```bash
cargo test search::chunk
```
Expected: 모든 테스트 PASS

- [ ] **Step 3: 커밋**

```bash
git add src/search/chunk.rs
git commit -m "Implement chunker: tree-sitter Rust function extraction + fixed-size fallback"
```

---

### Task 6: EmbeddingModel — fastembed 래퍼 + 테스트 stub

**Files:**
- Modify: `src/search/embedding.rs`

- [ ] **Step 1: 테스트 먼저 작성**

`src/search/embedding.rs` 전체 교체:

```rust
use fastembed::{EmbeddingModel as FastembedModel, InitOptions, TextEmbedding};

// 모델 설정 상수 (교체 시 여기만 변경)
// 추천: JinaEmbeddingsV3 (1024-dim, 한국어+코드 지원)
// 현재: AllMiniLML6V2 (384-dim, 영어 위주, 빠른 다운로드 < 50MB)
const MODEL: FastembedModel = FastembedModel::AllMiniLML6V2;
pub const MODEL_NAME: &str = "all-MiniLM-L6-v2";
pub const MODEL_DIM: usize = 384;

// JinaEmbeddingsV3 사용 시:
// const MODEL: FastembedModel = FastembedModel::JinaEmbeddingsV3;
// pub const MODEL_NAME: &str = "jina-embeddings-v3";
// pub const MODEL_DIM: usize = 1024;

pub struct EmbeddingModel {
    inner: EmbeddingBackend,
}

enum EmbeddingBackend {
    Fastembed(TextEmbedding),
    Stub(usize), // dim — 테스트용
}

impl EmbeddingModel {
    /// Production: fastembed 모델 (첫 실행 시 자동 다운로드 → ~/.cache/huggingface/)
    pub fn new() -> Result<Self, String> {
        let model = TextEmbedding::try_new(
            InitOptions::new(MODEL).with_show_download_progress(true),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { inner: EmbeddingBackend::Fastembed(model) })
    }

    /// Test stub: 결정론적 해시 기반 벡터 (모델 다운로드 없음, 고품질 아님)
    pub fn new_stub(dim: usize) -> Self {
        Self { inner: EmbeddingBackend::Stub(dim) }
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        match &self.inner {
            EmbeddingBackend::Fastembed(model) => {
                let mut results = model
                    .embed(vec![text.to_string()], None)
                    .map_err(|e| e.to_string())?;
                Ok(results.remove(0))
            }
            EmbeddingBackend::Stub(dim) => Ok(stub_embed(text, *dim)),
        }
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        match &self.inner {
            EmbeddingBackend::Fastembed(model) => {
                let owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
                model.embed(owned, None).map_err(|e| e.to_string())
            }
            EmbeddingBackend::Stub(dim) => {
                Ok(texts.iter().map(|t| stub_embed(t, *dim)).collect())
            }
        }
    }

    pub fn dim(&self) -> usize {
        match &self.inner {
            EmbeddingBackend::Fastembed(_) => MODEL_DIM,
            EmbeddingBackend::Stub(dim) => *dim,
        }
    }
}

/// 결정론적 해시 기반 임베딩 (테스트/오프라인 fallback)
/// 동일 입력 → 동일 출력 (재현 가능)
/// 품질은 random에 가깝지만 파이프라인 검증에 충분
fn stub_embed(text: &str, dim: usize) -> Vec<f32> {
    let seed = text
        .bytes()
        .fold(0x517cc1b727220a95u64, |acc, b| {
            acc.wrapping_mul(0x517cc1b727220a95).wrapping_add(b as u64)
        });

    let mut v: Vec<f32> = (0..dim)
        .map(|i| {
            let x = seed
                .wrapping_add(i as u64 * 0x9e3779b97f4a7c15)
                .wrapping_mul(0x6c62272e07bb0142);
            let x = x ^ (x >> 32);
            (x as i64 as f32) / (i64::MAX as f32)
        })
        .collect();

    // L2-normalize (turbovec은 hypersphere 위 벡터를 가정)
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in &mut v {
        *x /= norm;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_embed_dimension() {
        let model = EmbeddingModel::new_stub(128);
        let emb = model.embed("hello world").unwrap();
        assert_eq!(emb.len(), 128);
    }

    #[test]
    fn test_stub_embed_normalized() {
        let model = EmbeddingModel::new_stub(64);
        let emb = model.embed("test text").unwrap();
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "should be L2-normalized, got {norm}");
    }

    #[test]
    fn test_stub_embed_deterministic() {
        let model = EmbeddingModel::new_stub(32);
        let a = model.embed("same text").unwrap();
        let b = model.embed("same text").unwrap();
        assert_eq!(a, b, "stub should be deterministic");
    }

    #[test]
    fn test_stub_embed_different_texts_differ() {
        let model = EmbeddingModel::new_stub(32);
        let a = model.embed("hello").unwrap();
        let b = model.embed("world").unwrap();
        assert_ne!(a, b, "different texts should produce different embeddings");
    }

    #[test]
    fn test_stub_embed_batch() {
        let model = EmbeddingModel::new_stub(16);
        let batch = model.embed_batch(&["foo", "bar", "baz"]).unwrap();
        assert_eq!(batch.len(), 3);
        for emb in &batch {
            assert_eq!(emb.len(), 16);
        }
    }
}
```

- [ ] **Step 2: 테스트 실행**

```bash
cargo test search::embedding
```
Expected: 모든 테스트 PASS (stub 사용, 네트워크 없음)

- [ ] **Step 3: 커밋**

```bash
git add src/search/embedding.rs
git commit -m "Implement EmbeddingModel with fastembed backend and deterministic stub"
```

---

### Task 7: 인덱서 파이프라인

**Files:**
- Modify: `src/search/indexer.rs`

- [ ] **Step 1: 테스트 먼저 작성**

`src/search/indexer.rs` 전체 교체:

```rust
use std::path::Path;
use anyhow::{Context, Result};
use crate::git::commit::list_commits;
use crate::git::repo::GitRepo;
use crate::git::tree::{is_binary_blob, list_tree, read_blob, EntryKind};
use super::bm25::Bm25Index;
use super::chunk::{commit_chunk, split_file, Chunk};
use super::embedding::EmbeddingModel;
use super::vector::VectorIndex;
use super::Meta;

const BATCH_SIZE: usize = 32;
const MAX_FILE_CHARS: usize = 4096; // per chunk

pub struct IndexConfig {
    pub batch_size: usize,
    pub max_file_bytes: usize,
    pub force: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self { batch_size: BATCH_SIZE, max_file_bytes: 1_048_576, force: false }
    }
}

pub fn build_index(repo_path: &Path, output_path: &Path, cfg: IndexConfig) -> Result<()> {
    build_index_with_model(repo_path, output_path, cfg, None)
}

/// `embedding_model` = None → uses EmbeddingModel::new() (fastembed)
/// `embedding_model` = Some(stub) → for tests (no download)
pub fn build_index_with_model(
    repo_path: &Path,
    output_path: &Path,
    cfg: IndexConfig,
    embedding_model: Option<EmbeddingModel>,
) -> Result<()> {
    let repo = GitRepo::open(repo_path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let head_oid = {
        let r = repo.repository();
        r.head().context("get HEAD")?
            .target().context("HEAD has no target")?
            .to_string()
    };

    // Skip if up-to-date (unless --force)
    let meta_path = output_path.join("meta.toml");
    if !cfg.force && meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if content.contains(&head_oid) {
                eprintln!("Index up-to-date (HEAD {}). Use --force to rebuild.", &head_oid[..7]);
                return Ok(());
            }
        }
    }

    // Old index with incompatible version: require --force
    if !cfg.force && meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            let meta: Result<Meta, _> = toml::from_str(&content);
            if let Ok(m) = meta {
                if m.version != Meta::CURRENT_VERSION {
                    anyhow::bail!(
                        "Index format version {} is incompatible. Run `glc index --force` to rebuild.",
                        m.version
                    );
                }
            }
        }
    }

    eprintln!("Building search index for {} ...", repo_path.display());

    let commits = list_commits(&repo).map_err(|e| anyhow::anyhow!("{e}"))?;
    if commits.is_empty() {
        anyhow::bail!("No commits found");
    }
    let head_commit = &commits[0];

    // Collect all chunks
    let mut all_chunks: Vec<Chunk> = Vec::new();
    let mut next_doc_id: u64 = 0;

    // 1. Commit messages
    eprintln!("  Indexing {} commits...", commits.len());
    for commit in &commits {
        all_chunks.push(commit_chunk(
            &commit.id.to_string(),
            &commit.short_id,
            &commit.message,
            next_doc_id,
        ));
        next_doc_id += 1;
    }

    // 2. HEAD file tree
    let tree = list_tree(&repo, head_commit)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let file_entries: Vec<_> = tree.iter()
        .filter(|e| matches!(e.kind, EntryKind::File))
        .collect();
    eprintln!("  Indexing {} files...", file_entries.len());

    for entry in &file_entries {
        if is_binary_blob(&repo, head_commit, &entry.path).unwrap_or(true) {
            continue;
        }
        let content = match read_blob(&repo, head_commit, &entry.path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.len() > cfg.max_file_bytes {
            continue;
        }
        let file_chunks = split_file(
            &entry.path,
            &content,
            &head_commit.id.to_string(),
            next_doc_id,
            MAX_FILE_CHARS,
        );
        let chunk_count = file_chunks.len() as u64;
        all_chunks.extend(file_chunks);
        next_doc_id += chunk_count;
    }

    eprintln!("  Total chunks: {}", all_chunks.len());

    // Clean up old index
    if output_path.exists() {
        std::fs::remove_dir_all(output_path)
            .context("remove old index")?;
    }
    std::fs::create_dir_all(output_path)
        .context("create index dir")?;

    // 3. Build BM25 index
    let bm25_path = output_path.join("bm25");
    Bm25Index::build(&bm25_path, &all_chunks)
        .map_err(|e| anyhow::anyhow!("BM25 build failed: {e}"))?;

    // 4. Generate embeddings + build turbovec index
    let model = match embedding_model {
        Some(m) => m,
        None => EmbeddingModel::new().map_err(|e| anyhow::anyhow!("Embedding model init: {e}"))?,
    };
    let dim = model.dim();
    let mut vector_index = VectorIndex::new(dim);

    eprintln!("  Generating embeddings (dim={dim}, batch={})...", cfg.batch_size);
    for batch in all_chunks.chunks(cfg.batch_size) {
        let texts: Vec<&str> = batch.iter()
            .map(|c| c.body.as_str())
            .collect();
        let embeddings = model.embed_batch(&texts)
            .map_err(|e| anyhow::anyhow!("Embedding failed: {e}"))?;
        // embed_batch returns Vec<Vec<f32>>; flatten to row-major for VectorIndex::add
        let flat: Vec<f32> = embeddings.into_iter().flatten().collect();
        let ids: Vec<u64> = batch.iter().map(|c| c.doc_id).collect();
        vector_index.add(&flat, &ids)
            .map_err(|e| anyhow::anyhow!("VectorIndex add failed: {e}"))?;
    }

    let tvim_path = output_path.join("vectors/index.tvim");
    vector_index.write(&tvim_path)
        .map_err(|e| anyhow::anyhow!("VectorIndex write failed: {e}"))?;

    // 5. Write meta.toml
    let meta = Meta {
        version: Meta::CURRENT_VERSION,
        head_oid,
        doc_count: next_doc_id,
        indexed_at: unix_timestamp_str(),
        model_name: super::embedding::MODEL_NAME.to_string(),
        vector_dim: dim,
        vector_backend: "turboquant_4bit".to_string(),
    };
    let meta_content = toml::to_string_pretty(&meta)
        .context("serialize meta.toml")?;
    std::fs::write(&meta_path, meta_content)
        .context("write meta.toml")?;

    eprintln!("  Done. {} chunks indexed.", next_doc_id);
    Ok(())
}

fn unix_timestamp_str() -> String {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use super::super::embedding::EmbeddingModel;
    use tempfile::TempDir;

    fn stub_model() -> EmbeddingModel {
        EmbeddingModel::new_stub(16) // small dim for fast tests
    }

    fn build_test_index(repo_dir: &Path) -> (TempDir, std::path::PathBuf) {
        let index_dir = TempDir::new().unwrap();
        let output = index_dir.path().join(".glc-index");
        build_index_with_model(repo_dir, &output, IndexConfig::default(), Some(stub_model())).unwrap();
        (index_dir, output)
    }

    #[test]
    fn test_build_creates_expected_files() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() { println!(\"hello\"); }", "Initial commit");

        let (_index_dir, output) = build_test_index(repo_dir.path());

        assert!(output.join("meta.toml").exists(), "meta.toml missing");
        assert!(output.join("bm25").exists(), "bm25/ missing");
        assert!(output.join("vectors/index.tvim").exists(), "vectors/index.tvim missing");
    }

    #[test]
    fn test_meta_version_is_2() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn f() {}", "Add f");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let content = std::fs::read_to_string(output.join("meta.toml")).unwrap();
        let meta: Meta = toml::from_str(&content).unwrap();
        assert_eq!(meta.version, 2);
        assert_eq!(meta.vector_backend, "turboquant_4bit");
    }

    #[test]
    fn test_build_skips_if_up_to_date() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn f() {}", "Add f");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let mtime1 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();

        // Second build should skip
        build_index_with_model(repo_dir.path(), &output, IndexConfig::default(), Some(stub_model())).unwrap();
        let mtime2 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();
        assert_eq!(mtime1, mtime2, "second build should not modify meta.toml");
    }

    #[test]
    fn test_build_force_rebuilds() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn f() {}", "Add f");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let mtime1 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        let cfg = IndexConfig { force: true, ..Default::default() };
        build_index_with_model(repo_dir.path(), &output, cfg, Some(stub_model())).unwrap();
        let mtime2 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();
        assert_ne!(mtime1, mtime2, "--force should rebuild");
    }

    #[test]
    fn test_build_skips_binary_files() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() {}", "Add code");
        add_file_commit(&repo, "img.png", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "Add image");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let meta: Meta = toml::from_str(
            &std::fs::read_to_string(output.join("meta.toml")).unwrap()
        ).unwrap();
        // commits (2) + 1 text file (img.png skipped) = 3 chunks minimum
        assert!(meta.doc_count >= 3);
    }

    #[test]
    fn test_bm25_searchable_after_index() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "error.rs", b"fn handle_error() { panic!(\"oops\"); }", "Add error handling");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let bm25 = Bm25Index::open(&output.join("bm25")).unwrap();
        let results = bm25.search("error", 10).unwrap();
        assert!(!results.is_empty(), "should find 'error' after indexing");
    }

    #[test]
    fn test_doc_ids_unique_across_index() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn a() {}", "Add a");
        add_file_commit(&repo, "b.rs", b"fn b() {}", "Add b");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let bm25 = Bm25Index::open(&output.join("bm25")).unwrap();
        let store = bm25.scan_doc_store().unwrap();

        let ids: Vec<u64> = store.keys().copied().collect();
        let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "all doc_ids must be unique");
    }
}
```

- [ ] **Step 2: 테스트 실행**

```bash
cargo test search::indexer
```
Expected: 모든 테스트 PASS

- [ ] **Step 3: 커밋**

```bash
git add src/search/indexer.rs
git commit -m "Implement indexer pipeline: commits + tree-sitter chunks + turbovec + BM25"
```

---

### Task 8: CLI 서브커맨드 + main.rs 라우팅

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: cli.rs 확장**

`src/cli.rs` 전체 교체:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "glc", about = "Terminal git history file viewer")]
pub struct Cli {
    /// Git repository path (TUI 모드)
    pub path: Option<String>,

    /// Log level (trace|debug|info|warn|error)
    #[arg(long, default_value = "warn", global = true)]
    pub log_level: String,

    /// Enable debug overlay
    #[arg(long, global = true)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build search index for semantic search
    Index {
        /// Repository path (default: current directory)
        #[arg(default_value = ".")]
        repo_path: PathBuf,

        /// Batch size for embedding generation
        #[arg(long, default_value = "32")]
        batch_size: usize,

        /// Max file size to index in bytes
        #[arg(long, default_value = "1048576")]
        max_file_size: usize,

        /// Force rebuild even if index is current
        #[arg(long)]
        force: bool,
    },
}
```

- [ ] **Step 2: main.rs 업데이트**

`src/main.rs` 전체 교체:

```rust
use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind, KeyModifiers};
use gluck::app::App;
use gluck::cli::{Cli, Commands};
use gluck::config::Config;
use gluck::debug;
use gluck::git::repo::GitRepo;
use gluck::search::indexer::{build_index, IndexConfig};
use std::path::PathBuf;

fn main() -> Result<()> {
    let cli = Cli::parse();
    debug::init_logging(&cli.log_level);

    match cli.command {
        Some(Commands::Index { repo_path, batch_size, max_file_size, force }) => {
            let output_path = repo_path.join(".glc-index");
            let cfg = IndexConfig { batch_size, max_file_bytes: max_file_size, force };
            build_index(&repo_path, &output_path, cfg)
        }
        None => run_tui(cli),
    }
}

fn run_tui(cli: Cli) -> Result<()> {
    let path = cli
        .path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let repo = match GitRepo::open(&path) {
        Ok(r) => r,
        Err(_) => {
            eprintln!("fatal: not a git repository: {}", path.display());
            std::process::exit(1);
        }
    };
    let config = Config::load().unwrap_or_default();
    let mut app = App::new(repo, config)?;
    if cli.debug {
        app.debug_overlay = true;
    }

    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| app.render(f))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.handle_ctrl_key(key.code);
                } else {
                    app.handle_key(key.code);
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
```

- [ ] **Step 3: 컴파일 확인**

```bash
cargo check
```
Expected: 성공

- [ ] **Step 4: CLI 동작 확인**

```bash
cargo run -- --help
```
Expected: `index` 서브커맨드가 출력됨

```bash
cargo run -- index --help
```
Expected: `--force`, `--batch-size`, `--max-file-size` 옵션이 보임

- [ ] **Step 5: 커밋**

```bash
git add src/cli.rs src/main.rs
git commit -m "Add glc index subcommand with --force flag"
```

---

### Task 9: config.rs SearchConfig 추가

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: SearchConfig 추가**

`Config` 구조체 직후 `#[serde(default)] pub search: SearchConfig,` 필드를 추가:

```rust
// Config 구조체에 search 필드 추가
#[serde(default)]
pub search: SearchConfig,
```

파일 끝에 SearchConfig 추가:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub rrf_k: f32,
    pub bm25_top_k: usize,
    pub vector_top_k: usize,
    pub result_limit: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            rrf_k: 60.0,
            bm25_top_k: 50,
            vector_top_k: 50,
            result_limit: 20,
        }
    }
}
```

- [ ] **Step 2: 기존 테스트 통과 확인**

```bash
cargo test config
```
Expected: 모든 테스트 PASS (`#[serde(default)]`로 하위 호환)

- [ ] **Step 3: 커밋**

```bash
git add src/config.rs
git commit -m "Add SearchConfig section to config.toml"
```

---

### Task 10: SemanticSearchModal 상태머신

**Files:**
- Modify: `src/search/modal.rs`
- Modify: `src/mode.rs`

- [ ] **Step 1: 테스트 먼저 작성**

`src/search/modal.rs` 전체 교체:

```rust
use crossterm::event::KeyCode;
use super::SearchResult;

#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    Files,
    Commits,
}

#[derive(Debug, Clone)]
pub struct SemanticSearchModal {
    pub input: String,
    pub file_results: Vec<SearchResult>,
    pub commit_results: Vec<SearchResult>,
    pub selected: usize,
    pub focused_section: Section,
    pub active: bool,
    pub warning: Option<String>,   // stale index 경고
    pub no_index: bool,            // 인덱스 없음
    pub incompatible: bool,        // 버전 불일치
}

#[derive(Debug, Clone)]
pub enum ModalAction {
    None,
    Close,
    Navigate(SearchResult),
}

impl SemanticSearchModal {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            file_results: vec![],
            commit_results: vec![],
            selected: 0,
            focused_section: Section::Files,
            active: false,
            warning: None,
            no_index: false,
            incompatible: false,
        }
    }

    pub fn open(
        &mut self,
        is_available: bool,
        is_stale: bool,
        is_incompatible: bool,
    ) {
        self.active = true;
        self.input.clear();
        self.file_results.clear();
        self.commit_results.clear();
        self.selected = 0;
        self.focused_section = Section::Files;
        self.warning = None;
        self.no_index = false;
        self.incompatible = false;

        if !is_available {
            self.no_index = true;
        } else if is_incompatible {
            self.incompatible = true;
        } else if is_stale {
            self.warning = Some("Index may be stale — run `glc index` to refresh.".to_string());
        }
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn handle_key(
        &mut self,
        code: KeyCode,
        search_fn: impl FnOnce(&str) -> Vec<SearchResult>,
    ) -> ModalAction {
        match code {
            KeyCode::Esc => {
                self.close();
                ModalAction::Close
            }
            KeyCode::Enter => {
                let result = self.selected_result().cloned();
                self.close();
                match result {
                    Some(r) => ModalAction::Navigate(r),
                    None => ModalAction::Close,
                }
            }
            KeyCode::Tab => {
                self.toggle_section();
                ModalAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                ModalAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                ModalAction::None
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.update_results(search_fn);
                ModalAction::None
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.update_results(search_fn);
                ModalAction::None
            }
            _ => ModalAction::None,
        }
    }

    fn update_results(&mut self, search_fn: impl FnOnce(&str) -> Vec<SearchResult>) {
        if self.input.is_empty() || self.no_index || self.incompatible {
            self.file_results.clear();
            self.commit_results.clear();
            self.selected = 0;
            return;
        }
        let all = search_fn(&self.input);
        self.file_results = all.iter()
            .filter(|r| r.kind == super::DocKind::File)
            .cloned()
            .collect();
        self.commit_results = all.iter()
            .filter(|r| r.kind == super::DocKind::Commit)
            .cloned()
            .collect();
        self.selected = 0;
    }

    fn current_section_results(&self) -> &[SearchResult] {
        match self.focused_section {
            Section::Files => &self.file_results,
            Section::Commits => &self.commit_results,
        }
    }

    fn move_down(&mut self) {
        let max = self.current_section_results().len().saturating_sub(1);
        if self.current_section_results().is_empty() { return; }
        self.selected = (self.selected + 1).min(max);
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn toggle_section(&mut self) {
        self.focused_section = match self.focused_section {
            Section::Files => Section::Commits,
            Section::Commits => Section::Files,
        };
        self.selected = 0;
    }

    pub fn selected_result(&self) -> Option<&SearchResult> {
        self.current_section_results().get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::DocKind;

    fn make_result(id: u64, kind: DocKind, title: &str) -> SearchResult {
        SearchResult {
            doc_id: id,
            kind,
            title: title.to_string(),
            path: if kind == DocKind::File { Some(title.to_string()) } else { None },
            commit_oid: if kind == DocKind::Commit { Some("abc".to_string()) } else { None },
            score: 1.0,
        }
    }

    fn no_search(_: &str) -> Vec<SearchResult> { vec![] }

    fn some_results(_: &str) -> Vec<SearchResult> {
        vec![
            make_result(1, DocKind::File, "src/main.rs"),
            make_result(2, DocKind::File, "src/lib.rs"),
            make_result(3, DocKind::Commit, "Fix bug"),
        ]
    }

    #[test]
    fn test_modal_opens_with_no_index() {
        let mut m = SemanticSearchModal::new();
        m.open(false, false, false);
        assert!(m.active);
        assert!(m.no_index);
    }

    #[test]
    fn test_modal_opens_with_incompatible_index() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, true);
        assert!(m.active);
        assert!(m.incompatible);
    }

    #[test]
    fn test_modal_opens_with_stale_warning() {
        let mut m = SemanticSearchModal::new();
        m.open(true, true, false);
        assert!(m.active);
        assert!(m.warning.is_some());
    }

    #[test]
    fn test_esc_closes() {
        let mut m = SemanticSearchModal::new();
        m.active = true;
        let action = m.handle_key(KeyCode::Esc, no_search);
        assert!(!m.active);
        assert!(matches!(action, ModalAction::Close));
    }

    #[test]
    fn test_tab_toggles_section() {
        let mut m = SemanticSearchModal::new();
        m.active = true;
        assert_eq!(m.focused_section, Section::Files);
        m.handle_key(KeyCode::Tab, no_search);
        assert_eq!(m.focused_section, Section::Commits);
        m.handle_key(KeyCode::Tab, no_search);
        assert_eq!(m.focused_section, Section::Files);
    }

    #[test]
    fn test_typing_updates_input_and_calls_search() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        m.handle_key(KeyCode::Char('h'), some_results);
        m.handle_key(KeyCode::Char('i'), some_results);
        assert_eq!(m.input, "hi");
        // some_results always returns 2 files + 1 commit
        assert_eq!(m.file_results.len(), 2);
        assert_eq!(m.commit_results.len(), 1);
    }

    #[test]
    fn test_backspace_updates_input() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        m.handle_key(KeyCode::Char('a'), no_search);
        m.handle_key(KeyCode::Char('b'), no_search);
        m.handle_key(KeyCode::Backspace, no_search);
        assert_eq!(m.input, "a");
    }

    #[test]
    fn test_navigation_bounds() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        // No results: navigation should not panic
        m.handle_key(KeyCode::Down, no_search);
        assert_eq!(m.selected, 0);
        m.handle_key(KeyCode::Up, no_search);
        assert_eq!(m.selected, 0);
    }

    #[test]
    fn test_navigate_to_result_closes_modal() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        m.handle_key(KeyCode::Char('x'), some_results);
        assert_eq!(m.file_results.len(), 2);

        let action = m.handle_key(KeyCode::Enter, no_search);
        assert!(!m.active);
        assert!(matches!(action, ModalAction::Navigate(_)));
    }

    #[test]
    fn test_enter_empty_returns_close() {
        let mut m = SemanticSearchModal::new();
        m.active = true;
        let action = m.handle_key(KeyCode::Enter, no_search);
        assert!(matches!(action, ModalAction::Close));
    }
}
```

- [ ] **Step 2: mode.rs에 SemanticSearch 액션 추가**

`Action` enum에 추가:
```rust
// ScrollUp 다음에 추가
SemanticSearch,
```

`KeyBindings::default_bindings()`에 추가:
```rust
// ScrollUp 바인딩 다음에 추가
bindings.insert(KeyCode::Char('S'), Action::SemanticSearch);
```

- [ ] **Step 3: 테스트 실행**

```bash
cargo test search::modal
cargo test mode::tests
```
Expected: 모든 테스트 PASS

- [ ] **Step 4: 커밋**

```bash
git add src/search/modal.rs src/mode.rs
git commit -m "Implement semantic search modal with unified Files+Commits view"
```

---

### Task 11: UI 렌더러 + App 통합

**Files:**
- Create: `src/ui/search_modal.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: src/ui/search_modal.rs 생성**

```rust
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::search::modal::{Section, SemanticSearchModal};

pub fn render_search_modal(frame: &mut Frame, area: Rect, modal: &SemanticSearchModal) {
    let popup = centered_rect(76, 72, area);
    frame.render_widget(Clear, popup);

    let border_style = Style::default().fg(Color::Cyan);
    let block = Block::default()
        .title(" Semantic Search (S) ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if modal.no_index {
        render_message(frame, inner, &[
            Line::from(""),
            Line::from(Span::styled("  No search index found.", Style::default().fg(Color::Yellow))),
            Line::from(Span::raw("  Run `glc index` to build one.")),
            Line::from(""),
            Line::from(Span::styled("  [Esc] close", Style::default().fg(Color::DarkGray))),
        ]);
        return;
    }

    if modal.incompatible {
        render_message(frame, inner, &[
            Line::from(""),
            Line::from(Span::styled("  Index format outdated.", Style::default().fg(Color::Red))),
            Line::from(Span::raw("  Run `glc index --force` to rebuild.")),
            Line::from(""),
            Line::from(Span::styled("  [Esc] close", Style::default().fg(Color::DarkGray))),
        ]);
        return;
    }

    // Layout: [input 3] [warning 1?] [results rest] [help 1]
    let warning_lines = if modal.warning.is_some() { 1u16 } else { 0u16 };
    let constraints = [
        Constraint::Length(3),
        Constraint::Length(warning_lines),
        Constraint::Min(4),
        Constraint::Length(1),
    ];
    let chunks = Layout::vertical(constraints).split(inner);

    // Input
    let input_block = Block::default().borders(Borders::ALL).title(" Query ");
    let input_widget = Paragraph::new(modal.input.as_str()).block(input_block);
    frame.render_widget(input_widget, chunks[0]);

    // Stale warning
    if let Some(ref w) = modal.warning {
        let warn = Paragraph::new(Span::styled(
            format!("  ⚠ {w}"),
            Style::default().fg(Color::Yellow),
        ));
        frame.render_widget(warn, chunks[1]);
    }

    // Results: split 50/50 between Files and Commits
    let result_area = chunks[2];
    let [files_area, commits_area] = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ]).areas(result_area);

    render_section(frame, files_area, "Files", &modal.file_results, &modal.focused_section, Section::Files, modal.selected);
    render_section(frame, commits_area, "Commits", &modal.commit_results, &modal.focused_section, Section::Commits, modal.selected);

    // Help bar
    let help = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(Color::Green)),
        Span::raw(" open  "),
        Span::styled("[Esc]", Style::default().fg(Color::Green)),
        Span::raw(" close  "),
        Span::styled("[Tab]", Style::default().fg(Color::Green)),
        Span::raw(" section  "),
        Span::styled("[j/k↑↓]", Style::default().fg(Color::Green)),
        Span::raw(" navigate"),
    ]));
    frame.render_widget(help, chunks[3]);
}

fn render_section(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    results: &[crate::search::SearchResult],
    focused: &Section,
    this_section: Section,
    selected: usize,
) {
    let is_focused = *focused == this_section;
    let title_style = if is_focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(Span::styled(format!(" {title} "), title_style))
        .borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let is_selected = is_focused && i == selected;
            let marker = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(r.title.clone(), style),
                Span::styled(
                    format!("  {:.3}", r.score),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn render_message(frame: &mut Frame, area: Rect, lines: &[Line]) {
    let para = Paragraph::new(lines.to_vec());
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vert[1])[1]
}
```

- [ ] **Step 2: ui/mod.rs에 등록**

`src/ui/mod.rs`에 추가:
```rust
pub mod search_modal;
```

- [ ] **Step 3: App에 search_engine + search_modal 추가**

`src/app.rs` 상단 import에 추가:
```rust
use crate::search::modal::{ModalAction, SemanticSearchModal};
use crate::search::SearchEngine;
```

`App` 구조체에 필드 추가:
```rust
pub search_engine: SearchEngine,
pub search_modal: SemanticSearchModal,
```

`App::new()`에서 `search_engine`, `search_modal` 초기화 (config 로드 직후):
```rust
let repo_path = repo.repository()
    .workdir()
    .unwrap_or_else(|| repo.repository().path())
    .to_path_buf();
let index_root = repo_path.join(".glc-index");
let mut search_engine = SearchEngine::new(index_root);
// 인덱스가 있으면 열기 (실패해도 TUI는 정상 시작)
let _ = search_engine.open();
let search_modal = SemanticSearchModal::new();
```

`Self { ... }` 생성부에 추가:
```rust
search_engine,
search_modal,
```

- [ ] **Step 4: handle_key에 모달 인터셉트 + S 키 핸들러 추가**

`handle_key` 메서드 최상단에 추가 (기존 `is_searching` 체크 **앞**에):
```rust
// Semantic search modal이 활성화된 경우 모든 키를 모달로 전달
if self.search_modal.active {
    let engine = &self.search_engine;
    let action = self.search_modal.handle_key(code, |q| {
        engine.search(q).unwrap_or_default()
    });
    if let ModalAction::Navigate(result) = action {
        self.navigate_to_search_result(result);
    }
    return;
}
```

`match action` 블록에 추가:
```rust
Action::SemanticSearch => self.open_semantic_search(),
```

helper 메서드 추가 (impl App):
```rust
fn open_semantic_search(&mut self) {
    let head = self.current_head_oid();
    let is_available = self.search_engine.is_available();
    let is_stale = self.search_engine.is_stale(&head);
    let is_incompatible = is_available && self.search_engine.read_meta()
        .map(|m| m.verify_version().is_err())
        .unwrap_or(false);
    self.search_modal.open(is_available, is_stale, is_incompatible);
}

fn current_head_oid(&self) -> String {
    self.repo.repository()
        .head().ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default()
}

fn navigate_to_search_result(&mut self, result: crate::search::SearchResult) {
    use crate::search::DocKind;
    match result.kind {
        DocKind::Commit => {
            if let Some(oid_str) = &result.commit_oid {
                if let Some(idx) = self.store.loaded.iter().position(|c| c.id.to_string() == *oid_str) {
                    let commit = self.store.loaded[idx].clone();
                    let view_state = self.make_view_state(commit);
                    self.mode = crate::mode::Mode::View(view_state);
                    self.load_view_file();
                }
            }
        }
        DocKind::File => {
            if !self.store.loaded.is_empty() {
                let commit = self.store.loaded[0].clone();
                let mut view_state = self.make_view_state(commit);
                if let Some(path) = &result.path {
                    if let Some(idx) = view_state.tree.iter().position(|e| &e.path == path) {
                        view_state.selected_file = idx;
                    }
                }
                self.mode = crate::mode::Mode::View(view_state);
                self.load_view_file();
            }
        }
    }
}
```

- [ ] **Step 5: render()에 모달 오버레이 추가**

```rust
pub fn render(&self, frame: &mut Frame) {
    match &self.mode {
        Mode::Pick(_) => ui::pick::render_pick(frame, frame.area(), self),
        Mode::View(_) => ui::view::render_view(frame, frame.area(), self),
        Mode::Diff(_) => ui::diff::render_diff(frame, frame.area(), self),
    }

    if self.search_modal.active {
        ui::search_modal::render_search_modal(frame, frame.area(), &self.search_modal);
    }

    if self.debug_overlay {
        self.render_debug_overlay(frame);
    }
}
```

- [ ] **Step 6: 컴파일 확인**

```bash
cargo check
```
Expected: 성공 (import 오류 있으면 수정)

- [ ] **Step 7: 통합 테스트**

`src/app.rs` 테스트 모듈에 추가:

```rust
#[test]
fn test_s_key_opens_search_modal() {
    let (_dir, mut app) = test_app();
    assert!(!app.search_modal.active);
    app.handle_key(KeyCode::Char('S'));
    assert!(app.search_modal.active);
    assert!(app.search_modal.no_index); // index not built yet
}

#[test]
fn test_modal_esc_closes() {
    let (_dir, mut app) = test_app();
    app.handle_key(KeyCode::Char('S'));
    assert!(app.search_modal.active);
    app.handle_key(KeyCode::Esc);
    assert!(!app.search_modal.active);
}

#[test]
fn test_modal_typing_when_no_index_does_not_panic() {
    let (_dir, mut app) = test_app();
    app.handle_key(KeyCode::Char('S'));
    assert!(app.search_modal.no_index);
    // Typing should not panic even with no index
    app.handle_key(KeyCode::Char('e'));
    app.handle_key(KeyCode::Char('r'));
    assert_eq!(app.search_modal.input, "er");
}

#[test]
fn test_modal_keys_not_propagated_to_app() {
    let (_dir, mut app) = test_app();
    app.handle_key(KeyCode::Char('S'));
    assert!(app.search_modal.active);
    // 'q' would quit the app, but modal intercepts it
    app.handle_key(KeyCode::Char('q'));
    assert!(!app.should_quit, "modal should intercept 'q'");
}
```

- [ ] **Step 8: 전체 테스트 실행**

```bash
cargo test
```
Expected: 모든 테스트 PASS

- [ ] **Step 9: 커밋**

```bash
git add src/ui/search_modal.rs src/ui/mod.rs src/app.rs
git commit -m "Integrate semantic search modal into TUI: S key, overlay renderer, navigation"
```

---

### Task 12: E2E 검증 + .gitignore

**Files:**
- `.gitignore` (create or modify)

- [ ] **Step 1: release 빌드 확인**

```bash
cargo build --release 2>&1 | tail -5
```
Expected: `Finished release` 메시지

- [ ] **Step 2: gluck 레포 자체에 인덱스 빌드**

```bash
cargo run --release -- index .
```
Expected:
- `Building search index for ...` 출력
- `Done. N chunks indexed.` 출력
- `.glc-index/meta.toml`, `.glc-index/bm25/`, `.glc-index/vectors/index.tvim` 파일 생성

- [ ] **Step 3: meta.toml 내용 확인**

```bash
cat .glc-index/meta.toml
```
Expected:
```toml
version = 2
head_oid = "<current HEAD oid>"
doc_count = <N>
vector_backend = "turboquant_4bit"
```

- [ ] **Step 4: TUI에서 검색 연습**

```bash
cargo run --release
```
- `S` 키 → 통합 모달 열림 확인
- `error` 타이핑 → Files + Commits 섹션에 결과 표시 확인
- `Tab` → Commits 섹션으로 포커스 이동 확인
- `Enter` → 해당 결과로 네비게이션 확인
- `Esc` → 모달 닫힘 확인

- [ ] **Step 5: --force 재빌드 테스트**

```bash
cargo run --release -- index . --force
```
Expected: 인덱스 재빌드 완료 (기존 `.glc-index/` 교체)

- [ ] **Step 6: .gitignore 추가**

`.gitignore` 파일 끝에 추가 (없으면 생성):
```
.glc-index/
```

- [ ] **Step 7: cargo clippy**

```bash
cargo clippy -- -D warnings 2>&1 | head -30
```
Expected: 경고 없음 (있으면 수정 후 계속)

- [ ] **Step 8: 최종 커밋**

```bash
git add .gitignore
git commit -m "Add .glc-index to gitignore"
```

---

## 요약

| Task | 파일 | 핵심 결과물 |
|------|------|-----------|
| 1 | `Cargo.toml`, `src/search/*` skeleton | 컴파일되는 모듈 뼈대 |
| 2 | `vector.rs` | turbovec IdMapIndex 래퍼, l2_normalize |
| 3 | `bm25.rs`, `chunk.rs` | u64 doc_id BM25, Korean bigram tokenizer |
| 4 | `rrf.rs` | u64 기반 RRF fusion |
| 5 | `chunk.rs` (완성) | tree-sitter Rust 청킹 + fixed-size fallback |
| 6 | `embedding.rs` | fastembed 래퍼 + deterministic stub |
| 7 | `indexer.rs` | 전체 파이프라인 (commits + files → tantivy + turbovec) |
| 8 | `cli.rs`, `main.rs` | `glc index --force` 서브커맨드 |
| 9 | `config.rs` | `[search]` 섹션 |
| 10 | `modal.rs`, `mode.rs` | 통합 모달 상태머신, `S` 키 바인딩 |
| 11 | `ui/search_modal.rs`, `app.rs` | ratatui 오버레이, 앱 통합 |
| 12 | `.gitignore`, e2e | 전체 동작 검증 |

### 알려진 제약

- **임베딩 품질**: `AllMiniLML6V2` (384-dim, 영어 위주). 한국어 품질 개선은 `embedding.rs`의 `MODEL` 상수를 `JinaEmbeddingsV3`로 교체 (fastembed v4에서 지원 여부 확인 필요).
- **turbovec ID 타입**: IdMapIndex가 `i64`를 사용하면 `vector.rs`의 `ids_i64` 캐스팅이 이미 처리. `u64` 사용 시 캐스팅 제거.
- **tree-sitter 언어**: 현재 Rust만 함수 단위 청킹. 다른 언어는 fixed-size. 언어 추가는 `chunk.rs`의 `split_file` match 확장.
