//! `glc report` 서브명령 구현. 검색 품질·성능 리포트 생성.

pub mod fixtures;
pub mod metrics;
pub mod perf;
pub mod render;

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::git::repo::GitRepo;
use crate::search::indexer::index_dir_for;
use crate::search::report::metrics::{aggregate, evaluate, AggregateEval, QueryEval};
use crate::search::report::perf::{run_perf, LatencyStats};
use crate::search::report::render::{to_markdown_string, to_stdout};
use crate::search::{DocKind, IndexMeta, SearchEngine, SearchError};

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

pub struct ReportOptions {
    pub fixtures_path: PathBuf,
    pub out_markdown: Option<PathBuf>,
    pub warmup: usize,
    pub iters: usize,
    pub limit: usize,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            fixtures_path: PathBuf::from("tests/fixtures/search_queries.toml"),
            out_markdown: None,
            warmup: 3,
            iters: 10,
            limit: 10,
        }
    }
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn working_head_oid(repo: &GitRepo) -> Result<String, ReportError> {
    let r = repo.repository();
    let head = r
        .head()
        .map_err(|e| ReportError::Io(std::io::Error::other(e.to_string())))?;
    let oid = head
        .peel_to_commit()
        .map_err(|e| ReportError::Io(std::io::Error::other(e.to_string())))?
        .id();
    Ok(oid.to_string())
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(read) = std::fs::read_dir(p) else {
        return 0;
    };
    for entry in read.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            total += dir_size(&entry.path());
        } else if let Ok(md) = entry.metadata() {
            total += md.len();
        }
    }
    total
}

pub fn run(repo: &GitRepo, repo_path: &Path, opts: &ReportOptions) -> Result<(), ReportError> {
    let set = fixtures::load(&opts.fixtures_path)?;

    let index_dir = index_dir_for(repo_path);
    if !index_dir.join("meta.toml").exists() {
        return Err(ReportError::Search(SearchError::IndexNotFound(
            index_dir.clone(),
        )));
    }
    let engine = SearchEngine::open(&index_dir)?;

    let meta_str = std::fs::read_to_string(index_dir.join("meta.toml"))?;
    let meta: IndexMeta = toml::from_str(&meta_str)?;

    let head = working_head_oid(repo)?;
    let head_mismatch = head != meta.head_oid;

    let (latency, last_results) =
        run_perf(&engine, &set.queries, opts.warmup, opts.iters, opts.limit)?;

    let per_query: Vec<_> = set
        .queries
        .iter()
        .zip(last_results.iter())
        .map(|(q, r)| evaluate(q, r))
        .collect();
    let aggregate_eval = aggregate(&per_query);

    let mut commit_n = 0usize;
    let mut file_n = 0usize;
    let mut sym_n = 0usize;
    for m in engine.doc_store.values() {
        match m.kind {
            DocKind::Commit => commit_n += 1,
            DocKind::File => file_n += 1,
            DocKind::Symbol => sym_n += 1,
        }
    }
    let index_stats = IndexStats {
        index_dir: index_dir.clone(),
        size_bytes: dir_size(&index_dir),
        doc_count_total: engine.doc_store.len(),
        doc_count_commit: commit_n,
        doc_count_file: file_n,
        doc_count_symbol: sym_n,
        head_oid: meta.head_oid.clone(),
        indexed_at: meta.indexed_at.clone(),
        embedding_model: meta.embedding.model.clone(),
        embedding_dim: meta.embedding.dim,
        bm25_tokenizer: meta.bm25.tokenizer.clone(),
        vector_backend: meta.vector.backend.clone(),
    };

    let report = Report {
        generated_at: now_iso8601(),
        working_head_oid: head,
        head_mismatch,
        warmup: opts.warmup,
        iters: opts.iters,
        limit: opts.limit,
        aggregate: aggregate_eval,
        latency,
        index: index_stats,
        per_query,
    };

    to_stdout(&report);

    if let Some(out_path) = &opts.out_markdown {
        let md = to_markdown_string(&report);
        std::fs::write(out_path, md)?;
        eprintln!("wrote markdown report to {}", out_path.display());
    }

    Ok(())
}
