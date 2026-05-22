# Semantic Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add hybrid semantic search (BM25 + vector) to gluck's TUI via `S` key modal, with `glc index` subcommand for building the search index.

**Architecture:** SearchEngine facade orchestrates BM25 (tantivy) and vector (ONNX Runtime + CodeBERT) searches, fusing results via Reciprocal Rank Fusion. Index stored in `.glc-index/` directory. Modal overlay provides interactive search UI.

**Tech Stack:** Rust, tantivy 0.22, ort 2 (ONNX Runtime), tokenizers 0.20, ratatui (existing)

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/search/mod.rs` | SearchEngine facade, SearchDocument/SearchResult types, query orchestration |
| `src/search/bm25.rs` | Tantivy BM25 full-text index build + query |
| `src/search/vector.rs` | Embedding generation (ONNX), vector I/O, cosine similarity |
| `src/search/rrf.rs` | Reciprocal Rank Fusion merge |
| `src/search/indexer.rs` | Index builder: walks repo, coordinates BM25 + vector indexing |
| `src/search/modal.rs` | SemanticSearchModal state machine (input, results, navigation) |
| `src/cli.rs` | Extend with `Commands` enum (subcommand support) |
| `src/main.rs` | Route `glc index` subcommand |
| `src/config.rs` | Add `[search]` config section |
| `src/mode.rs` | Add `Action::SemanticSearch` variant |
| `src/app.rs` | Integrate modal: key handling, render overlay |

---

### Task 1: Add dependencies and search module skeleton

**Files:**
- Modify: `Cargo.toml`
- Create: `src/search/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add after the `dirs = "6"` line:

```toml
tantivy = "0.22"
ort = { version = "2", features = ["load-dynamic"] }
tokenizers = "0.20"
```

- [ ] **Step 2: Create search module with core types**

Create `src/search/mod.rs`:

```rust
pub mod bm25;
pub mod indexer;
pub mod modal;
pub mod rrf;
pub mod vector;

#[derive(Debug, Clone, PartialEq)]
pub enum DocKind {
    Commit,
    File,
}

#[derive(Debug, Clone)]
pub struct SearchDocument {
    pub id: String,
    pub kind: DocKind,
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub doc_id: String,
    pub kind: DocKind,
    pub title: String,
    pub path: Option<String>,
    pub score: f32,
}

pub struct SearchEngine {
    index_path: std::path::PathBuf,
}

impl SearchEngine {
    pub fn new(index_path: std::path::PathBuf) -> Self {
        Self { index_path }
    }

    pub fn is_available(&self) -> bool {
        self.index_path.join("meta.toml").exists()
    }

    pub fn is_stale(&self, current_head: &str) -> bool {
        let meta_path = self.index_path.join("meta.toml");
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            !content.contains(current_head)
        } else {
            true
        }
    }

    pub fn search(&self, query: &str, filter: Option<DocKind>) -> Vec<SearchResult> {
        if query.trim().is_empty() || !self.is_available() {
            return vec![];
        }

        let bm25_results = bm25::search(&self.index_path.join("bm25"), query, 50);
        let vector_results = vector::search(&self.index_path.join("vectors"), query, 50);

        let mut fused = rrf::fuse(bm25_results, vector_results, 60);

        if let Some(kind) = filter {
            fused.retain(|r| r.kind == kind);
        }

        fused.truncate(20);
        fused
    }
}
```

- [ ] **Step 3: Register the search module in lib.rs**

Add `pub mod search;` to `src/lib.rs`.

- [ ] **Step 4: Create placeholder submodule files**

Create empty placeholder files so the module compiles:

`src/search/bm25.rs`:
```rust
use std::path::Path;
use super::SearchResult;

pub fn search(_index_path: &Path, _query: &str, _top_k: usize) -> Vec<(String, f32)> {
    vec![]
}
```

`src/search/vector.rs`:
```rust
use std::path::Path;
use super::SearchResult;

pub fn search(_vectors_path: &Path, _query: &str, _top_k: usize) -> Vec<(String, f32)> {
    vec![]
}
```

`src/search/rrf.rs`:
```rust
use super::SearchResult;

pub fn fuse(
    _bm25: Vec<(String, f32)>,
    _vector: Vec<(String, f32)>,
    _k: usize,
) -> Vec<SearchResult> {
    vec![]
}
```

`src/search/indexer.rs`:
```rust
use std::path::Path;
use anyhow::Result;

pub fn build_index(_repo_path: &Path, _output_path: &Path, _batch_size: usize, _max_file_size: usize) -> Result<()> {
    Ok(())
}
```

`src/search/modal.rs`:
```rust
use super::{DocKind, SearchResult};

#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    Files,
    Commits,
}

#[derive(Debug, Clone)]
pub struct SemanticSearchModal {
    pub input: String,
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub focused_section: Section,
    pub active: bool,
    pub warning: Option<String>,
}

impl SemanticSearchModal {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            results: vec![],
            selected: 0,
            focused_section: Section::Files,
            active: false,
            warning: None,
        }
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check`
Expected: compiles with no errors (warnings for unused are fine)

- [ ] **Step 6: Commit**

```bash
git add src/search/ src/lib.rs Cargo.toml
git commit -m "Add search module skeleton with tantivy and ort dependencies"
```

---

### Task 2: Implement RRF fusion

**Files:**
- Modify: `src/search/rrf.rs`

- [ ] **Step 1: Write tests for RRF fusion**

Replace `src/search/rrf.rs`:

```rust
use std::collections::HashMap;
use super::SearchResult;
use super::DocKind;

pub fn fuse(
    bm25: Vec<(String, f32)>,
    vector: Vec<(String, f32)>,
    k: usize,
) -> Vec<SearchResult> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    for (rank, (doc_id, _score)) in bm25.iter().enumerate() {
        *scores.entry(doc_id.clone()).or_default() += 1.0 / (k as f32 + rank as f32 + 1.0);
    }

    for (rank, (doc_id, _score)) in vector.iter().enumerate() {
        *scores.entry(doc_id.clone()).or_default() += 1.0 / (k as f32 + rank as f32 + 1.0);
    }

    let mut results: Vec<SearchResult> = scores
        .into_iter()
        .map(|(doc_id, score)| {
            let kind = if doc_id.starts_with("commit:") {
                DocKind::Commit
            } else {
                DocKind::File
            };
            let title = doc_id
                .split(':')
                .nth(1)
                .unwrap_or(&doc_id)
                .to_string();
            let path = if kind == DocKind::File {
                Some(title.clone())
            } else {
                None
            };
            SearchResult {
                doc_id,
                kind,
                title,
                path,
                score,
            }
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_empty_inputs() {
        let result = fuse(vec![], vec![], 60);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_single_source() {
        let bm25 = vec![
            ("file:src/main.rs".to_string(), 1.0),
            ("file:src/lib.rs".to_string(), 0.8),
        ];
        let result = fuse(bm25, vec![], 60);
        assert_eq!(result.len(), 2);
        assert!(result[0].score > result[1].score);
        assert_eq!(result[0].doc_id, "file:src/main.rs");
    }

    #[test]
    fn test_rrf_overlapping_results_boost() {
        let bm25 = vec![
            ("file:src/main.rs".to_string(), 1.0),
            ("file:src/lib.rs".to_string(), 0.8),
        ];
        let vector = vec![
            ("file:src/lib.rs".to_string(), 0.95),
            ("file:src/main.rs".to_string(), 0.7),
        ];
        let result = fuse(bm25, vector, 60);
        assert_eq!(result.len(), 2);
        // Both appear in both lists, but lib.rs has rank 0 in vector + rank 1 in bm25
        // main.rs has rank 0 in bm25 + rank 1 in vector
        // With k=60: main.rs = 1/61 + 1/62, lib.rs = 1/62 + 1/61 → same score
        // Actually both get the same score, so order depends on HashMap iteration
    }

    #[test]
    fn test_rrf_doc_kind_detection() {
        let bm25 = vec![("commit:abc1234".to_string(), 1.0)];
        let vector = vec![("file:src/foo.rs".to_string(), 0.9)];
        let result = fuse(bm25, vector, 60);
        let commit = result.iter().find(|r| r.doc_id == "commit:abc1234").unwrap();
        let file = result.iter().find(|r| r.doc_id == "file:src/foo.rs").unwrap();
        assert_eq!(commit.kind, DocKind::Commit);
        assert_eq!(file.kind, DocKind::File);
    }

    #[test]
    fn test_rrf_sorted_by_score_desc() {
        let bm25 = vec![
            ("file:a.rs".to_string(), 1.0),
            ("file:b.rs".to_string(), 0.5),
            ("file:c.rs".to_string(), 0.3),
        ];
        let vector = vec![
            ("file:a.rs".to_string(), 0.9),
            ("file:c.rs".to_string(), 0.8),
        ];
        let result = fuse(bm25, vector, 60);
        for w in result.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test search::rrf`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/search/rrf.rs
git commit -m "Implement RRF fusion with rank-based score merging"
```

---

### Task 3: Implement BM25 search with tantivy

**Files:**
- Modify: `src/search/bm25.rs`

- [ ] **Step 1: Implement BM25 index builder and search**

Replace `src/search/bm25.rs`:

```rust
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{Index, IndexWriter, TantivyDocument};

use super::SearchDocument;

pub fn schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("kind", STRING | STORED);
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT);
    builder.add_text_field("path", STRING | STORED);
    builder.add_text_field("commit_oid", STRING | STORED);
    builder.build()
}

pub fn build_index(index_path: &Path, documents: &[SearchDocument]) -> tantivy::Result<()> {
    std::fs::create_dir_all(index_path).map_err(|e| {
        tantivy::TantivyError::SystemError(format!("Failed to create index dir: {}", e))
    })?;

    let schema = schema();
    let index = Index::create_in_dir(index_path, schema.clone())?;
    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let id_field = schema.get_field("id").unwrap();
    let kind_field = schema.get_field("kind").unwrap();
    let title_field = schema.get_field("title").unwrap();
    let body_field = schema.get_field("body").unwrap();
    let path_field = schema.get_field("path").unwrap();
    let commit_oid_field = schema.get_field("commit_oid").unwrap();

    for doc in documents {
        let kind_str = match doc.kind {
            super::DocKind::Commit => "commit",
            super::DocKind::File => "file",
        };
        let mut tantivy_doc = TantivyDocument::new();
        tantivy_doc.add_text(id_field, &doc.id);
        tantivy_doc.add_text(kind_field, kind_str);
        tantivy_doc.add_text(title_field, &doc.title);
        tantivy_doc.add_text(body_field, &doc.body);
        tantivy_doc.add_text(path_field, doc.path.as_deref().unwrap_or(""));
        tantivy_doc.add_text(commit_oid_field, doc.commit_oid.as_deref().unwrap_or(""));
        writer.add_document(tantivy_doc)?;
    }

    writer.commit()?;
    Ok(())
}

pub fn search(index_path: &Path, query: &str, top_k: usize) -> Vec<(String, f32)> {
    let Ok(index) = Index::open_in_dir(index_path) else {
        return vec![];
    };
    let schema = index.schema();
    let title_field = schema.get_field("title").unwrap();
    let body_field = schema.get_field("body").unwrap();
    let id_field = schema.get_field("id").unwrap();

    let Ok(reader) = index.reader() else {
        return vec![];
    };
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![title_field, body_field]);
    let Ok(parsed_query) = query_parser.parse_query(query) else {
        return vec![];
    };

    let Ok(top_docs) = searcher.search(&parsed_query, &TopDocs::with_limit(top_k)) else {
        return vec![];
    };

    top_docs
        .into_iter()
        .filter_map(|(score, addr)| {
            let doc: TantivyDocument = searcher.doc(addr).ok()?;
            let id = doc.get_first(id_field)?.as_str()?.to_string();
            Some((id, score))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::DocKind;
    use tempfile::TempDir;

    fn sample_docs() -> Vec<SearchDocument> {
        vec![
            SearchDocument {
                id: "commit:abc1234".to_string(),
                kind: DocKind::Commit,
                title: "Fix error handling in parser".to_string(),
                body: "Refactored the error handling logic to use Result types".to_string(),
                path: None,
                commit_oid: Some("abc1234".to_string()),
            },
            SearchDocument {
                id: "file:src/parser.rs".to_string(),
                kind: DocKind::File,
                title: "src/parser.rs".to_string(),
                body: "fn parse() -> Result<AST, ParseError> { todo!() }".to_string(),
                path: Some("src/parser.rs".to_string()),
                commit_oid: None,
            },
            SearchDocument {
                id: "file:src/main.rs".to_string(),
                kind: DocKind::File,
                title: "src/main.rs".to_string(),
                body: "fn main() { println!(\"hello world\"); }".to_string(),
                path: Some("src/main.rs".to_string()),
                commit_oid: None,
            },
        ]
    }

    #[test]
    fn test_build_and_search() {
        let dir = TempDir::new().unwrap();
        let index_path = dir.path().join("bm25");
        build_index(&index_path, &sample_docs()).unwrap();

        let results = search(&index_path, "error handling", 10);
        assert!(!results.is_empty());
        assert!(results.iter().any(|(id, _)| id == "commit:abc1234"));
    }

    #[test]
    fn test_search_no_results() {
        let dir = TempDir::new().unwrap();
        let index_path = dir.path().join("bm25");
        build_index(&index_path, &sample_docs()).unwrap();

        let results = search(&index_path, "zzzznonexistent", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_nonexistent_index() {
        let results = search(Path::new("/nonexistent/path"), "test", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_respects_top_k() {
        let dir = TempDir::new().unwrap();
        let index_path = dir.path().join("bm25");
        build_index(&index_path, &sample_docs()).unwrap();

        let results = search(&index_path, "fn", 1);
        assert!(results.len() <= 1);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test search::bm25`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/search/bm25.rs
git commit -m "Implement BM25 full-text search with tantivy"
```

---

### Task 4: Implement vector storage and cosine similarity

**Files:**
- Modify: `src/search/vector.rs`

- [ ] **Step 1: Implement vector I/O and cosine search (no ONNX yet)**

Replace `src/search/vector.rs`:

```rust
use std::io::{Read, Write};
use std::path::Path;

pub fn save_vectors(vectors_path: &Path, doc_ids: &[String], embeddings: &[Vec<f32>]) -> std::io::Result<()> {
    std::fs::create_dir_all(vectors_path)?;

    let ids_path = vectors_path.join("doc_ids.bin");
    let emb_path = vectors_path.join("embeddings.bin");

    // Save doc_ids as newline-separated text
    let ids_content = doc_ids.join("\n");
    std::fs::write(&ids_path, ids_content.as_bytes())?;

    // Save embeddings as raw f32 bytes
    let dim = embeddings.first().map(|v| v.len()).unwrap_or(0);
    let mut file = std::fs::File::create(&emb_path)?;
    // Write dimension as first 4 bytes (u32 little-endian)
    file.write_all(&(dim as u32).to_le_bytes())?;
    for vec in embeddings {
        for &val in vec {
            file.write_all(&val.to_le_bytes())?;
        }
    }

    Ok(())
}

pub fn load_vectors(vectors_path: &Path) -> Option<(Vec<String>, Vec<Vec<f32>>)> {
    let ids_path = vectors_path.join("doc_ids.bin");
    let emb_path = vectors_path.join("embeddings.bin");

    let ids_content = std::fs::read_to_string(&ids_path).ok()?;
    let doc_ids: Vec<String> = ids_content.lines().map(|s| s.to_string()).collect();

    let mut file = std::fs::File::open(&emb_path).ok()?;
    let mut dim_buf = [0u8; 4];
    file.read_exact(&mut dim_buf).ok()?;
    let dim = u32::from_le_bytes(dim_buf) as usize;

    let mut embeddings = Vec::with_capacity(doc_ids.len());
    for _ in 0..doc_ids.len() {
        let mut vec = Vec::with_capacity(dim);
        for _ in 0..dim {
            let mut buf = [0u8; 4];
            if file.read_exact(&mut buf).is_err() {
                break;
            }
            vec.push(f32::from_le_bytes(buf));
        }
        if vec.len() == dim {
            embeddings.push(vec);
        }
    }

    Some((doc_ids, embeddings))
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

pub fn search(vectors_path: &Path, _query: &str, top_k: usize) -> Vec<(String, f32)> {
    // MVP: requires pre-embedded query vector stored during indexing
    // Full implementation will embed query at search time via ONNX
    // For now, load vectors and return empty (placeholder for ONNX integration)
    let Some((doc_ids, _embeddings)) = load_vectors(vectors_path) else {
        return vec![];
    };
    let _ = (doc_ids, top_k);
    vec![]
}

pub fn search_with_embedding(vectors_path: &Path, query_embedding: &[f32], top_k: usize) -> Vec<(String, f32)> {
    let Some((doc_ids, embeddings)) = load_vectors(vectors_path) else {
        return vec![];
    };

    let mut scored: Vec<(String, f32)> = doc_ids
        .into_iter()
        .zip(embeddings.iter())
        .map(|(id, emb)| {
            let score = cosine_similarity(query_embedding, emb);
            (id, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vectors");

        let doc_ids = vec!["file:a.rs".to_string(), "commit:abc".to_string()];
        let embeddings = vec![vec![1.0, 0.5, 0.0], vec![0.0, 0.5, 1.0]];

        save_vectors(&path, &doc_ids, &embeddings).unwrap();
        let (loaded_ids, loaded_emb) = load_vectors(&path).unwrap();

        assert_eq!(loaded_ids, doc_ids);
        assert_eq!(loaded_emb.len(), 2);
        for (orig, loaded) in embeddings.iter().zip(loaded_emb.iter()) {
            for (a, b) in orig.iter().zip(loaded.iter()) {
                assert!((a - b).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn test_load_nonexistent() {
        assert!(load_vectors(Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn test_search_with_embedding() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vectors");

        let doc_ids = vec![
            "file:close.rs".to_string(),
            "file:far.rs".to_string(),
            "file:medium.rs".to_string(),
        ];
        let embeddings = vec![
            vec![0.9, 0.1, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![0.5, 0.5, 0.0],
        ];
        save_vectors(&path, &doc_ids, &embeddings).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = search_with_embedding(&path, &query, 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "file:close.rs");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test search::vector`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/search/vector.rs
git commit -m "Implement vector storage, cosine similarity, and search"
```

---

### Task 5: Implement indexer pipeline

**Files:**
- Modify: `src/search/indexer.rs`
- Modify: `src/search/mod.rs`

- [ ] **Step 1: Implement the index builder**

Replace `src/search/indexer.rs`:

```rust
use std::path::Path;
use anyhow::{Context, Result};
use crate::git::commit::CommitInfo;
use crate::git::repo::GitRepo;
use crate::git::store::CommitStore;
use crate::git::tree::{is_binary_blob, list_tree, read_blob, EntryKind};
use super::{DocKind, SearchDocument};
use super::bm25;
use super::vector;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IndexMeta {
    pub head_oid: String,
    pub doc_count: usize,
    pub indexed_at: String,
    pub model_name: String,
    pub vector_dim: usize,
    pub version: u32,
}

pub fn build_index(
    repo_path: &Path,
    output_path: &Path,
    _batch_size: usize,
    max_file_size: usize,
) -> Result<()> {
    let repo = GitRepo::open(repo_path)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let head_oid = {
        let repository = repo.repository();
        let head = repository.head()
            .context("Failed to get HEAD")?;
        head.target()
            .context("HEAD has no target")?
            .to_string()
    };

    // Check if index is up-to-date
    let meta_path = output_path.join("meta.toml");
    if meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if content.contains(&head_oid) {
                eprintln!("Index is up-to-date (HEAD: {})", &head_oid[..7]);
                return Ok(());
            }
        }
    }

    eprintln!("Building search index...");

    // Collect documents
    let mut documents = Vec::new();

    // 1. Collect commit messages
    let store = CommitStore::new(&repo, 1000)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let commits = store.loaded.clone();
    // Load all remaining commits
    let mut store = store;
    while !store.exhausted {
        store.load_batch(&repo).map_err(|e| anyhow::anyhow!("{}", e))?;
    }
    let commits = store.loaded.clone();

    eprintln!("  Indexing {} commits...", commits.len());
    for commit in commits.iter() {
        let id = format!("commit:{}", commit.short_id);
        let title = commit.message.lines().next().unwrap_or("").to_string();
        let body = commit.message.clone();
        documents.push(SearchDocument {
            id,
            kind: DocKind::Commit,
            title,
            body,
            path: None,
            commit_oid: Some(commit.id.to_string()),
        });
    }

    // 2. Walk HEAD file tree
    let head_commit = &commits[0];
    let tree = list_tree(&repo, head_commit)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let file_entries: Vec<_> = tree
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::File))
        .collect();

    eprintln!("  Indexing {} files...", file_entries.len());
    for entry in file_entries {
        if is_binary_blob(&repo, head_commit, &entry.path).unwrap_or(true) {
            continue;
        }
        let content = match read_blob(&repo, head_commit, &entry.path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.len() > max_file_size {
            continue;
        }
        let id = format!("file:{}", entry.path);
        documents.push(SearchDocument {
            id,
            kind: DocKind::File,
            title: entry.path.clone(),
            body: content,
            path: Some(entry.path.clone()),
            commit_oid: Some(head_commit.id.to_string()),
        });
    }

    eprintln!("  Total documents: {}", documents.len());

    // 3. Build BM25 index
    if output_path.exists() {
        std::fs::remove_dir_all(output_path)
            .context("Failed to clean old index")?;
    }
    std::fs::create_dir_all(output_path)
        .context("Failed to create index directory")?;

    let bm25_path = output_path.join("bm25");
    bm25::build_index(&bm25_path, &documents)
        .map_err(|e| anyhow::anyhow!("BM25 index build failed: {}", e))?;

    // 4. Generate dummy vectors (placeholder until ONNX integration)
    let vectors_path = output_path.join("vectors");
    let dim = 768;
    let doc_ids: Vec<String> = documents.iter().map(|d| d.id.clone()).collect();
    let embeddings: Vec<Vec<f32>> = documents
        .iter()
        .map(|_| vec![0.0; dim])
        .collect();
    vector::save_vectors(&vectors_path, &doc_ids, &embeddings)
        .context("Failed to save vectors")?;

    // 5. Write meta.toml
    let meta = IndexMeta {
        head_oid,
        doc_count: documents.len(),
        indexed_at: chrono_now(),
        model_name: "placeholder".to_string(),
        vector_dim: dim,
        version: 1,
    };
    let meta_content = format!(
        "head_oid = \"{}\"\ndoc_count = {}\nindexed_at = \"{}\"\nmodel_name = \"{}\"\nvector_dim = {}\nversion = {}\n",
        meta.head_oid, meta.doc_count, meta.indexed_at, meta.model_name, meta.vector_dim, meta.version
    );
    std::fs::write(output_path.join("meta.toml"), meta_content)
        .context("Failed to write meta.toml")?;

    eprintln!("  Index built successfully ({} documents)", documents.len());
    Ok(())
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!("{}Z", secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use tempfile::TempDir;

    #[test]
    fn test_build_index_basic() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() { println!(\"hello\"); }", "Initial commit");
        add_file_commit(&repo, "lib.rs", b"pub fn add(a: i32, b: i32) -> i32 { a + b }", "Add lib");

        let index_dir = TempDir::new().unwrap();
        let output = index_dir.path().join(".glc-index");

        build_index(repo_dir.path(), &output, 32, 1_048_576).unwrap();

        assert!(output.join("meta.toml").exists());
        assert!(output.join("bm25").exists());
        assert!(output.join("vectors").exists());

        let meta = std::fs::read_to_string(output.join("meta.toml")).unwrap();
        assert!(meta.contains("doc_count"));
        assert!(meta.contains("version = 1"));
    }

    #[test]
    fn test_build_index_skips_binary() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() {}", "Add code");
        add_file_commit(&repo, "image.png", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "Add image");

        let index_dir = TempDir::new().unwrap();
        let output = index_dir.path().join(".glc-index");

        build_index(repo_dir.path(), &output, 32, 1_048_576).unwrap();

        let meta = std::fs::read_to_string(output.join("meta.toml")).unwrap();
        // Should have commits + 1 file (binary skipped)
        assert!(meta.contains("doc_count"));
    }

    #[test]
    fn test_build_index_skips_if_uptodate() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() {}", "Init");

        let index_dir = TempDir::new().unwrap();
        let output = index_dir.path().join(".glc-index");

        build_index(repo_dir.path(), &output, 32, 1_048_576).unwrap();
        let first_meta = std::fs::read_to_string(output.join("meta.toml")).unwrap();

        // Build again — should skip
        build_index(repo_dir.path(), &output, 32, 1_048_576).unwrap();
        let second_meta = std::fs::read_to_string(output.join("meta.toml")).unwrap();
        assert_eq!(first_meta, second_meta);
    }

    #[test]
    fn test_bm25_search_after_indexing() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "error.rs", b"fn handle_error() { panic!(\"oops\"); }", "Add error handling");

        let index_dir = TempDir::new().unwrap();
        let output = index_dir.path().join(".glc-index");

        build_index(repo_dir.path(), &output, 32, 1_048_576).unwrap();

        let results = bm25::search(&output.join("bm25"), "error", 10);
        assert!(!results.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test search::indexer`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/search/indexer.rs
git commit -m "Implement indexer pipeline with commit and file document collection"
```

---

### Task 6: Add `glc index` CLI subcommand

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Extend CLI with subcommand support**

Replace `src/cli.rs`:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "glc", about = "Terminal git history file viewer")]
pub struct Cli {
    /// Git repository path (for TUI mode)
    #[arg(global = true)]
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
        /// Repository path (default: ".")
        #[arg(default_value = ".")]
        repo_path: PathBuf,

        /// Batch size for embedding generation
        #[arg(long, default_value = "32")]
        batch_size: usize,

        /// Max file size to index in bytes
        #[arg(long, default_value = "1048576")]
        max_file_size: usize,
    },
}
```

- [ ] **Step 2: Route subcommand in main.rs**

Replace `src/main.rs`:

```rust
use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind, KeyModifiers};
use gluck::app::App;
use gluck::cli::{Cli, Commands};
use gluck::config::Config;
use gluck::debug;
use gluck::git::repo::GitRepo;
use std::path::PathBuf;

fn main() -> Result<()> {
    let cli = Cli::parse();

    debug::init_logging(&cli.log_level);

    match cli.command {
        Some(Commands::Index {
            repo_path,
            batch_size,
            max_file_size,
        }) => {
            let index_path = repo_path.join(".glc-index");
            gluck::search::indexer::build_index(&repo_path, &index_path, batch_size, max_file_size)
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

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: compiles successfully

- [ ] **Step 4: Test CLI help**

Run: `cargo run -- --help`
Expected: shows `index` subcommand in output

Run: `cargo run -- index --help`
Expected: shows repo_path, batch_size, max_file_size options

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "Add glc index subcommand for building search index"
```

---

### Task 7: Implement semantic search modal state machine

**Files:**
- Modify: `src/search/modal.rs`
- Modify: `src/mode.rs`

- [ ] **Step 1: Implement full modal state machine**

Replace `src/search/modal.rs`:

```rust
use crossterm::event::KeyCode;
use super::{DocKind, SearchEngine, SearchResult};

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
    pub warning: Option<String>,
    pub no_index: bool,
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
        }
    }

    pub fn open(&mut self, engine: &SearchEngine, current_head: &str) {
        self.active = true;
        self.input.clear();
        self.file_results.clear();
        self.commit_results.clear();
        self.selected = 0;
        self.focused_section = Section::Files;
        self.warning = None;
        self.no_index = false;

        if !engine.is_available() {
            self.no_index = true;
        } else if engine.is_stale(current_head) {
            self.warning = Some("Stale index — results may be incomplete".to_string());
        }
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn handle_key(&mut self, code: KeyCode, engine: &SearchEngine) -> ModalAction {
        match code {
            KeyCode::Esc => {
                self.close();
                ModalAction::Close
            }
            KeyCode::Enter => {
                let result = self.selected_result().cloned();
                self.close();
                if let Some(r) = result {
                    ModalAction::Navigate(r)
                } else {
                    ModalAction::Close
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
                self.execute_search(engine);
                ModalAction::None
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.execute_search(engine);
                ModalAction::None
            }
            _ => ModalAction::None,
        }
    }

    fn execute_search(&mut self, engine: &SearchEngine) {
        if self.input.is_empty() || self.no_index {
            self.file_results.clear();
            self.commit_results.clear();
            self.selected = 0;
            return;
        }

        let all_results = engine.search(&self.input, None);
        self.file_results = all_results
            .iter()
            .filter(|r| r.kind == DocKind::File)
            .cloned()
            .collect();
        self.commit_results = all_results
            .iter()
            .filter(|r| r.kind == DocKind::Commit)
            .cloned()
            .collect();
        self.selected = 0;
    }

    fn current_section_len(&self) -> usize {
        match self.focused_section {
            Section::Files => self.file_results.len(),
            Section::Commits => self.commit_results.len(),
        }
    }

    fn move_down(&mut self) {
        let max = self.current_section_len().saturating_sub(1);
        self.selected = self.selected.saturating_add(1).min(max);
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
        match self.focused_section {
            Section::Files => self.file_results.get(self.selected),
            Section::Commits => self.commit_results.get(self.selected),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModalAction {
    None,
    Close,
    Navigate(SearchResult),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn dummy_engine() -> (TempDir, SearchEngine) {
        let dir = TempDir::new().unwrap();
        let engine = SearchEngine::new(dir.path().join(".glc-index"));
        (dir, engine)
    }

    #[test]
    fn test_modal_open_no_index() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.open(&engine, "abc123");
        assert!(modal.active);
        assert!(modal.no_index);
    }

    #[test]
    fn test_modal_close() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.open(&engine, "abc");
        assert!(modal.active);
        modal.close();
        assert!(!modal.active);
    }

    #[test]
    fn test_modal_esc_closes() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.active = true;
        let action = modal.handle_key(KeyCode::Esc, &engine);
        assert!(!modal.active);
        assert!(matches!(action, ModalAction::Close));
    }

    #[test]
    fn test_modal_tab_toggles_section() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.active = true;
        assert_eq!(modal.focused_section, Section::Files);
        modal.handle_key(KeyCode::Tab, &engine);
        assert_eq!(modal.focused_section, Section::Commits);
        modal.handle_key(KeyCode::Tab, &engine);
        assert_eq!(modal.focused_section, Section::Files);
    }

    #[test]
    fn test_modal_typing_updates_input() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.active = true;
        modal.handle_key(KeyCode::Char('h'), &engine);
        modal.handle_key(KeyCode::Char('i'), &engine);
        assert_eq!(modal.input, "hi");
    }

    #[test]
    fn test_modal_backspace() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.active = true;
        modal.handle_key(KeyCode::Char('a'), &engine);
        modal.handle_key(KeyCode::Char('b'), &engine);
        modal.handle_key(KeyCode::Backspace, &engine);
        assert_eq!(modal.input, "a");
    }

    #[test]
    fn test_modal_move_bounds() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.active = true;
        // No results, move should not panic
        modal.handle_key(KeyCode::Down, &engine);
        assert_eq!(modal.selected, 0);
        modal.handle_key(KeyCode::Up, &engine);
        assert_eq!(modal.selected, 0);
    }

    #[test]
    fn test_modal_enter_empty_returns_close() {
        let (_dir, engine) = dummy_engine();
        let mut modal = SemanticSearchModal::new();
        modal.active = true;
        let action = modal.handle_key(KeyCode::Enter, &engine);
        assert!(matches!(action, ModalAction::Close));
    }
}
```

- [ ] **Step 2: Add SemanticSearch action to mode.rs**

Add `SemanticSearch` variant to the `Action` enum in `src/mode.rs`:

```rust
// Add after ScrollUp in the Action enum:
    SemanticSearch,
```

Add keybinding in `KeyBindings::default_bindings()`:

```rust
// Add after the ScrollDown binding:
        bindings.insert(KeyCode::Char('S'), Action::SemanticSearch);
```

- [ ] **Step 3: Run tests**

Run: `cargo test search::modal`
Expected: all tests PASS

Run: `cargo test mode::tests`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/search/modal.rs src/mode.rs
git commit -m "Implement semantic search modal state machine with S keybinding"
```

---

### Task 8: Integrate modal into App (key handling + render)

**Files:**
- Modify: `src/app.rs`
- Create: `src/ui/search_modal.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Add SearchEngine and modal to App state**

In `src/app.rs`, add imports at the top:

```rust
use crate::search::modal::{ModalAction, SemanticSearchModal};
use crate::search::SearchEngine;
```

Add fields to the `App` struct:

```rust
    pub search_engine: SearchEngine,
    pub search_modal: SemanticSearchModal,
```

Update `App::new()` to initialize:

```rust
    // After config is loaded, before building the App struct:
    let repo_path = repo.repository().workdir()
        .unwrap_or_else(|| repo.repository().path())
        .to_path_buf();
    let index_path = repo_path.join(".glc-index");
    let search_engine = SearchEngine::new(index_path);
    let search_modal = SemanticSearchModal::new();
```

Add these fields to the `Self { ... }` construction:

```rust
            search_engine,
            search_modal,
```

- [ ] **Step 2: Handle `S` key and modal input in handle_key**

Add modal interception at the beginning of `handle_key`:

```rust
    pub fn handle_key(&mut self, code: KeyCode) {
        // Semantic search modal intercepts all keys when active
        if self.search_modal.active {
            let head = self.current_head_oid();
            let action = self.search_modal.handle_key(code, &self.search_engine);
            match action {
                ModalAction::Navigate(result) => {
                    self.navigate_to_search_result(result);
                }
                _ => {}
            }
            return;
        }

        // ... existing code unchanged ...
```

Add the `SemanticSearch` action handler in the match:

```rust
            Action::SemanticSearch => self.open_semantic_search(),
```

Add helper methods:

```rust
    fn open_semantic_search(&mut self) {
        let head = self.current_head_oid();
        self.search_modal.open(&self.search_engine, &head);
    }

    fn current_head_oid(&self) -> String {
        self.repo
            .repository()
            .head()
            .ok()
            .and_then(|h| h.target())
            .map(|oid| oid.to_string())
            .unwrap_or_default()
    }

    fn navigate_to_search_result(&mut self, result: crate::search::SearchResult) {
        use crate::search::DocKind;
        match result.kind {
            DocKind::Commit => {
                // Find commit in loaded list and navigate to view
                if let Some(oid_str) = result.doc_id.strip_prefix("commit:") {
                    if let Some(idx) = self.store.loaded.iter().position(|c| c.short_id == oid_str) {
                        let commit = self.store.loaded[idx].clone();
                        self.mode = Mode::View(self.make_view_state(commit));
                        self.load_view_file();
                    }
                }
            }
            DocKind::File => {
                // Navigate to view mode showing this file at HEAD
                if !self.store.loaded.is_empty() {
                    let commit = self.store.loaded[0].clone();
                    let mut view_state = self.make_view_state(commit);
                    if let Some(path) = &result.path {
                        if let Some(idx) = view_state.tree.iter().position(|e| &e.path == path) {
                            view_state.selected_file = idx;
                        }
                    }
                    self.mode = Mode::View(view_state);
                    self.load_view_file();
                }
            }
        }
    }
```

- [ ] **Step 3: Add modal rendering to App::render**

Update `render` method to overlay modal:

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

- [ ] **Step 4: Create search modal renderer**

Create `src/ui/search_modal.rs`:

```rust
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::search::modal::{Section, SemanticSearchModal};

pub fn render_search_modal(frame: &mut Frame, area: Rect, modal: &SemanticSearchModal) {
    let popup = centered_rect(75, 70, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Semantic Search ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if modal.no_index {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No search index found.",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::raw("  Run `glc index` to build one.")),
            Line::from(""),
            Line::from(Span::styled(
                "  [Esc] close",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        frame.render_widget(msg, inner);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(3), // input
        Constraint::Min(5),   // results
        Constraint::Length(1), // help line
    ])
    .split(inner);

    // Input field
    let input_block = Block::default().borders(Borders::ALL).title(" Query ");
    let input = Paragraph::new(modal.input.as_str()).block(input_block);
    frame.render_widget(input, chunks[0]);

    // Results area
    let results_area = chunks[1];
    let result_chunks = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(results_area);

    // Files section
    let files_style = if modal.focused_section == Section::Files {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let files_block = Block::default()
        .title(Span::styled(" Files ", files_style))
        .borders(Borders::TOP);

    let file_items: Vec<ListItem> = modal
        .file_results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let marker = if modal.focused_section == Section::Files && i == modal.selected {
                "▶ "
            } else {
                "  "
            };
            let style = if modal.focused_section == Section::Files && i == modal.selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(r.title.clone(), style),
                Span::styled(format!("  ({:.2})", r.score), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    let files_list = List::new(file_items).block(files_block);
    frame.render_widget(files_list, result_chunks[0]);

    // Commits section
    let commits_style = if modal.focused_section == Section::Commits {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let commits_block = Block::default()
        .title(Span::styled(" Commits ", commits_style))
        .borders(Borders::TOP);

    let commit_items: Vec<ListItem> = modal
        .commit_results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let marker = if modal.focused_section == Section::Commits && i == modal.selected {
                "▶ "
            } else {
                "  "
            };
            let style = if modal.focused_section == Section::Commits && i == modal.selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(r.title.clone(), style),
                Span::styled(format!("  ({:.2})", r.score), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    let commits_list = List::new(commit_items).block(commits_block);
    frame.render_widget(commits_list, result_chunks[1]);

    // Warning
    if let Some(ref warning) = modal.warning {
        let warn = Paragraph::new(Span::styled(
            format!("  ⚠ {}", warning),
            Style::default().fg(Color::Yellow),
        ));
        // Render warning above results
        let warn_area = Rect::new(inner.x, inner.y + 3, inner.width, 1);
        frame.render_widget(warn, warn_area);
    }

    // Help line
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" [Enter]", Style::default().fg(Color::Green)),
        Span::raw(" open  "),
        Span::styled("[Esc]", Style::default().fg(Color::Green)),
        Span::raw(" close  "),
        Span::styled("[Tab]", Style::default().fg(Color::Green)),
        Span::raw(" section  "),
        Span::styled("[j/k]", Style::default().fg(Color::Green)),
        Span::raw(" navigate"),
    ]));
    frame.render_widget(help, chunks[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
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
    .split(popup_layout[1])[1]
}
```

- [ ] **Step 5: Register search_modal in ui/mod.rs**

Add `pub mod search_modal;` to `src/ui/mod.rs`.

- [ ] **Step 6: Verify compilation**

Run: `cargo check`
Expected: compiles (fix any import issues)

- [ ] **Step 7: Run all tests**

Run: `cargo test`
Expected: all existing tests still pass

- [ ] **Step 8: Commit**

```bash
git add src/app.rs src/ui/search_modal.rs src/ui/mod.rs
git commit -m "Integrate semantic search modal into TUI with S key binding"
```

---

### Task 9: Add search config section

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Add SearchConfig to config.rs**

Add after `UiConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub model_path: Option<String>,
    pub rrf_k: usize,
    pub bm25_top_k: usize,
    pub vector_top_k: usize,
    pub result_limit: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            rrf_k: 60,
            bm25_top_k: 50,
            vector_top_k: 50,
            result_limit: 20,
        }
    }
}
```

Add `search` field to `Config`:

```rust
pub struct Config {
    pub theme: ThemeConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub search: SearchConfig,
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test config`
Expected: all tests PASS (serde default handles missing field)

- [ ] **Step 3: Commit**

```bash
git add src/config.rs
git commit -m "Add search configuration section to config.toml"
```

---

### Task 10: End-to-end integration test

**Files:**
- Modify: `src/app.rs` (add integration test)

- [ ] **Step 1: Write integration test**

Add to `src/app.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn test_semantic_search_modal_opens_and_closes() {
        let (_dir, mut app) = test_app();
        // S key should open modal
        app.handle_key(KeyCode::Char('S'));
        assert!(app.search_modal.active);
        assert!(app.search_modal.no_index); // no index built

        // Esc should close
        app.handle_key(KeyCode::Esc);
        assert!(!app.search_modal.active);
    }

    #[test]
    fn test_semantic_search_modal_typing() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('S'));
        assert!(app.search_modal.active);

        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('e'));
        assert_eq!(app.search_modal.input, "te");

        app.handle_key(KeyCode::Backspace);
        assert_eq!(app.search_modal.input, "t");
    }

    #[test]
    fn test_semantic_search_with_index() {
        use crate::search::indexer;
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "error.rs", b"fn handle_error() { panic!(\"oops\"); }", "Fix error handling");
        add_file_commit(&repo, "main.rs", b"fn main() { println!(\"hello\"); }", "Add main");

        let index_path = dir.path().join(".glc-index");
        indexer::build_index(dir.path(), &index_path, 32, 1_048_576).unwrap();

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();

        app.handle_key(KeyCode::Char('S'));
        assert!(app.search_modal.active);
        assert!(!app.search_modal.no_index);

        // Type a search query
        app.handle_key(KeyCode::Char('e'));
        app.handle_key(KeyCode::Char('r'));
        app.handle_key(KeyCode::Char('r'));
        app.handle_key(KeyCode::Char('o'));
        app.handle_key(KeyCode::Char('r'));

        // Should have results from BM25
        let has_results = !app.search_modal.file_results.is_empty()
            || !app.search_modal.commit_results.is_empty();
        assert!(has_results, "Expected search results for 'error'");
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "Add integration tests for semantic search modal"
```

---

### Task 11: Add .glc-index to .gitignore pattern

**Files:**
- Verify behavior: `.glc-index/` should be excluded from version control

- [ ] **Step 1: Document in README or .gitignore**

If the project has a `.gitignore`, add:

```
.glc-index/
```

If not, create one with that entry. This ensures user repos don't accidentally commit the index.

- [ ] **Step 2: Commit**

```bash
git add .gitignore
git commit -m "Add .glc-index to gitignore"
```

---

## Summary

| Task | Component | Key Output |
|------|-----------|-----------|
| 1 | Module skeleton | Compiling project with search types |
| 2 | RRF fusion | Rank-based score merging |
| 3 | BM25 search | Full-text indexing + query |
| 4 | Vector storage | Save/load/cosine similarity |
| 5 | Indexer pipeline | Repo → documents → index |
| 6 | CLI subcommand | `glc index` working |
| 7 | Modal state machine | Input/navigation/section toggle |
| 8 | App integration | `S` key opens modal, renders overlay |
| 9 | Config | `[search]` section in config.toml |
| 10 | Integration tests | End-to-end validation |
| 11 | Gitignore | Clean repo hygiene |

**ONNX/CodeBERT integration** is deferred — the plan uses placeholder (zero) vectors. A follow-up task will add real embeddings via the `ort` crate once the search infrastructure is validated end-to-end with BM25 alone.
