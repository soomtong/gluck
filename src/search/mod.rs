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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DocKind {
    Commit,
    File,
    Symbol,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocMeta {
    pub doc_id: u64,
    pub kind: DocKind,
    pub title: String,
    pub commit_oid: String,
    pub path: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: f32,
    pub meta: DocMeta,
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("index not found at {0}")]
    IndexNotFound(PathBuf),
    #[error("index version mismatch: expected {expected}, got {found}")]
    VersionMismatch { expected: u32, found: u32 },
    #[error("index is stale: HEAD moved to {current_oid}")]
    StaleIndex { current_oid: String },
    #[error("tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("toml error: {0}")]
    Toml(String),
}

impl From<toml::de::Error> for SearchError {
    fn from(e: toml::de::Error) -> Self {
        Self::Toml(e.to_string())
    }
}

impl From<toml::ser::Error> for SearchError {
    fn from(e: toml::ser::Error) -> Self {
        Self::Toml(e.to_string())
    }
}

pub struct SearchEngine {
    pub bm25: bm25::Bm25Index,
    pub vector: vector::VectorIndex,
    pub embedding: embedding::EmbeddingModel,
    pub doc_store: HashMap<u64, DocMeta>,
    pub index_dir: PathBuf,
}

impl SearchEngine {
    pub fn open(index_dir: &Path) -> Result<Self, SearchError> {
        let meta_path = index_dir.join("meta.toml");
        if !meta_path.exists() {
            return Err(SearchError::IndexNotFound(index_dir.to_path_buf()));
        }
        let meta_str = std::fs::read_to_string(&meta_path)?;
        let meta: IndexMeta = toml::from_str(&meta_str)?;
        if meta.version != INDEX_VERSION {
            return Err(SearchError::VersionMismatch {
                expected: INDEX_VERSION,
                found: meta.version,
            });
        }

        let bm25 = bm25::Bm25Index::open(index_dir.join("bm25"))?;
        let vector = vector::VectorIndex::load(index_dir.join("vectors").join("index.tvim"))?;
        let embedding = embedding::EmbeddingModel::load()?;
        let doc_store = bm25.scan_doc_store()?;

        Ok(Self {
            bm25,
            vector,
            embedding,
            doc_store,
            index_dir: index_dir.to_path_buf(),
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, SearchError> {
        let bm25_hits = self.bm25.search(query, limit * 2)?;
        let query_vec = self
            .embedding
            .encode_single(query)
            .map_err(|e| SearchError::Embedding(e.to_string()))?;
        let vec_hits = self.vector.search(&query_vec, limit * 2);
        let fused = rrf::rrf_fuse(&bm25_hits, &vec_hits, 60.0, limit);
        Ok(self.hydrate(fused))
    }

    fn hydrate(&self, hits: Vec<(u64, f32)>) -> Vec<SearchResult> {
        hits.into_iter()
            .filter_map(|(doc_id, score)| {
                self.doc_store.get(&doc_id).map(|meta| SearchResult {
                    score,
                    meta: meta.clone(),
                })
            })
            .collect()
    }
}

pub const INDEX_VERSION: u32 = 3;
pub const INDEX_DIR_NAME: &str = ".glc-index";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IndexMeta {
    pub version: u32,
    pub head_oid: String,
    pub doc_count: u64,
    pub indexed_at: String,
    pub embedding: EmbeddingMeta,
    pub bm25: Bm25Meta,
    pub vector: VectorMeta,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingMeta {
    pub model: String,
    pub dim: usize,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Bm25Meta {
    pub tokenizer: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VectorMeta {
    pub backend: String,
}
