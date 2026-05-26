//! `glc report` 서브명령 구현. 검색 품질·성능 리포트 생성.

pub mod fixtures;
pub mod metrics;
pub mod perf;
pub mod render;

use std::path::PathBuf;

use thiserror::Error;

use crate::search::SearchError;

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
