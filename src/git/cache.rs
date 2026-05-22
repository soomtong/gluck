use std::collections::{HashMap, VecDeque};

use git2::Oid;

use crate::git::commit::CommitInfo;
use crate::git::diff::{compute_diff, DiffResult};
use crate::git::repo::{GitError, GitRepo};
use crate::git::tree::{list_tree, FileEntry};

pub struct DiffCache {
    entries: HashMap<(Oid, Oid), DiffResult>,
    order: VecDeque<(Oid, Oid)>,
    max_size: usize,
}

impl DiffCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            max_size,
        }
    }

    pub fn get_or_compute(
        &mut self,
        repo: &GitRepo,
        parent: &CommitInfo,
        commit: &CommitInfo,
    ) -> Result<&DiffResult, GitError> {
        todo!()
    }
}

pub struct TreeCache {
    entries: HashMap<Oid, Vec<FileEntry>>,
    order: VecDeque<Oid>,
    max_size: usize,
}

impl TreeCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            max_size,
        }
    }

    pub fn get_or_compute(
        &mut self,
        repo: &GitRepo,
        commit: &CommitInfo,
    ) -> Result<&Vec<FileEntry>, GitError> {
        todo!()
    }
}
