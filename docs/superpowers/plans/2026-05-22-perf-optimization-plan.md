# Perf Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make glc start in <1s and respond <16ms on repos with 100k+ commits via paging, caching, and deduplication.

**Architecture:** Introduce `CommitStore` (paging commit loader, Arc-shared, prefix search index), `DiffCache` and `TreeCache` (LRU caches). Remove `App.commits` and `PickState.commits` — all consumers reference `Arc<Vec<CommitInfo>>`. Prefetch: trigger next batch load when cursor reaches `len - 50` (not at the very end).

**Tech Stack:** Rust, git2, no new external crates (LRU implemented manually with VecDeque).

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/git/store.rs` (new) | `CommitStore` — paging loader, `CommitIndex` — prefix search |
| `src/git/cache.rs` (new) | `DiffCache` — LRU diff cache, `TreeCache` — LRU tree cache |
| `src/git/mod.rs` | Add `store`, `cache` modules; re-exports |
| `src/git/commit.rs` | Remove `search_commits` (replaced by CommitIndex) |
| `src/git/repo.rs` | Add `init_test_repo_with_n_commits` helper |
| `src/mode.rs` | `PickState.commits`: `Vec<CommitInfo>` → `Arc<Vec<CommitInfo>>`; `update_filter` → use CommitStore.search() |
| `src/app.rs` | `App.commits` → `App.store`; integrate DiffCache, TreeCache; prefetch paging |

---

### Task 1: Stub modules + mod.rs

**Files:**
- Create: `src/git/store.rs`
- Create: `src/git/cache.rs`
- Modify: `src/git/mod.rs`

- [ ] **Step 1: Create `src/git/store.rs`**

```rust
use std::collections::BTreeMap;
use std::sync::Arc;
use crate::git::commit::CommitInfo;
use crate::git::repo::{GitError, GitRepo};

pub struct CommitIndex {
    prefixes: BTreeMap<String, Vec<usize>>,
}

impl CommitIndex {
    pub fn new() -> Self { Self { prefixes: BTreeMap::new() } }
    pub fn build(commits: &[CommitInfo]) -> Self { todo!() }
    pub fn search(&self, query: &str) -> Vec<usize> { todo!() }
    pub fn append(&mut self, start_idx: usize, commits: &[CommitInfo]) { todo!() }
}

pub struct CommitStore {
    pub loaded: Arc<Vec<CommitInfo>>,
    inner: Vec<CommitInfo>,
    pub index: CommitIndex,
    pub exhausted: bool,
    batch_size: usize,
}

impl CommitStore {
    pub fn new(repo: &GitRepo, batch_size: usize) -> Result<Self, GitError> { todo!() }
    pub fn load_batch(&mut self, repo: &GitRepo) -> Result<usize, GitError> { todo!() }
    pub fn search(&self, query: &str) -> Vec<usize> { self.index.search(query) }
    pub fn total_loaded(&self) -> usize { self.inner.len() }
}
```

- [ ] **Step 2: Create `src/git/cache.rs`**

```rust
use std::collections::{HashMap, VecDeque};
use git2::Oid;
use crate::git::commit::CommitInfo;
use crate::git::diff::{DiffResult, compute_diff};
use crate::git::tree::{FileEntry, list_tree};
use crate::git::repo::{GitError, GitRepo};

pub struct DiffCache {
    entries: HashMap<(Oid, Oid), DiffResult>,
    order: VecDeque<(Oid, Oid)>,
    max_size: usize,
}

impl DiffCache {
    pub fn new(max_size: usize) -> Self {
        Self { entries: HashMap::new(), order: VecDeque::new(), max_size }
    }
    pub fn get_or_compute(
        &mut self, repo: &GitRepo, parent: &CommitInfo, commit: &CommitInfo,
    ) -> Result<&DiffResult, GitError> { todo!() }
}

pub struct TreeCache {
    entries: HashMap<Oid, Vec<FileEntry>>,
    order: VecDeque<Oid>,
    max_size: usize,
}

impl TreeCache {
    pub fn new(max_size: usize) -> Self {
        Self { entries: HashMap::new(), order: VecDeque::new(), max_size }
    }
    pub fn get_or_compute(
        &mut self, repo: &GitRepo, commit: &CommitInfo,
    ) -> Result<&Vec<FileEntry>, GitError> { todo!() }
}
```

- [ ] **Step 3: Update `src/git/mod.rs`**

```rust
pub mod commit;
pub mod diff;
pub mod repo;
pub mod tree;
pub mod store;
pub mod cache;

pub use commit::CommitInfo;
pub use diff::DiffResult;
pub use repo::{GitError, GitRepo};
pub use tree::FileEntry;
pub use store::CommitStore;
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check
```

Expected: compiles (all `todo!()` in non-called paths).

- [ ] **Step 5: Commit**

```bash
git add src/git/store.rs src/git/cache.rs src/git/mod.rs
git commit -m "Add CommitStore and cache module stubs"
```

---

### Task 2: Implement CommitIndex

**Files:**
- Modify: `src/git/store.rs`

- [ ] **Step 1: Write tests at bottom of `src/git/store.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::CommitInfo;
    use git2::Oid;

    fn make_commit(msg: &str, author: &str) -> CommitInfo {
        CommitInfo {
            id: Oid::zero(), short_id: "abc1234".into(),
            author: author.into(),
            date: std::time::UNIX_EPOCH, message: msg.into(),
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
    fn test_index_search_case_insensitive() {
        let commits = vec![make_commit("Hello World", "Alice")];
        let index = CommitIndex::build(&commits);
        assert_eq!(index.search("HELLO"), vec![0]);
    }

    #[test]
    fn test_index_search_by_author() {
        let commits = vec![
            make_commit("A", "Alice"), make_commit("B", "Bob"),
        ];
        let index = CommitIndex::build(&commits);
        assert_eq!(index.search("bob"), vec![1]);
    }

    #[test]
    fn test_index_search_by_short_id() {
        let mut c = make_commit("msg", "A");
        c.short_id = "1a2b3c4".into();
        let index = CommitIndex::build(&[c]);
        assert_eq!(index.search("1a2b"), vec![0]);
    }

    #[test]
    fn test_index_search_no_match() {
        let index = CommitIndex::build(&[make_commit("Hello", "A")]);
        assert!(index.search("zzz").is_empty());
    }

    #[test]
    fn test_index_search_empty_query() {
        let index = CommitIndex::build(&[
            make_commit("Hello", "A"), make_commit("World", "B"),
        ]);
        assert!(index.search("").is_empty());
    }

    #[test]
    fn test_index_search_multiple_matches() {
        let commits = vec![
            make_commit("Add login", "Bob"),
            make_commit("Fix login bug", "Alice"),
        ];
        let index = CommitIndex::build(&commits);
        let results = index.search("login");
        assert_eq!(results.len(), 2);
    }
}
```

- [ ] **Step 2: Run tests — see them fail**

```bash
cargo test store::tests::test_index_search_by_message
```

Expected: FAIL (todo!())

- [ ] **Step 3: Implement CommitIndex**

Replace stubs with real implementation:

```rust
impl CommitIndex {
    pub fn new() -> Self { Self { prefixes: BTreeMap::new() } }

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
                let prefix: String = word[..end].into();
                self.prefixes.entry(prefix).or_default().push(idx);
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
```

- [ ] **Step 4: Run tests**

```bash
cargo test store::tests
```

Expected: all 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/git/store.rs
git commit -m "Implement CommitIndex with prefix search"
```

---

### Task 3: Implement CommitStore paging

**Files:**
- Modify: `src/git/store.rs`

- [ ] **Step 1: Write tests in store::tests**

```rust
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;

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
        let arc1 = store.loaded.clone();
        let arc2 = store.loaded.clone();
        assert!(Arc::strong_count(&store.loaded) >= 3);
        assert_eq!(arc1.len(), 3);
        assert_eq!(arc2.len(), 3);
    }

    #[test]
    fn test_store_search_after_paging() {
        let (dir, repo) = init_test_repo();
        for i in 0..10 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
        }
        add_file_commit(&repo, "z.txt", b"z", "Sphinx of black quartz");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut store = CommitStore::new(&git_repo, 3).unwrap();

        assert!(store.search("sphinx").is_empty());
        while !store.exhausted {
            store.load_batch(&git_repo).unwrap();
            if !store.search("sphinx").is_empty() { break; }
        }
        assert_eq!(store.search("sphinx").len(), 1);
    }
```

- [ ] **Step 2: Run — see them fail**

```bash
cargo test store::tests::test_store_loads_initial_batch
```

Expected: FAIL (todo!())

- [ ] **Step 3: Implement CommitStore**

```rust
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
                    let commit = repository.find_commit(oid)?;
                    self.inner.push(CommitInfo::from_git_commit(&commit));
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
```

- [ ] **Step 4: Run tests**

```bash
cargo test store::tests
```

Expected: 13 tests PASS (7 index + 6 store).

- [ ] **Step 5: Commit**

```bash
git add src/git/store.rs
git commit -m "Implement CommitStore paging with Arc sharing"
```

---

### Task 4: Implement DiffCache

**Files:**
- Modify: `src/git/cache.rs`

- [ ] **Step 1: Write tests at bottom of `src/git/cache.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::list_commits;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;

    #[test]
    fn test_diff_cache_hit() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First");
        add_file_commit(&repo, "a.txt", b"second", "Second");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();

        let mut cache = DiffCache::new(10);
        let _r1 = cache.get_or_compute(&git_repo, &commits[1], &commits[0]).unwrap();
        let _r2 = cache.get_or_compute(&git_repo, &commits[1], &commits[0]).unwrap();
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn test_diff_cache_lru_eviction() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(5);

        for i in 0..5 {
            cache.get_or_compute(&git_repo, &commits[i + 1], &commits[i]).unwrap();
        }
        assert_eq!(cache.entries.len(), 5);

        cache.get_or_compute(&git_repo, &commits[6], &commits[5]).unwrap();
        assert_eq!(cache.entries.len(), 5);
    }

    #[test]
    fn test_diff_cache_lru_refreshes_on_hit() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
        }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(5);

        for i in 0..5 {
            cache.get_or_compute(&git_repo, &commits[i + 1], &commits[i]).unwrap();
        }
        // Re-access first entry → LRU refresh
        cache.get_or_compute(&git_repo, &commits[1], &commits[0]).unwrap();
        // Add new entry → evicts second (oldest unaccessed)
        cache.get_or_compute(&git_repo, &commits[6], &commits[5]).unwrap();
        // First entry still present
        assert!(cache.entries.contains_key(&(commits[1].id, commits[0].id)));
    }
}
```

- [ ] **Step 2: Run — see fail**

```bash
cargo test cache::tests::test_diff_cache_hit
```

Expected: FAIL (todo!())

- [ ] **Step 3: Implement DiffCache**

```rust
impl DiffCache {
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
```

- [ ] **Step 4: Run tests**

```bash
cargo test cache::tests
```

Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/git/cache.rs
git commit -m "Implement DiffCache with LRU eviction"
```

---

### Task 5: Implement TreeCache

**Files:**
- Modify: `src/git/cache.rs`

- [ ] **Step 1: Add tree cache tests**

```rust
    #[test]
    fn test_tree_cache_hit() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Initial");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();

        let mut cache = TreeCache::new(10);
        let t1 = cache.get_or_compute(&git_repo, &commits[0]).unwrap();
        let t2 = cache.get_or_compute(&git_repo, &commits[0]).unwrap();
        assert_eq!(t1.len(), t2.len());
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn test_tree_cache_lru_eviction() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 {
            add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i));
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
```

- [ ] **Step 2: Run — see fail**

```bash
cargo test cache::tests::test_tree_cache_hit
```

Expected: FAIL (todo!())

- [ ] **Step 3: Implement TreeCache**

```rust
impl TreeCache {
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
```

- [ ] **Step 4: Run tests**

```bash
cargo test cache::tests
```

Expected: 5 tests PASS (3 diff + 2 tree).

- [ ] **Step 5: Commit**

```bash
git add src/git/cache.rs
git commit -m "Implement TreeCache with LRU eviction"
```

---

### Task 6: Update PickState — Arc<Vec<CommitInfo>>

**Files:**
- Modify: `src/mode.rs`
- Modify: `src/git/commit.rs`

- [ ] **Step 1: Change PickState.commits type**

In `src/mode.rs`:

```rust
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PickState {
    pub commits: Arc<Vec<CommitInfo>>,
    pub filtered_indices: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub search: SearchState,
    pub selected_diff: Option<DiffResult>,
}

impl PickState {
    pub fn new(commits: Arc<Vec<CommitInfo>>) -> Self {
        let filtered_indices = (0..commits.len()).collect();
        Self {
            commits,
            filtered_indices,
            selected: 0,
            scroll: 0,
            search: SearchState::Idle { query: None },
            selected_diff: None,
        }
    }
}
```

Note: Remove `PartialEq` derive from `PickState` (Arc doesn't implement PartialEq). Instead derive `Clone` only, or impl `PartialEq` manually.

Actually, `Arc<Vec<CommitInfo>>` doesn't impl PartialEq. Let's remove PartialEq from PickState's derive:

```rust
#[derive(Debug, Clone)]
pub struct PickState {
```

Wait, but `Mode` derives `PartialEq` too and contains `PickState`. Let me check what actually needs PartialEq...

Looking at the code, `Mode` derives `PartialEq` but it's mainly used in tests for `assert!(matches!(...))` — not `assert_eq!` on the actual state. Let's check if `Mode` or `PickState` is compared with `==` anywhere...

In `mode.rs` tests, there's `assert_eq!(state.commit, commit)` which compares `CommitInfo` directly, not `PickState`. And `assert_eq!(state.from, from)` compares `CommitInfo`.

`Mode::PartialEq` is likely only used for `matches!()` which doesn't need PartialEq. But to be safe, let me just remove PartialEq from Mode and its children, or impl it manually for PickState.

Actually, `MODE` having `PartialEq` is used in tests like:
```rust
assert!(matches!(app.mode, Mode::Pick(_)));
```
`matches!` doesn't require PartialEq. It's just pattern matching.

Let me remove `PartialEq` from `PickState`, `ViewState`, `DiffState`, and `Mode`. The test `assert_eq!` calls in mode.rs compare `CommitInfo` and `DiffResult` directly, not the whole state.

Actually, wait — let's check what derives are needed. `Mode` derives `Debug, Clone, PartialEq`. I'll remove `PartialEq` from the ones that now contain `Arc`:

- `PickState`: remove `PartialEq` (contains Arc)
- `ViewState`: keep `PartialEq` (no Arc)
- `DiffState`: keep `PartialEq` (no Arc)  
- `Mode`: keep `PartialEq` but need to impl it since PickState lost it

Actually for `Mode`, we can do:

```rust
impl PartialEq for PickState {
    fn eq(&self, other: &Self) -> bool {
        self.filtered_indices == other.filtered_indices
            && self.selected == other.selected
            && self.scroll == other.scroll
            && self.search == other.search
            && self.selected_diff == other.selected_diff
    }
}
```

This avoids comparing the Arc itself. Or we can just derive it:

```rust
#[derive(Debug, Clone, PartialEq)]
```

Wait, `Arc<Vec<CommitInfo>>` — does `Arc<T>` implement `PartialEq`? Let me check. In Rust std, `Arc<T>` implements `PartialEq` if `T: PartialEq`. And `Vec<CommitInfo>` implements `PartialEq` if `CommitInfo: PartialEq`. And `CommitInfo` derives `PartialEq`.

So `Arc<Vec<CommitInfo>>` should implement `PartialEq` by pointer equality? No — `Arc` implements `PartialEq` by delegating to the inner type's `PartialEq`. So `Arc<Vec<CommitInfo>>` compares the vectors, not the pointers.

Wait, actually — `Arc`'s `PartialEq` impl is `impl<T: PartialEq> PartialEq for Arc<T>` which compares `*self == *other` — i.e., it dereferences and compares the values. So two Arcs that point to the same data but are different Arc instances will compare equal.

So we CAN keep `PartialEq` on `PickState`! Let me verify this...

From the Rust std docs: `impl<T: PartialEq> PartialEq for Arc<T>` — "Equality for two `Arc`s. Two `Arc`s are equal if their inner values are equal, even if they are stored in different allocation."

Great, so `#[derive(Debug, Clone, PartialEq)]` works fine with `Arc<Vec<CommitInfo>>`.

- [ ] **Step 2: Update PickState::update_filter to use store search**

We'll pass the CommitStore or the search function. Since `update_filter` currently calls `search_commits`, we can change it to accept a search function or the store:

```rust
    pub fn update_filter(&mut self, query: &str) {
        self.filtered_indices = if query.is_empty() {
            (0..self.commits.len()).collect()
        } else {
            self.search_index(query)
        };
        self.selected = 0;
        self.scroll = 0;
    }

    fn search_index(&self, query: &str) -> Vec<usize> {
        // We can't call store.search() here without access to store.
        // Alternative: keep a fallback linear search, or pass store ref.
        // For now: still use linear O(n) on loaded commits.
        // The index is in CommitStore — update_filter will be called from App
        // which has access to store.
        let q = query.to_lowercase();
        self.commits
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.message.to_lowercase().contains(&q)
                    || c.author.to_lowercase().contains(&q)
                    || c.short_id.starts_with(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }
```

Actually, better: the App will call `store.search()` and set filtered_indices directly. Let's have App handle the search, and remove `update_filter` from PickState. Or keep it as a convenience that takes a closure.

Let me keep it simple: PickState.update_filter stays as before (calls search_commits on the Arc), but App can override filtered_indices directly with store.search() for better performance.

Let me actually just change update_filter to accept a search function:

No wait, that's overcomplicating. Let me just delete search_commits from commit.rs and have PickState always use linear search (it's on loaded commits which are at most a few hundred before more are loaded). The store's CommitIndex is used via `store.search()` when in App context.

Actually the simplest approach: remove `search_commits` function from commit.rs. PickState.update_filter does a linear scan on `self.commits` (which is an Arc pointing to at most a few hundred items in memory). This is fine — the CommitStore index is used for the App-level search which pre-filters.

Wait, but `update_filter` IS the App-level search. It's called from `handle_search_input`. Let me just have it use a simple linear scan for now — the loaded commits are at most 200-400 until more are loaded. The CommitIndex in store is available in App context but not easily accessible from PickState.

Best approach: Keep PickState.update_filter as is (linear scan on Arc commits), AND also have App use store.search() for the filtered_indices rebuild when paging happens. The CommitIndex in store is used to quickly rebuild filters after paging.

OK let me just write the code:

- [ ] **Step 3: Commit**

```bash
git add src/mode.rs
git commit -m "Switch PickState.commits to Arc<Vec<CommitInfo>>"
```

Wait, I need to remove `search_commits` from commit.rs too since it's no longer needed (PickState does its own linear scan):

Actually `search_commits` is used by PickState.update_filter currently. Let me keep it but change it — or just move the logic inline.

Actually wait: `search_commits` takes `&[CommitInfo]`, but now `PickState.commits` is `Arc<Vec<CommitInfo>>`. We can just deref the Arc. Let me keep `search_commits` as is but have PickState.update_filter use it.

Actually you know what, let's keep things simple. I'll keep the `search_commits` function in commit.rs, and PickState.update_filter will call it. The function signature needs to change to accept `&Arc<Vec<CommitInfo>>` but since `Arc<Vec<CommitInfo>>` derefs to `[CommitInfo]`, `&[CommitInfo]` works.

OK, I think this is getting too detailed for the plan. Let me simplify — I'll remove search_commits and put the linear scan in PickState. Let me just write the plan concisely and move on.

Let me finalize the plan. The key thing the user asked was about prefetch (-n before end). I'll make sure that's captured in Task 6.

Let me take a completely different approach and write a much more concise plan.

---

OK here's my plan for a rewritten plan:

Task 1: Stubs + mod.rs (unchanged)
Task 2: CommitIndex (unchanged) 
Task 3: CommitStore paging (unchanged)
Task 4: DiffCache (unchanged)
Task 5: TreeCache (unchanged)
Task 6: Update PickState for Arc
Task 7: Integrate into App (the big one)
Task 8: Remove search_commits from commit.rs
Task 9: Add init_test_repo_with_n_commits helper
Task 10: Integration tests
Task 11: Format + test + commit

For Task 7 (App integration), I'll show the key method changes but note that borrow checker workarounds (extracting data before mutating) will be resolved during implementation. The important part is:
- App.store instead of app.commits
- diff_cache and tree_cache usage
- Prefetch at `selected >= commits.len() - 50`
- All methods that used self.commits now use self.store.loaded.clone()

Let me rewrite the plan now.<｜end▁of▁thinking｜>

<｜｜DSML｜｜tool_calls>
<｜｜DSML｜｜invoke name="write">
<｜｜DSML｜｜parameter name="content" string="true"># Perf Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make glc start in <1s and respond <16ms on repos with 100k+ commits via paging, caching, and deduplication. Prefetch next batch when cursor reaches `len - 50`.

**Architecture:** `CommitStore` (paging, Arc-shared, prefix index), `DiffCache` / `TreeCache` (LRU). Remove `App.commits` and `PickState.commits` duplication — all share `Arc<Vec<CommitInfo>>`.

**Tech Stack:** Rust, git2, no new crates (manual LRU with VecDeque).

---

## File Map

| File | What |
|------|------|
| `src/git/store.rs` **new** | `CommitStore` + `CommitIndex` |
| `src/git/cache.rs` **new** | `DiffCache` + `TreeCache` |
| `src/git/mod.rs` | add store, cache modules |
| `src/git/commit.rs` | remove `search_commits` |
| `src/git/repo.rs` | add `init_test_repo_with_n_commits` |
| `src/mode.rs` | `PickState.commits` → `Arc<Vec<CommitInfo>>`; `update_filter` inline |
| `src/app.rs` | `store`, `diff_cache`, `tree_cache` fields; all `self.commits` → `store`; prefetch paging |

---

### Task 1: Stubs + mod.rs

**Files:** Create `src/git/store.rs`, `src/git/cache.rs`; modify `src/git/mod.rs`

- [ ] **Step 1: Create `src/git/store.rs`**

```rust
use std::collections::BTreeMap;
use std::sync::Arc;
use crate::git::commit::CommitInfo;
use crate::git::repo::{GitError, GitRepo};

pub struct CommitIndex { prefixes: BTreeMap<String, Vec<usize>> }

impl CommitIndex {
    pub fn new() -> Self { Self { prefixes: BTreeMap::new() } }
    pub fn build(commits: &[CommitInfo]) -> Self { todo!() }
    pub fn search(&self, query: &str) -> Vec<usize> { todo!() }
    pub fn append(&mut self, start_idx: usize, commits: &[CommitInfo]) { todo!() }
}

pub struct CommitStore {
    pub loaded: Arc<Vec<CommitInfo>>,
    inner: Vec<CommitInfo>,
    pub index: CommitIndex,
    pub exhausted: bool,
    batch_size: usize,
}

impl CommitStore {
    pub fn new(repo: &GitRepo, batch_size: usize) -> Result<Self, GitError> { todo!() }
    pub fn load_batch(&mut self, repo: &GitRepo) -> Result<usize, GitError> { todo!() }
    pub fn search(&self, query: &str) -> Vec<usize> { self.index.search(query) }
    pub fn total_loaded(&self) -> usize { self.inner.len() }
}
```

- [ ] **Step 2: Create `src/git/cache.rs`**

```rust
use std::collections::{HashMap, VecDeque};
use git2::Oid;
use crate::git::commit::CommitInfo;
use crate::git::diff::{DiffResult, compute_diff};
use crate::git::tree::{FileEntry, list_tree};
use crate::git::repo::{GitError, GitRepo};

pub struct DiffCache {
    entries: HashMap<(Oid, Oid), DiffResult>,
    order: VecDeque<(Oid, Oid)>,
    max_size: usize,
}

impl DiffCache {
    pub fn new(max_size: usize) -> Self {
        Self { entries: HashMap::new(), order: VecDeque::new(), max_size }
    }
    pub fn get_or_compute(&mut self, repo: &GitRepo, parent: &CommitInfo, commit: &CommitInfo)
        -> Result<&DiffResult, GitError> { todo!() }
}

pub struct TreeCache {
    entries: HashMap<Oid, Vec<FileEntry>>,
    order: VecDeque<Oid>,
    max_size: usize,
}

impl TreeCache {
    pub fn new(max_size: usize) -> Self {
        Self { entries: HashMap::new(), order: VecDeque::new(), max_size }
    }
    pub fn get_or_compute(&mut self, repo: &GitRepo, commit: &CommitInfo)
        -> Result<&Vec<FileEntry>, GitError> { todo!() }
}
```

- [ ] **Step 3: Update `src/git/mod.rs`** — add `pub mod store; pub mod cache;` and `pub use store::CommitStore;`

- [ ] **Step 4: Check compiles** — `cargo check` (all `todo!()` in dead paths)

- [ ] **Step 5: Commit**

```bash
git add src/git/store.rs src/git/cache.rs src/git/mod.rs
git commit -m "Add CommitStore and cache module stubs"
```

---

### Task 2: CommitIndex — prefix search

**Files:** Modify `src/git/store.rs`

- [ ] **Step 1: Write tests** (add to bottom of `src/git/store.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::CommitInfo;
    use git2::Oid;

    fn make_commit(msg: &str, author: &str) -> CommitInfo {
        CommitInfo {
            id: Oid::zero(), short_id: "abc1234".into(),
            author: author.into(), date: std::time::UNIX_EPOCH, message: msg.into(),
        }
    }

    #[test]
    fn test_index_search_by_message() {
        let commits = vec![make_commit("Fix login bug", "Alice"), make_commit("Add auth module", "Bob")];
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
        let index = CommitIndex::build(&[make_commit("Add login", "Bob"), make_commit("Fix login bug", "Alice")]);
        assert_eq!(index.search("login").len(), 2);
    }
}
```

- [ ] **Step 2: Run → fail** — `cargo test store::tests::test_index_search_by_message`

- [ ] **Step 3: Implement**

```rust
impl CommitIndex {
    pub fn build(commits: &[CommitInfo]) -> Self {
        let mut index = Self::new();
        for (i, c) in commits.iter().enumerate() { index.insert(i, c); }
        index
    }

    pub fn append(&mut self, start_idx: usize, commits: &[CommitInfo]) {
        for (i, c) in commits.iter().enumerate() { self.insert(start_idx + i, c); }
    }

    fn insert(&mut self, idx: usize, commit: &CommitInfo) {
        self.index_tokens(idx, &commit.message.to_lowercase());
        self.index_tokens(idx, &commit.author.to_lowercase());
        self.index_tokens(idx, &commit.short_id.to_lowercase());
    }

    fn index_tokens(&mut self, idx: usize, text: &str) {
        for word in text.split_whitespace() {
            for end in 1..=word.len() {
                self.prefixes.entry(word[..end].into()).or_default().push(idx);
            }
        }
    }

    pub fn search(&self, query: &str) -> Vec<usize> {
        let q = query.to_lowercase().trim().to_string();
        if q.is_empty() { return vec![]; }
        let Some(indices) = self.prefixes.get(&q) else { return vec![]; };
        let mut result = indices.clone();
        result.sort_unstable();
        result.dedup();
        result
    }
}
```

- [ ] **Step 4: Run** — `cargo test store::tests` → 7 PASS

- [ ] **Step 5: Commit** — `git add src/git/store.rs && git commit -m "Implement CommitIndex prefix search"`

---

### Task 3: CommitStore paging

**Files:** Modify `src/git/store.rs`

- [ ] **Step 1: Write tests**

```rust
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;

    #[test]
    fn test_store_loads_initial_batch() {
        let (dir, repo) = init_test_repo();
        for i in 0..10 { add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i)); }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let store = CommitStore::new(&git_repo, 5).unwrap();
        assert_eq!(store.loaded.len(), 5);
        assert!(!store.exhausted);
    }

    #[test]
    fn test_store_paging_loads_more() {
        let (dir, repo) = init_test_repo();
        for i in 0..10 { add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i)); }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut store = CommitStore::new(&git_repo, 5).unwrap();
        assert_eq!(store.loaded.len(), 5);
        assert_eq!(store.load_batch(&git_repo).unwrap(), 5);
        assert_eq!(store.loaded.len(), 10);
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
        for i in 0..3 { add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i)); }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let store = CommitStore::new(&git_repo, 3).unwrap();
        let a1 = store.loaded.clone();
        let a2 = store.loaded.clone();
        assert!(Arc::strong_count(&store.loaded) >= 3);
        assert_eq!(a1.len(), 3);
    }

    #[test]
    fn test_store_search_after_paging() {
        let (dir, repo) = init_test_repo();
        for i in 0..10 { add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i)); }
        add_file_commit(&repo, "z.txt", b"z", "Sphinx of black quartz");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut store = CommitStore::new(&git_repo, 3).unwrap();
        assert!(store.search("sphinx").is_empty());
        while !store.exhausted { store.load_batch(&git_repo).unwrap(); }
        assert_eq!(store.search("sphinx").len(), 1);
    }
```

- [ ] **Step 2: Run → fail** — `cargo test store::tests::test_store_loads_initial_batch`

- [ ] **Step 3: Implement**

```rust
impl CommitStore {
    pub fn new(repo: &GitRepo, batch_size: usize) -> Result<Self, GitError> {
        let mut store = Self {
            loaded: Arc::new(vec![]), inner: Vec::new(),
            index: CommitIndex::new(), exhausted: false, batch_size,
        };
        store.load_batch(repo)?;
        Ok(store)
    }

    pub fn load_batch(&mut self, repo: &GitRepo) -> Result<usize, GitError> {
        if self.exhausted { return Ok(0); }
        let repository = repo.repository();
        let mut revwalk = repository.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

        let skip = self.inner.len();
        for _ in 0..skip {
            if revwalk.next().is_none() { self.exhausted = true; return Ok(0); }
        }

        let start_idx = self.inner.len();
        let mut count = 0;
        for _ in 0..self.batch_size {
            match revwalk.next() {
                Some(Ok(oid)) => {
                    self.inner.push(CommitInfo::from_git_commit(&repository.find_commit(oid)?));
                    count += 1;
                }
                _ => { self.exhausted = true; break; }
            }
        }
        if count > 0 { self.index.append(start_idx, &self.inner[start_idx..]); }
        self.loaded = Arc::new(self.inner.clone());
        Ok(count)
    }
}
```

- [ ] **Step 4: Run** — `cargo test store::tests` → 13 PASS (7 index + 6 store)

- [ ] **Step 5: Commit** — `git add src/git/store.rs && git commit -m "Implement CommitStore paging with Arc sharing"`

---

### Task 4: DiffCache

**Files:** Modify `src/git/cache.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::list_commits;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;

    #[test]
    fn test_diff_cache_hit() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First");
        add_file_commit(&repo, "a.txt", b"second", "Second");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(10);
        cache.get_or_compute(&git_repo, &commits[1], &commits[0]).unwrap();
        cache.get_or_compute(&git_repo, &commits[1], &commits[0]).unwrap();
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn test_diff_cache_lru_eviction() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 { add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i)); }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(5);
        for i in 0..5 { cache.get_or_compute(&git_repo, &commits[i+1], &commits[i]).unwrap(); }
        assert_eq!(cache.entries.len(), 5);
        cache.get_or_compute(&git_repo, &commits[6], &commits[5]).unwrap();
        assert_eq!(cache.entries.len(), 5);
    }

    #[test]
    fn test_diff_cache_lru_refreshes_on_hit() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 { add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i)); }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = DiffCache::new(5);
        for i in 0..5 { cache.get_or_compute(&git_repo, &commits[i+1], &commits[i]).unwrap(); }
        cache.get_or_compute(&git_repo, &commits[1], &commits[0]).unwrap(); // refresh
        cache.get_or_compute(&git_repo, &commits[6], &commits[5]).unwrap(); // evict
        assert!(cache.entries.contains_key(&(commits[1].id, commits[0].id)));
    }
}
```

- [ ] **Step 2: Run → fail** — `cargo test cache::tests::test_diff_cache_hit`

- [ ] **Step 3: Implement**

```rust
impl DiffCache {
    pub fn get_or_compute(&mut self, repo: &GitRepo, parent: &CommitInfo, commit: &CommitInfo)
        -> Result<&DiffResult, GitError>
    {
        let key = (parent.id, commit.id);
        if self.entries.contains_key(&key) { self.touch(&key); return Ok(&self.entries[&key]); }
        let result = compute_diff(repo, parent, commit)?;
        self.insert(key, result);
        Ok(&self.entries[&key])
    }

    fn insert(&mut self, key: (Oid, Oid), result: DiffResult) {
        if self.entries.len() >= self.max_size {
            if let Some(oldest) = self.order.pop_front() { self.entries.remove(&oldest); }
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
```

- [ ] **Step 4: Run** — `cargo test cache::tests` → 3 PASS

- [ ] **Step 5: Commit** — `git add src/git/cache.rs && git commit -m "Implement DiffCache LRU"`

---

### Task 5: TreeCache

**Files:** Modify `src/git/cache.rs`

- [ ] **Step 1: Add tree cache tests**

```rust
    #[test]
    fn test_tree_cache_hit() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Initial");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = TreeCache::new(10);
        let t1 = cache.get_or_compute(&git_repo, &commits[0]).unwrap();
        let t2 = cache.get_or_compute(&git_repo, &commits[0]).unwrap();
        assert_eq!(t1.len(), t2.len());
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn test_tree_cache_lru_eviction() {
        let (dir, repo) = init_test_repo();
        for i in 0..15 { add_file_commit(&repo, &format!("f{}.txt", i), b"x", &format!("c{}", i)); }
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let mut cache = TreeCache::new(5);
        for i in 0..5 { cache.get_or_compute(&git_repo, &commits[i]).unwrap(); }
        assert_eq!(cache.entries.len(), 5);
        cache.get_or_compute(&git_repo, &commits[5]).unwrap();
        assert_eq!(cache.entries.len(), 5);
    }
```

- [ ] **Step 2: Run → fail** — `cargo test cache::tests::test_tree_cache_hit`

- [ ] **Step 3: Implement**

```rust
impl TreeCache {
    pub fn get_or_compute(&mut self, repo: &GitRepo, commit: &CommitInfo)
        -> Result<&Vec<FileEntry>, GitError>
    {
        let key = commit.id;
        if self.entries.contains_key(&key) { self.touch(&key); return Ok(&self.entries[&key]); }
        let tree = list_tree(repo, commit)?;
        self.insert(key, tree);
        Ok(&self.entries[&key])
    }

    fn insert(&mut self, key: Oid, entries: Vec<FileEntry>) {
        if self.entries.len() >= self.max_size {
            if let Some(oldest) = self.order.pop_front() { self.entries.remove(&oldest); }
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
```

- [ ] **Step 4: Run** — `cargo test cache::tests` → 5 PASS (3 diff + 2 tree)

- [ ] **Step 5: Commit** — `git add src/git/cache.rs && git commit -m "Implement TreeCache LRU"`

---

### Task 6: PickState → Arc, remove search_commits

**Files:** Modify `src/mode.rs`, `src/git/commit.rs`

- [ ] **Step 1: Change PickState.commits type**

In `src/mode.rs`, add `use std::sync::Arc;` at top. Change:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PickState {
    pub commits: Arc<Vec<CommitInfo>>,
    // ... rest unchanged
}

impl PickState {
    pub fn new(commits: Arc<Vec<CommitInfo>>) -> Self {
        let filtered_indices = (0..commits.len()).collect();
        Self { commits, filtered_indices, selected: 0, scroll: 0,
               search: SearchState::Idle { query: None }, selected_diff: None }
    }

    pub fn update_filter(&mut self, query: &str) {
        let q = query.to_lowercase();
        self.filtered_indices = if query.is_empty() {
            (0..self.commits.len()).collect()
        } else {
            self.commits.iter().enumerate()
                .filter(|(_, c)| c.message.to_lowercase().contains(&q)
                    || c.author.to_lowercase().contains(&q)
                    || c.short_id.starts_with(&q))
                .map(|(i, _)| i).collect()
        };
        self.selected = 0;
        self.scroll = 0;
    }
}
```

- [ ] **Step 2: Remove `search_commits` from `src/git/commit.rs`**

Delete the `search_commits` function and its tests (`test_search_by_message`, `test_search_by_hash_prefix`).

- [ ] **Step 3: Fix mode.rs tests that use PickState::new**

All calls like `PickState::new(vec![...])` → `PickState::new(Arc::new(vec![...]))`.

- [ ] **Step 4: Run mode tests** — `cargo test mode::tests` → all PASS

- [ ] **Step 5: Commit**

```bash
git add src/mode.rs src/git/commit.rs
git commit -m "Switch PickState.commits to Arc, inline search"
```

---

### Task 7: Integrate into App

**Files:** Modify `src/app.rs`

This is the core integration. We replace `self.commits` with `self.store`, use `diff_cache`/`tree_cache`, and add prefetch paging.

- [ ] **Step 1: Update imports and App struct**

```rust
use crate::git::cache::{DiffCache, TreeCache};
use crate::git::store::CommitStore;
use std::sync::Arc;

pub struct App {
    pub mode: Mode,
    pub repo: GitRepo,
    pub store: CommitStore,
    pub diff_cache: DiffCache,
    pub tree_cache: TreeCache,
    // ... rest unchanged
}
```

Remove `pub commits: Vec<CommitInfo>`.

- [ ] **Step 2: Update App::new**

```rust
pub fn new(repo: GitRepo, config: Config) -> Result<Self> {
    let mut store = CommitStore::new(&repo, 200)?;
    let pick_state = PickState::new(store.loaded.clone());
    let theme_name = config.theme.name.clone();
    let palette = crate::theme::resolve_palette(Some(&theme_name));
    let mut app = Self {
        mode: Mode::Pick(pick_state),
        repo,
        store,
        diff_cache: DiffCache::new(64),
        tree_cache: TreeCache::new(32),
        keybindings: KeyBindings::default_bindings(),
        should_quit: false,
        debug_overlay: false,
        highlight: HighlightEngine::new(),
        palette,
        theme_name,
        config,
        saved_search: SearchState::Idle { query: None },
    };
    app.highlight.set_theme(app.palette.to_highlight_map());
    app.update_pick_diff();
    Ok(app)
}
```

- [ ] **Step 3: Add prefetch paging helper and update move_down**

```rust
fn prefetch_if_near_end(&mut self) {
    if self.store.exhausted {
        return;
    }
    let (commit_idx, total) = match &self.mode {
        Mode::Pick(state) => {
            let absolute_idx = state.filtered_indices.get(state.selected).copied().unwrap_or(0);
            (absolute_idx, state.commits.len())
        }
        _ => return,
    };
    // Prefetch when within 50 of end
    if commit_idx + 50 >= total {
        let _ = self.store.load_batch(&self.repo);
        if let Mode::Pick(state) = &mut self.mode {
            let prev_selected = state.selected;
            state.commits = self.store.loaded.clone();
            let query = state.query().map(|s| s.to_string());
            if let Some(q) = query {
                state.update_filter(&q);
                state.selected = state.filtered_indices.get(prev_selected).copied().unwrap_or(0);
            } else {
                state.filtered_indices = (0..state.commits.len()).collect();
                state.selected = prev_selected;
            }
        }
    }
}
```

Update `move_down` to call `prefetch_if_near_end`:

```rust
fn move_down(&mut self) {
    match &mut self.mode {
        Mode::Pick(state) => {
            let max = state.filtered_indices.len().saturating_sub(1);
            state.selected = state.selected.saturating_add(1).min(max);
        }
        Mode::View(state) => {
            let max = state.tree.len().saturating_sub(1);
            state.selected_file = state.selected_file.saturating_add(1).min(max);
            self.load_view_file();
        }
        Mode::Diff(state) => {
            let max = state.diff_result.files.len().saturating_sub(1);
            let prev = state.selected_file;
            state.selected_file = state.selected_file.saturating_add(1).min(max);
            if state.selected_file != prev { state.scroll = 0; }
        }
    }
    if matches!(&self.mode, Mode::Pick(_)) {
        self.prefetch_if_near_end();
        self.update_pick_diff();
    }
}
```

- [ ] **Step 4: Update update_pick_diff — use DiffCache**

```rust
fn update_pick_diff(&mut self) {
    let (parent_info, commit) = {
        let Mode::Pick(state) = &mut self.mode else { return; };
        state.selected_diff = None;
        let Some(&idx) = state.filtered_indices.get(state.selected) else { return; };
        let commit = state.commits[idx].clone();
        let repository = self.repo.repository();
        let Ok(commit_obj) = repository.find_commit(commit.id) else { return; };
        let parent = match commit_obj.parent(0) {
            Ok(p) => p, Err(_) => return,
        };
        (CommitInfo::from_git_commit(&parent), commit)
    };
    let diff = self.diff_cache.get_or_compute(&self.repo, &parent_info, &commit).ok().cloned();
    if let Mode::Pick(state) = &mut self.mode {
        state.selected_diff = diff;
    }
}
```

Note: Extract parent_info+commit before calling diff_cache to avoid double borrow.

- [ ] **Step 5: Update back() — use store.loaded**

Replace `let mut pick = PickState::new(self.commits.clone());` with `let mut pick = PickState::new(self.store.loaded.clone());`

- [ ] **Step 6: Update enter() — use store.loaded**

Replace `state.commits[idx].clone()` with `state.commits[idx].clone()` (already using Arc deref — works as is).

- [ ] **Step 7: Update switch_mode, next_commit, prev_commit**

Every `self.commits.iter().position(...)` → `self.store.loaded.iter().position(...)`.
Every `self.commits[idx]` → `self.store.loaded[idx]`.
Where `compute_diff(&self.repo, &from, &to)` is called, use `self.diff_cache.get_or_compute(...)`.

- [ ] **Step 8: Update make_view_state, compute_changed_stats**

`make_view_state` → use `self.tree_cache.get_or_compute(...)` for tree, `self.diff_cache.get_or_compute(...)` for stats:

```rust
fn make_view_state(&mut self, commit: CommitInfo) -> ViewState {
    let tree = self.tree_cache.get_or_compute(&self.repo, &commit)
        .cloned().unwrap_or_default();
    let changed_stats = {
        let repository = self.repo.repository();
        if let Ok(commit_obj) = repository.find_commit(commit.id) {
            if let Ok(parent) = commit_obj.parent(0) {
                let parent_info = CommitInfo::from_git_commit(&parent);
                self.diff_cache.get_or_compute(&self.repo, &parent_info, &commit)
                    .map(|r| {
                        r.files.iter().filter_map(|f| {
                            let path = f.change.as_ref().map(|c| c.path().to_string())?;
                            let added = f.lines.iter().filter(|l| matches!(l, crate::git::diff::DiffLine::Added{..})).count();
                            let removed = f.lines.iter().filter(|l| matches!(l, crate::git::diff::DiffLine::Removed{..})).count();
                            Some((path, (added, removed)))
                        }).collect()
                    }).unwrap_or_default()
            } else { HashMap::new() }
        } else { HashMap::new() }
    };
    let changed_paths = changed_stats.keys().cloned().collect();
    ViewState { commit, tree, selected_file: 0,
        file_content: crate::mode::FileContent::NotLoaded, scroll: 0,
        show_ignored: true, changed_paths, changed_stats }
}
```

Remove the old `compute_changed_stats` method (its logic is now inline in make_view_state).

- [ ] **Step 9: Update toggle_gitignore — use tree_cache**

Replace `list_tree(&self.repo, &state.commit)` with `self.tree_cache.get_or_compute(&self.repo, &state.commit).cloned().unwrap_or_default()`.

- [ ] **Step 10: Update render_debug_overlay**

Replace `s.commits.len()` which now works (Arc deref), but also add loaded/exhausted info:

```rust
// Change format string to show store info:
format!("Mode: {} | Selected: {} | Loaded: {} | Filtered: {} | Exhausted: {}",
    mode_name, s.selected, s.commits.len(), s.filtered_indices.len(), self.store.exhausted)
```

- [ ] **Step 11: Update existing App tests**

Every `app.commits` reference in tests → `app.store.loaded`. E.g.:

```rust
// test_commits_cached_in_app:
assert!(!app.store.loaded.is_empty());
if let Mode::Pick(state) = &app.mode {
    assert_eq!(app.store.loaded.len(), state.commits.len());
}

// test_ctrl_p_at_oldest_stays:
let last_commit_id = app.store.loaded.last().unwrap().id;
```

- [ ] **Step 12: Run all tests** — `cargo test` → all existing tests PASS

- [ ] **Step 13: Commit**

```bash
git add src/app.rs
git commit -m "Integrate CommitStore, caches, and prefetch paging into App"
```

---

### Task 8: Test helper — init_test_repo_with_n_commits

**Files:** Modify `src/git/repo.rs`

- [ ] **Step 1: Add helper**

```rust
pub fn init_test_repo_with_n_commits(n: usize) -> (TempDir, Repository) {
    let (dir, repo) = init_test_repo();
    for i in 0..n {
        add_file_commit(
            &repo,
            &format!("f{}.txt", i),
            format!("content {}", i).as_bytes(),
            &format!("Commit number {}", i),
        );
    }
    (dir, repo)
}
```

- [ ] **Step 2: Write test**

Add to the test module:

```rust
    #[test]
    fn test_create_n_commits() {
        let (_dir, repo) = init_test_repo_with_n_commits(50);
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_head().unwrap();
        assert_eq!(revwalk.count(), 50);
    }
```

- [ ] **Step 3: Run** — `cargo test repo::tests::test_create_n_commits` → PASS

- [ ] **Step 4: Commit** — `git add src/git/repo.rs && git commit -m "Add init_test_repo_with_n_commits helper"`

---

### Task 9: Integration tests

**Files:** Modify `src/app.rs` (add tests at bottom)

- [ ] **Step 1: Add paging integration test**

```rust
    #[test]
    fn test_paging_triggers_on_near_end() {
        let (dir, repo) = init_test_repo_with_n_commits(300);
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();

        // Initial load: 200
        assert_eq!(app.store.loaded.len(), 200);
        assert!(!app.store.exhausted);

        // Navigate to near end (commit 150 → 200 → triggers prefetch)
        for _ in 0..150 {
            app.handle_key(KeyCode::Char('j'));
        }
        // After reaching commit ~150 (absolute index) + 50 ≥ 200 → prefetch triggered
        // The loaded count should have increased
        assert!(app.store.loaded.len() > 200 || app.store.exhausted);
    }

    #[test]
    fn test_diff_cache_hit_on_cursor_move() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First");
        add_file_commit(&repo, "a.txt", b"second", "Second");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();

        // Move down (to commit 2) → diff computed and cached
        app.handle_key(KeyCode::Char('j'));
        // Move up (back to commit 1) → should cache hit
        app.handle_key(KeyCode::Char('k'));

        let Mode::Pick(s) = &app.mode else { panic!("expected pick") };
        assert!(s.selected_diff.is_some());
    }

    #[test]
    fn test_tree_cache_hit_on_view_reentry() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Initial");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();

        // Enter view (populates tree cache)
        app.handle_key(KeyCode::Enter);
        // Back to pick
        app.handle_key(KeyCode::Esc);
        // Enter view again (should cache hit)
        app.handle_key(KeyCode::Enter);

        let Mode::View(s) = &app.mode else { panic!("expected view") };
        assert!(!s.tree.is_empty());
    }
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test app::tests::test_paging_triggers_on_near_end
cargo test app::tests::test_diff_cache_hit_on_cursor_move
cargo test app::tests::test_tree_cache_hit_on_view_reentry
```

Expected: all 3 PASS.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "Add paging, cache, and tree integration tests"
```

---

### Task 10: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

```bash
cargo test
```

Expected: all tests PASS.

- [ ] **Step 2: Run format check**

```bash
cargo fmt --check
```

Expected: fails on pre-existing files, but `src/git/store.rs`, `src/git/cache.rs`, `src/mode.rs`, `src/app.rs` should be clean. If any of our new files fail, run `cargo fmt` on them.

- [ ] **Step 3: Clippy**

```bash
cargo clippy -- -D warnings
```

Fix any warnings in our changed files.

- [ ] **Step 4: Release build**

```bash
cargo build --release
```

Expected: builds successfully.

- [ ] **Step 5: Manual test on real repo**

```bash
cargo run --release -- /path/to/large-repo
```

Verify: fast startup, responsive cursor, search works.

- [ ] **Step 6: Commit any fixes**

```bash
git add -u && git commit -m "Fix clippy warnings and formatting"
```
