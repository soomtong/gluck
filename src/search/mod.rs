pub mod bm25;
pub mod chunk;
pub mod diff;
pub mod embedding;
pub mod indexer;
pub mod modal_state;
pub mod rrf;
pub mod silence;
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
    #[error(
        "BM25 tokenizer mismatch: expected '{expected}', found '{found}' — run `glc index --force`"
    )]
    IncompatibleTokenizer { expected: String, found: String },
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

/// `path:"..."` 절을 쿼리에서 분리한다.
/// 반환: (필터값 Option, 나머지 쿼리)
fn extract_path_filter(query: &str) -> (Option<String>, String) {
    const PREFIX: &str = "path:\"";
    let Some(start) = query.find(PREFIX) else {
        return (None, query.to_string());
    };
    let after = &query[start + PREFIX.len()..];
    let Some(end_q) = after.find('"') else {
        return (None, query.to_string());
    };
    let path = after[..end_q].to_string();
    let before = query[..start].trim_end();
    let rest = after[end_q + 1..].trim_start();
    let remaining = match (before.is_empty(), rest.is_empty()) {
        (true, true) => String::new(),
        (true, false) => rest.to_string(),
        (false, true) => before.to_string(),
        (false, false) => format!("{} {}", before, rest),
    };
    (Some(path), remaining)
}

/// 결과 목록에서 path가 일치하는 항목만 limit개 유지. 상대 순서는 보존.
fn apply_path_filter(hits: Vec<SearchResult>, path: &str, limit: usize) -> Vec<SearchResult> {
    hits.into_iter()
        .filter(|r| r.meta.path.as_deref() == Some(path))
        .take(limit)
        .collect()
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
        if meta.bm25.tokenizer != bm25::TOKENIZER {
            return Err(SearchError::IncompatibleTokenizer {
                expected: bm25::TOKENIZER.to_string(),
                found: meta.bm25.tokenizer.clone(),
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
        let (path_filter, semantic_query) = extract_path_filter(query);

        // BM25는 path:"..." 문법을 QueryParser가 그대로 처리하므로 원본 쿼리 전달
        // path 필터가 있으면 후처리 필터링을 위해 후보를 더 많이 가져옴
        let candidate_limit = if path_filter.is_some() {
            (limit * 4).max(16)
        } else {
            limit * 2
        };
        let bm25_hits = self.bm25.search(query, candidate_limit)?;

        // 벡터 검색은 필드 문법을 모르므로 path:"..."가 제거된 의미 부분으로 임베딩
        let embed_text = if path_filter.is_some() {
            if semantic_query.is_empty() {
                // path:만 있는 쿼리 — 벡터 검색 생략
                ""
            } else {
                semantic_query.as_str()
            }
        } else {
            query
        };

        let fused = if embed_text.is_empty() {
            // 벡터 검색을 건너뛰고 BM25 결과만 사용
            bm25_hits
        } else {
            let query_vec = self
                .embedding
                .encode_single(embed_text)
                .map_err(|e| SearchError::Embedding(e.to_string()))?;
            let vec_hits = self.vector.search(&query_vec, candidate_limit);
            rrf::rrf_fuse(&bm25_hits, &vec_hits, 60.0, candidate_limit)
        };

        let hydrated = self.hydrate(fused);

        let result = if let Some(path) = path_filter {
            apply_path_filter(hydrated, &path, limit)
        } else {
            hydrated.into_iter().take(limit).collect()
        };

        Ok(result)
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

pub const INDEX_VERSION: u32 = 5;
pub const INDEX_DIR_NAME: &str = ".glc-index";

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_meta(dir: &TempDir, tokenizer: &str) {
        let meta = IndexMeta {
            version: INDEX_VERSION,
            head_oid: "0".repeat(40),
            doc_count: 0,
            indexed_at: "0Z".to_string(),
            embedding: EmbeddingMeta {
                model: "test".to_string(),
                dim: 256,
            },
            bm25: Bm25Meta {
                tokenizer: tokenizer.to_string(),
            },
            vector: VectorMeta {
                backend: "test".to_string(),
            },
        };
        let s = toml::to_string_pretty(&meta).unwrap();
        std::fs::write(dir.path().join("meta.toml"), s).unwrap();
    }

    fn meta_with_path(doc_id: u64, path: &str) -> DocMeta {
        DocMeta {
            doc_id,
            kind: DocKind::File,
            title: path.to_string(),
            commit_oid: format!("{:040x}", doc_id),
            path: Some(path.to_string()),
            line_start: None,
            line_end: None,
        }
    }

    fn sr(doc_id: u64, score: f32, path: &str) -> SearchResult {
        SearchResult {
            score,
            meta: meta_with_path(doc_id, path),
        }
    }

    #[test]
    fn extract_path_filter_quoted() {
        assert_eq!(
            extract_path_filter("path:\"src/search/error.rs\""),
            (Some("src/search/error.rs".to_string()), String::new())
        );
    }

    #[test]
    fn extract_path_filter_with_trailing_terms() {
        assert_eq!(
            extract_path_filter("path:\"src/search/error.rs\" 에러 처리"),
            (
                Some("src/search/error.rs".to_string()),
                "에러 처리".to_string()
            )
        );
    }

    #[test]
    fn extract_path_filter_with_leading_terms() {
        assert_eq!(
            extract_path_filter("에러 path:\"src/foo.rs\""),
            (Some("src/foo.rs".to_string()), "에러".to_string())
        );
    }

    #[test]
    fn extract_path_filter_absent() {
        assert_eq!(
            extract_path_filter("에러 처리"),
            (None, "에러 처리".to_string())
        );
    }

    #[test]
    fn apply_path_filter_keeps_only_matching_path() {
        let hits = vec![
            sr(1, 0.9, "src/search/error.rs"),
            sr(2, 0.8, "src/ui/view.rs"),
            sr(3, 0.7, "src/search/error.rs"),
        ];
        let filtered = apply_path_filter(hits, "src/search/error.rs", 10);
        assert_eq!(filtered.len(), 2);
        assert!(filtered
            .iter()
            .all(|r| r.meta.path.as_deref() == Some("src/search/error.rs")));
        // 정렬 순서(상대 점수 순) 유지
        assert_eq!(filtered[0].meta.doc_id, 1);
        assert_eq!(filtered[1].meta.doc_id, 3);
    }

    #[test]
    fn apply_path_filter_respects_limit() {
        let hits = vec![
            sr(1, 0.9, "src/a.rs"),
            sr(2, 0.8, "src/a.rs"),
            sr(3, 0.7, "src/a.rs"),
        ];
        let filtered = apply_path_filter(hits, "src/a.rs", 2);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn open_fails_on_tokenizer_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        write_meta(&dir, "stale_tokenizer");
        match SearchEngine::open(dir.path()) {
            Err(SearchError::IncompatibleTokenizer { expected, found }) => {
                assert_eq!(expected, bm25::TOKENIZER);
                assert_eq!(found, "stale_tokenizer");
            }
            other => panic!("expected IncompatibleTokenizer, got {:?}", other.err()),
        }
    }
}

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
