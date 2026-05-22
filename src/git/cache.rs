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
        let key = (parent.id, commit.id);
        if self.entries.contains_key(&key) {
            self.touch(&key);
            return Ok(&self.entries[&key]);
        }
        let result = compute_diff(repo, parent, commit)?;
        self.insert(key, result);
        Ok(&self.entries[&key])
    }

    fn insert(&mut self, key: (Oid, Oid), result: DiffResult) {
        if self.entries.len() >= self.max_size {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, result);
        self.order.push_back(key);
    }

    fn touch(&mut self, key: &(Oid, Oid)) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push_back(*key);
        }
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
        let key = commit.id;
        if self.entries.contains_key(&key) {
            self.touch(&key);
            return Ok(&self.entries[&key]);
        }
        let tree = list_tree(repo, commit)?;
        self.insert(key, tree);
        Ok(&self.entries[&key])
    }

    fn insert(&mut self, key: Oid, entries: Vec<FileEntry>) {
        if self.entries.len() >= self.max_size {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, entries);
        self.order.push_back(key);
    }

    fn touch(&mut self, key: &Oid) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push_back(*key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::list_commits;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;

    // ── DiffCache tests ──

    #[test]
    fn test_diff_cache_hit() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First");
        add_file_commit(&repo, "a.txt", b"second", "Second");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(10);
        cache
            .get_or_compute(&git_repo, &commits[1], &commits[0])
            .unwrap();
        cache
            .get_or_compute(&git_repo, &commits[1], &commits[0])
            .unwrap();
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn test_diff_cache_lru_eviction() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 {
            add_file_commit(
                &repo,
                &format!("f{}.txt", i),
                b"x",
                &format!("c{}", i),
            );
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(5);

        for i in 0..5 {
            cache
                .get_or_compute(&git_repo, &commits[i + 1], &commits[i])
                .unwrap();
        }
        assert_eq!(cache.entries.len(), 5);

        cache
            .get_or_compute(&git_repo, &commits[6], &commits[5])
            .unwrap();
        assert_eq!(cache.entries.len(), 5);
    }

    #[test]
    fn test_diff_cache_lru_refreshes_on_hit() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 {
            add_file_commit(
                &repo,
                &format!("f{}.txt", i),
                b"x",
                &format!("c{}", i),
            );
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(5);

        for i in 0..5 {
            cache
                .get_or_compute(&git_repo, &commits[i + 1], &commits[i])
                .unwrap();
        }
        // Refresh first entry
        cache
            .get_or_compute(&git_repo, &commits[1], &commits[0])
            .unwrap();
        // Add new entry — should evict second (oldest unaccessed)
        cache
            .get_or_compute(&git_repo, &commits[6], &commits[5])
            .unwrap();
        assert!(cache.entries.contains_key(&(commits[1].id, commits[0].id)));
    }

    // ── TreeCache tests ──

    #[test]
    fn test_tree_cache_hit() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Initial");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();

        let mut cache = TreeCache::new(10);
        let len1 = cache
            .get_or_compute(&git_repo, &commits[0])
            .unwrap()
            .len();
        let len2 = cache
            .get_or_compute(&git_repo, &commits[0])
            .unwrap()
            .len();
        assert_eq!(len1, len2);
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn test_tree_cache_lru_eviction() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 {
            add_file_commit(
                &repo,
                &format!("f{}.txt", i),
                b"x",
                &format!("c{}", i),
            );
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = TreeCache::new(5);

        for i in 0..5 {
            cache.get_or_compute(&git_repo, &commits[i]).unwrap();
        }
        assert_eq!(cache.entries.len(), 5);

        cache.get_or_compute(&git_repo, &commits[5]).unwrap();
        assert_eq!(cache.entries.len(), 5);
    }
}
