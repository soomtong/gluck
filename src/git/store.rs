use std::collections::BTreeMap;
use std::sync::Arc;

use crate::git::commit::CommitInfo;
use crate::git::repo::{GitError, GitRepo};

pub struct CommitIndex {
    prefixes: BTreeMap<String, Vec<usize>>,
}

impl CommitIndex {
    pub fn new() -> Self {
        Self {
            prefixes: BTreeMap::new(),
        }
    }

    pub fn build(commits: &[CommitInfo]) -> Self {
        todo!()
    }

    pub fn search(&self, query: &str) -> Vec<usize> {
        todo!()
    }

    pub fn append(&mut self, start_idx: usize, commits: &[CommitInfo]) {
        todo!()
    }
}

pub struct CommitStore {
    pub loaded: Arc<Vec<CommitInfo>>,
    inner: Vec<CommitInfo>,
    pub index: CommitIndex,
    pub exhausted: bool,
    batch_size: usize,
}

impl CommitStore {
    pub fn new(repo: &GitRepo, batch_size: usize) -> Result<Self, GitError> {
        todo!()
    }

    pub fn load_batch(&mut self, repo: &GitRepo) -> Result<usize, GitError> {
        todo!()
    }

    pub fn search(&self, query: &str) -> Vec<usize> {
        self.index.search(query)
    }

    pub fn total_loaded(&self) -> usize {
        self.inner.len()
    }
}
