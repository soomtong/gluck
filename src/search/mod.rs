pub mod bm25;
pub mod chunk;
pub mod embedding;
pub mod indexer;
pub mod modal;
pub mod rrf;
pub mod vector;

use std::collections::HashMap;
use std::path::PathBuf;

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
