use std::collections::BTreeMap;
use std::sync::Arc;

use crate::git::commit::CommitInfo;
use crate::git::repo::{GitError, GitRepo};

pub struct CommitIndex {
    prefixes: BTreeMap<String, Vec<usize>>,
}

impl Default for CommitIndex {
    fn default() -> Self {
        Self::new()
    }
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
            for (end, _) in word.char_indices().skip(1) {
                self.prefixes
                    .entry(word[..end].into())
                    .or_default()
                    .push(idx);
            }
            self.prefixes.entry(word.into()).or_default().push(idx);
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
        let mut store = Self {
            loaded: Arc::new(vec![]),
            inner: Vec::new(),
            index: CommitIndex::new(),
            exhausted: false,
            batch_size,
        };
        store.load_batch(repo)?;
        Ok(store)
    }

    pub fn load_batch(&mut self, repo: &GitRepo) -> Result<usize, GitError> {
        if self.exhausted {
            return Ok(0);
        }
        let repository = repo.repository();
        let mut revwalk = repository.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

        let skip = self.inner.len();
        for _ in 0..skip {
            if revwalk.next().is_none() {
                self.exhausted = true;
                return Ok(0);
            }
        }

        let start_idx = self.inner.len();
        let mut count = 0;
        for _ in 0..self.batch_size {
            match revwalk.next() {
                Some(Ok(oid)) => {
                    self.inner
                        .push(CommitInfo::from_git_commit(&repository.find_commit(oid)?));
                    count += 1;
                }
                _ => {
                    self.exhausted = true;
                    break;
                }
            }
        }
        if count > 0 {
            let new_commits = &self.inner[start_idx..];
            self.index.append(start_idx, new_commits);
        }
        self.loaded = Arc::new(self.inner.clone());
        Ok(count)
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
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;
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

    // ── CommitIndex tests ──

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
        let index = CommitIndex::build(&[make_commit("A", "Alice"), make_commit("B", "Bob")]);
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
        let index = CommitIndex::build(&[make_commit("Hello", "A"), make_commit("World", "B")]);
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

    // ── CommitStore tests ──

    #[test]
    fn test_store_loads_initial_batch() {
        let (dir, repo) = init_test_repo();
        for i in 0..10 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let store = CommitStore::new(&git_repo, 5).unwrap();
        assert_eq!(store.loaded.len(), 5);
        assert!(!store.exhausted);
    }

    #[test]
    fn test_store_paging_loads_more() {
        let (dir, repo) = init_test_repo();
        for i in 0..10 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut store = CommitStore::new(&git_repo, 5).unwrap();
        assert_eq!(store.loaded.len(), 5);

        let added = store.load_batch(&git_repo).unwrap();
        assert_eq!(added, 5);
        assert_eq!(store.loaded.len(), 10);
        // Next load should return 0 and mark exhausted
        assert_eq!(store.load_batch(&git_repo).unwrap(), 0);
        assert!(store.exhausted);
    }

    #[test]
    fn test_store_exhausted_returns_zero() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "f.txt", b"x", "only");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut store = CommitStore::new(&git_repo, 5).unwrap();
        assert!(store.exhausted);
        assert_eq!(store.load_batch(&git_repo).unwrap(), 0);
    }

    #[test]
    fn test_store_arc_shares_data() {
        let (dir, repo) = init_test_repo();
        for i in 0..3 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let store = CommitStore::new(&git_repo, 3).unwrap();
        let a1 = store.loaded.clone();
        let a2 = store.loaded.clone();
        assert!(Arc::strong_count(&store.loaded) >= 3);
        assert_eq!(a1.len(), 3);
        assert_eq!(a2.len(), 3);
    }

    #[test]
    fn test_store_search_after_paging() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "z.txt", b"z", "Sphinx of black quartz");
        for i in 0..10 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut store = CommitStore::new(&git_repo, 3).unwrap();

        let found_fast = !store.search("sphinx").is_empty();
        while !store.exhausted {
            store.load_batch(&git_repo).unwrap();
            if !store.search("sphinx").is_empty() {
                break;
            }
        }
        let found_later = !store.search("sphinx").is_empty();
        assert!(found_fast || found_later);
        assert_eq!(store.search("sphinx").len(), 1);
    }
}
