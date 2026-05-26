//! `glc report` 서브명령 구현. 검색 품질·성능 리포트 생성.

pub mod fixtures;
pub mod metrics;
pub mod perf;
pub mod render;

use std::path::PathBuf;

use thiserror::Error;

use crate::search::SearchError;
use crate::search::report::metrics::{AggregateEval, QueryEval};
use crate::search::report::perf::LatencyStats;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("fixtures file not found: {0}")]
    FixturesMissing(PathBuf),
    #[error("no queries in fixtures")]
    EmptyFixtures,
    #[error("query #{0} has empty `expected` array")]
    EmptyExpected(usize),
    #[error("search engine error: {0}")]
    Search(#[from] SearchError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Toml(String),
}

impl From<toml::de::Error> for ReportError {
    fn from(e: toml::de::Error) -> Self {
        Self::Toml(e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub index_dir: PathBuf,
    pub size_bytes: u64,
    pub doc_count_total: usize,
    pub doc_count_commit: usize,
    pub doc_count_file: usize,
    pub doc_count_symbol: usize,
    pub head_oid: String,
    pub indexed_at: String,
    pub embedding_model: String,
    pub embedding_dim: usize,
    pub bm25_tokenizer: String,
    pub vector_backend: String,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub generated_at: String,
    pub working_head_oid: String,
    pub head_mismatch: bool,
    pub warmup: usize,
    pub iters: usize,
    pub limit: usize,
    pub aggregate: AggregateEval,
    pub latency: LatencyStats,
    pub index: IndexStats,
    pub per_query: Vec<QueryEval>,
}
