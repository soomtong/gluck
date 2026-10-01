pub mod bm25;
pub mod chunk;
pub mod diagnose;
pub mod diff;
pub mod embedding;
pub mod glcignore;
pub mod indexer;
pub mod modal_state;
pub mod params;
pub mod report;
pub mod rrf;
pub mod silence;
pub mod text_prep;
pub mod typo;
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

/// Results plus a low-confidence flag for queries the index likely can't
/// answer (see `SearchEngine::search_scored`).
#[derive(Debug, Clone, Default)]
pub struct SearchOutcome {
    pub results: Vec<SearchResult>,
    /// No query word matches a title or path term, and the vector top-1 barely
    /// stands out from the top 10. Results are kept; the UI only labels them.
    pub weak: bool,
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
    #[error("git error: {0}")]
    Git(String),
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
    /// title + path_terms word vocabulary for typo correction.
    pub vocab: HashMap<String, u32>,
    pub index_dir: PathBuf,
    pub params: params::SearchParams,
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

/// `path`와 정확히 일치하는 문서 id 목록 (id 오름차순).
fn path_doc_ids(doc_store: &HashMap<u64, DocMeta>, path: &str) -> Vec<u64> {
    let mut ids: Vec<u64> = doc_store
        .values()
        .filter(|m| m.path.as_deref() == Some(path))
        .map(|m| m.doc_id)
        .collect();
    ids.sort_unstable();
    ids
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
        let vocab = bm25.word_vocab()?;

        Ok(Self {
            bm25,
            vector,
            embedding,
            doc_store,
            vocab,
            index_dir: index_dir.to_path_buf(),
            params: params::SearchParams::default(),
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, SearchError> {
        Ok(self.search_scored(query, limit)?.results)
    }

    /// `search` plus the `weak` flag. Neither signal alone separates negative
    /// queries: BM25 always finds some bigram overlap, and absolute cosine or
    /// BM25 scores vary per corpus. A missing word-level match combined with a
    /// flat vector top-10 (`top1 - mean < weak_gap`) is scale-free.
    pub fn search_scored(&self, query: &str, limit: usize) -> Result<SearchOutcome, SearchError> {
        let (path_filter, semantic_query) = extract_path_filter(query);

        // BM25는 path:"..." 문법을 QueryParser가 그대로 처리하므로 원본 쿼리 전달
        // path 필터가 있으면 후처리 필터링을 위해 후보를 더 많이 가져옴
        let candidate_limit = if path_filter.is_some() {
            (limit * 4).max(16)
        } else {
            limit * 2
        };
        let korean = text_prep::is_korean_query(query);
        let corrections = if korean || self.params.typo_mode == 0 {
            Vec::new()
        } else {
            typo::correct(&typo::query_words(&semantic_query), &self.vocab)
        };
        let bm25_query = typo::append_corrections(query, &corrections);
        let bm25_hits = if korean {
            self.bm25.search_path_title_only(query, candidate_limit)?
        } else {
            self.bm25.search(&bm25_query, candidate_limit)?
        };
        let any_bm25_hit = !bm25_hits.is_empty();
        // Vector top-1 minus top-10 mean; only measured on unfiltered queries.
        let mut vec_gap: Option<f32> = None;

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
        let embed_text = match self.params.typo_mode {
            _ if corrections.is_empty() => embed_text.to_string(),
            2 => typo::append_corrections(embed_text, &corrections),
            3 => typo::replace_corrections(embed_text, &corrections),
            _ => embed_text.to_string(),
        };
        let embed_text = embed_text.as_str();

        let fused = if embed_text.is_empty() {
            // 벡터 검색을 건너뛰고 BM25 결과만 사용
            bm25_hits
        } else {
            let query_vec = self
                .embedding
                .encode_single(embed_text)
                .map_err(|e| SearchError::Embedding(e.to_string()))?;
            // 벡터 후보를 limit*5 (min 50)로 늘려봤지만 하위권 노이즈가 RRF에 섞여
            // R@5/negative가 나빠졌다 (2026-10-01). BM25와 같은 깊이를 유지한다.
            let vec_k = candidate_limit;
            let p = &self.params;
            // commit penalty가 있으면 밀려난 자리를 채울 여유분을 더 가져온다.
            let fetch_k = if p.vec_commit_penalty > 0.0 {
                vec_k * 3
            } else {
                vec_k
            };
            let raw_vec_hits = match &path_filter {
                // path 필터는 해당 경로 문서만 대상으로 검색해야 후보 밖 누락이 없다.
                Some(path) => {
                    let allow = path_doc_ids(&self.doc_store, path);
                    self.vector.search_allowlist(&query_vec, fetch_k, &allow)
                }
                None => self.vector.search(&query_vec, fetch_k),
            };
            if path_filter.is_none() {
                let top = &raw_vec_hits[..raw_vec_hits.len().min(10)];
                if let Some(&(_, top1)) = top.first() {
                    let mean = top.iter().map(|h| h.1).sum::<f32>() / top.len() as f32;
                    vec_gap = Some(top1 - mean);
                }
            }
            let vec_hits = self.adjust_vec_hits(raw_vec_hits, vec_k);
            // 한국어 쿼리는 path 별칭으로 BM25가 정답을 잡지만 vector가 commit
            // 노이즈에 끌려가므로, BM25 top 3을 anchor해서 단일 강한 매칭을 보존한다.
            // 그 뒤는 weighted RRF로 채운다 (vec 1.5x — paraphrase 케이스 보호).
            if text_prep::is_korean_query(embed_text) {
                rrf::rrf_fuse_with_bm25_anchor(
                    &bm25_hits,
                    &vec_hits,
                    p.rrf_k,
                    candidate_limit,
                    1.0,
                    p.w_vec_korean,
                    p.korean_anchor,
                )
            } else {
                rrf::rrf_fuse_weighted(
                    &bm25_hits,
                    &vec_hits,
                    p.rrf_k,
                    candidate_limit,
                    1.0,
                    p.w_vec,
                )
            }
        };

        let weak = match vec_gap {
            Some(gap) if gap < self.params.weak_gap => {
                // Korean BM25 already searched title + path_terms only.
                let word_hits = if korean {
                    any_bm25_hit
                } else {
                    !self.bm25.search_path_title_only(&bm25_query, 1)?.is_empty()
                };
                !word_hits
            }
            _ => false,
        };

        let hydrated = self.hydrate(fused);

        let results = if let Some(path) = path_filter {
            apply_path_filter(hydrated, &path, limit)
        } else {
            hydrated.into_iter().take(limit).collect()
        };

        Ok(SearchOutcome { results, weak })
    }

    /// `vec_min_score` 미만 제거, Commit 문서 점수에 `vec_commit_penalty` 적용 후
    /// 재정렬해서 상위 `k`개만 남긴다. 기본 파라미터에서는 입력 순서 그대로다.
    fn adjust_vec_hits(&self, hits: Vec<(u64, f32)>, k: usize) -> Vec<(u64, f32)> {
        let p = &self.params;
        let mut out: Vec<(u64, f32)> = hits
            .into_iter()
            .filter(|(_, score)| *score >= p.vec_min_score)
            .map(|(id, score)| {
                let is_commit = self
                    .doc_store
                    .get(&id)
                    .is_some_and(|m| m.kind == DocKind::Commit);
                if is_commit {
                    (id, score - p.vec_commit_penalty)
                } else {
                    (id, score)
                }
            })
            .collect();
        if p.vec_commit_penalty != 0.0 {
            // stable sort: 동점은 원래 vector 순서 유지
            out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }
        out.truncate(k);
        out
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

// 8: turbovec 1.0 (v7 file format, explicit TQ+ calibration)
// 9: changed paths in commit embed text
// 10: BM25 body_words field (typo-correction vocabulary)
// 11: BM25 title_raw stored field (doc_store titles were camelCase-split)
pub const INDEX_VERSION: u32 = 11;
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
            ignore_hash: None,
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
    fn path_doc_ids_matches_exact_path_sorted() {
        let store: HashMap<u64, DocMeta> = [
            sr(5, 0.0, "src/a.rs"),
            sr(2, 0.0, "src/a.rs"),
            sr(3, 0.0, "src/a.rs.bak"),
            sr(4, 0.0, "src/b.rs"),
        ]
        .into_iter()
        .map(|r| (r.meta.doc_id, r.meta))
        .collect();
        assert_eq!(path_doc_ids(&store, "src/a.rs"), vec![2, 5]);
        assert!(path_doc_ids(&store, "src/none.rs").is_empty());
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexMeta {
    pub version: u32,
    pub head_oid: String,
    pub doc_count: u64,
    pub indexed_at: String,
    pub embedding: EmbeddingMeta,
    pub bm25: Bm25Meta,
    pub vector: VectorMeta,
    /// Blob oid of `.glcignore` at build time; a mismatch forces a full rebuild.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_hash: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingMeta {
    pub model: String,
    pub dim: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bm25Meta {
    pub tokenizer: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorMeta {
    pub backend: String,
}
