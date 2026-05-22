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
        let mut index = Self::new();
        for (i, c) in commits.iter().enumerate() {
            index.insert(i, c);
        }
        index
    }

    pub fn append(&mut self, start_idx: usize, commits: &[CommitInfo]) {
        for (i, c) in commits.iter().enumerate() {
            self.insert(start_idx + i, c);
        }
    }

    fn insert(&mut self, idx: usize, commit: &CommitInfo) {
        self.index_tokens(idx, &commit.message.to_lowercase());
        self.index_tokens(idx, &commit.author.to_lowercase());
        self.index_tokens(idx, &commit.short_id.to_lowercase());
    }

    fn index_tokens(&mut self, idx: usize, text: &str) {
        for word in text.split_whitespace() {
            for end in 1..=word.len() {
                self.prefixes
                    .entry(word[..end].into())
                    .or_default()
                    .push(idx);
            }
        }
    }

    pub fn search(&self, query: &str) -> Vec<usize> {
        let q = query.to_lowercase().trim().to_string();
        if q.is_empty() {
            return vec![];
        }
        let Some(indices) = self.prefixes.get(&q) else {
            return vec![];
        };
        let mut result = indices.clone();
        result.sort_unstable();
        result.dedup();
        result
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::CommitInfo;
    use git2::Oid;

    fn make_commit(msg: &str, author: &str) -> CommitInfo {
        CommitInfo {
            id: Oid::zero(),
            short_id: "abc1234".into(),
            author: author.into(),
            date: std::time::UNIX_EPOCH,
            message: msg.into(),
        }
    }

    #[test]
    fn test_index_search_by_message() {
        let commits = vec![
            make_commit("Fix login bug", "Alice"),
            make_commit("Add auth module", "Bob"),
        ];
        let index = CommitIndex::build(&commits);
        assert_eq!(index.search("auth"), vec![1]);
        assert_eq!(index.search("fix"), vec![0]);
    }

    #[test]
    fn test_index_case_insensitive() {
        let index = CommitIndex::build(&[make_commit("Hello World", "Alice")]);
        assert_eq!(index.search("HELLO"), vec![0]);
    }

    #[test]
    fn test_index_by_author() {
        let index = CommitIndex::build(&[
            make_commit("A", "Alice"),
            make_commit("B", "Bob"),
        ]);
        assert_eq!(index.search("bob"), vec![1]);
    }

    #[test]
    fn test_index_by_short_id() {
        let mut c = make_commit("msg", "A");
        c.short_id = "1a2b3c4".into();
        let index = CommitIndex::build(&[c]);
        assert_eq!(index.search("1a2b"), vec![0]);
    }

    #[test]
    fn test_index_no_match() {
        let index = CommitIndex::build(&[make_commit("Hello", "A")]);
        assert!(index.search("zzz").is_empty());
    }

    #[test]
    fn test_index_empty_query() {
        let index = CommitIndex::build(&[
            make_commit("Hello", "A"),
            make_commit("World", "B"),
        ]);
        assert!(index.search("").is_empty());
    }

    #[test]
    fn test_index_multiple_matches() {
        let index = CommitIndex::build(&[
            make_commit("Add login", "Bob"),
            make_commit("Fix login bug", "Alice"),
        ]);
        assert_eq!(index.search("login").len(), 2);
    }
}
