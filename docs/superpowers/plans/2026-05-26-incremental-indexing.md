# Incremental Indexing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** HEAD가 변경되어도 전체 인덱스를 재구축하지 않고, 변경된 파일/커밋만 골라 BM25 + turbovec 인덱스에 add/remove를 적용한다.

**Architecture:** `glc index`가 기존 `meta.toml::head_oid`와 현재 HEAD를 비교, `git2::Tree::diff`로 added/modified/deleted 파일 목록과 신규 커밋을 산출. tantivy `IndexWriter::delete_term`과 turbovec `IdMapIndex::remove`로 stale 문서 제거 후 변경분만 새로 임베딩·삽입. fallback 조건(스키마 변경, old_head 부재, 토크나이저/모델 변경)에서는 기존 full rebuild 경로로 회귀.

**Tech Stack:** Rust 2021, tantivy 0.22 (delete_term), turbovec 0.5 (remove/add_with_ids), git2 (tree diff + revwalk), model2vec-rs.

---

## File Structure

- **Modify** `src/search/bm25.rs` — `delete_doc(writer, doc_id)` 추가
- **Modify** `src/search/vector.rs` — `remove(id) -> bool` 래퍼 추가
- **Create** `src/search/diff.rs` — HEAD 간 파일 diff + 신규 커밋 산출 헬퍼
- **Modify** `src/search/indexer.rs` — `build_index` 진입점에서 incremental vs full 분기, `build_index_incremental` 신규
- **Modify** `src/search/mod.rs` — 모듈 등록 (`pub mod diff;`)

`IndexMeta`는 변경 없음 — 기존 `head_oid` 필드로 충분. doc_counter는 BM25 `scan_doc_store`의 `max(doc_id) + 1`로 복원.

---

## Task 1: BM25 단일 문서 삭제 API

**Files:**
- Modify: `src/search/bm25.rs` (impl Bm25Index 안)
- Test: `src/search/bm25.rs` (#[cfg(test)] mod tests 안)

- [ ] **Step 1: Write the failing test**

`src/search/bm25.rs` 의 `mod tests` 블록 안에 추가:

```rust
#[test]
fn test_delete_doc_removes_from_search() {
    let (_dir, idx) = tmp_index();
    let mut w = idx.writer().unwrap();
    idx.add_doc(&mut w, &commit_meta(1, "hello world"), "greeting")
        .unwrap();
    idx.add_doc(&mut w, &commit_meta(2, "hello again"), "second")
        .unwrap();
    idx.commit(w).unwrap();
    assert_eq!(idx.search("he", 10).unwrap().len(), 2);

    let mut w = idx.writer().unwrap();
    idx.delete_doc(&mut w, 1);
    idx.commit(w).unwrap();
    let r = idx.search("he", 10).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0, 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib search::bm25::tests::test_delete_doc_removes_from_search`
Expected: FAIL — `no method named delete_doc found`

- [ ] **Step 3: Add `delete_doc` to `impl Bm25Index`**

`add_doc`와 `commit` 사이에 추가:

```rust
pub fn delete_doc(&self, writer: &mut IndexWriter, doc_id: u64) {
    let term = tantivy::Term::from_field_u64(self.fields.id, doc_id);
    writer.delete_term(term);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib search::bm25::tests::test_delete_doc_removes_from_search`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/bm25.rs
git commit -m "Bm25Index::delete_doc 추가"
```

---

## Task 2: VectorIndex remove 래퍼

**Files:**
- Modify: `src/search/vector.rs`
- Test: `src/search/vector.rs` (#[cfg(test)] mod tests 안)

- [ ] **Step 1: Write the failing test**

`src/search/vector.rs` 의 `mod tests` 블록 안에 추가:

```rust
#[test]
fn test_remove_drops_from_search() {
    let dim = 16;
    let mut idx = VectorIndex::new(dim);
    idx.add(&[1, 2], &[make_vec(1.0, dim), make_vec(0.1, dim)])
        .unwrap();
    assert!(idx.remove(1));
    let results = idx.search(&make_vec(1.0, dim), 5);
    assert!(results.iter().all(|(id, _)| *id != 1));
    assert!(!idx.remove(1), "second remove of same id returns false");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib search::vector::tests::test_remove_drops_from_search`
Expected: FAIL — `no method named remove found`

- [ ] **Step 3: Add `remove` to `impl VectorIndex`**

`add`와 `search` 사이에 추가:

```rust
pub fn remove(&mut self, id: u64) -> bool {
    self.inner.remove(id)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib search::vector::tests::test_remove_drops_from_search`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/vector.rs
git commit -m "VectorIndex::remove 추가"
```

---

## Task 3: 파일 변경 감지 헬퍼 — `compute_file_changes`

**Files:**
- Create: `src/search/diff.rs`
- Modify: `src/search/mod.rs` (모듈 등록)
- Test: `src/search/diff.rs` (#[cfg(test)] mod tests 안)

- [ ] **Step 1: Write the failing test**

`src/search/diff.rs` 신규 작성:

```rust
use std::path::Path;

use git2::{DiffOptions, Oid};

use crate::git::repo::GitRepo;
use crate::search::SearchError;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileChanges {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

pub fn compute_file_changes(
    repo: &GitRepo,
    old_oid: &str,
    new_oid: &str,
) -> Result<FileChanges, SearchError> {
    let r = repo.repository();
    let old = Oid::from_str(old_oid)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let new = Oid::from_str(new_oid)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let old_tree = r
        .find_commit(old)
        .and_then(|c| c.tree())
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let new_tree = r
        .find_commit(new)
        .and_then(|c| c.tree())
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;

    let mut opts = DiffOptions::new();
    let diff = r
        .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;

    let mut out = FileChanges::default();
    for delta in diff.deltas() {
        match delta.status() {
            git2::Delta::Added | git2::Delta::Copied => {
                if let Some(p) = delta.new_file().path().and_then(Path::to_str) {
                    out.added.push(p.to_string());
                }
            }
            git2::Delta::Modified => {
                if let Some(p) = delta.new_file().path().and_then(Path::to_str) {
                    out.modified.push(p.to_string());
                }
            }
            git2::Delta::Deleted => {
                if let Some(p) = delta.old_file().path().and_then(Path::to_str) {
                    out.deleted.push(p.to_string());
                }
            }
            git2::Delta::Renamed => {
                if let Some(p) = delta.old_file().path().and_then(Path::to_str) {
                    out.deleted.push(p.to_string());
                }
                if let Some(p) = delta.new_file().path().and_then(Path::to_str) {
                    out.added.push(p.to_string());
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};

    #[test]
    fn test_added_modified_deleted_classified() {
        let (_dir, repo) = init_test_repo();
        let c1 = add_file_commit(&repo, "keep.txt", b"v1", "Add keep");
        let _c2 = add_file_commit(&repo, "drop.txt", b"x", "Add drop");
        let c3_oid = {
            // c3: modify keep.txt, delete drop.txt, add new.txt
            std::fs::write(_dir.path().join("keep.txt"), b"v2").unwrap();
            std::fs::remove_file(_dir.path().join("drop.txt")).unwrap();
            std::fs::write(_dir.path().join("new.txt"), b"hi").unwrap();
            let mut idx = repo.index().unwrap();
            idx.add_path(std::path::Path::new("keep.txt")).unwrap();
            idx.add_path(std::path::Path::new("new.txt")).unwrap();
            idx.remove_path(std::path::Path::new("drop.txt")).unwrap();
            idx.write().unwrap();
            let tree_oid = idx.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("t", "t@e").unwrap();
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "c3", &tree, &[&head])
                .unwrap()
                .to_string()
        };

        let gr = crate::git::repo::GitRepo::open(_dir.path()).unwrap();
        let changes = compute_file_changes(&gr, &c1.to_string(), &c3_oid).unwrap();
        assert!(changes.added.iter().any(|p| p == "new.txt"));
        assert!(changes.modified.iter().any(|p| p == "keep.txt"));
        assert!(changes.deleted.iter().any(|p| p == "drop.txt"));
    }
}
```

`src/search/mod.rs` 최상단 모듈 선언에 추가:

```rust
pub mod diff;
```

(기존 `pub mod bm25;` 등과 같은 위치)

확인: `init_test_repo`/`add_file_commit` 시그니처가 매치하는지 `src/git/repo.rs` 의 `pub mod tests`에서 확인. `add_file_commit`은 `Oid`를 반환해야 함 — 만약 아니라면 helper 자체를 먼저 손봐야 하므로 테스트 직전에 다음 명령으로 확인:

Run: `rg -n 'pub fn add_file_commit' src/git/repo.rs`

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib search::diff::tests::test_added_modified_deleted_classified`
Expected: FAIL — `unresolved import` 혹은 first compile

- [ ] **Step 3: 모듈 등록 후 재컴파일**

위 코드는 이미 step 1에서 작성됨. compile 통과 확인.

Run: `cargo build --lib`
Expected: 성공

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib search::diff::tests::test_added_modified_deleted_classified`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/diff.rs src/search/mod.rs
git commit -m "compute_file_changes 헬퍼 추가"
```

---

## Task 4: 신규 커밋 산출 헬퍼 — `commits_since`

**Files:**
- Modify: `src/search/diff.rs`
- Test: `src/search/diff.rs`

- [ ] **Step 1: Write the failing test**

`src/search/diff.rs` 의 `mod tests` 블록 안에 추가:

```rust
#[test]
fn test_commits_since_excludes_old_and_includes_new() {
    let (_dir, repo) = init_test_repo();
    let c1 = add_file_commit(&repo, "a.txt", b"1", "first");
    let c2 = add_file_commit(&repo, "b.txt", b"2", "second");
    let c3 = add_file_commit(&repo, "c.txt", b"3", "third");

    let gr = crate::git::repo::GitRepo::open(_dir.path()).unwrap();
    let commits = commits_since(&gr, &c1.to_string(), &c3.to_string()).unwrap();
    // old_oid(c1) 자체는 제외, c2/c3만 포함
    let oids: Vec<String> = commits.iter().map(|c| c.oid.clone()).collect();
    assert!(oids.contains(&c2.to_string()));
    assert!(oids.contains(&c3.to_string()));
    assert!(!oids.contains(&c1.to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib search::diff::tests::test_commits_since_excludes_old_and_includes_new`
Expected: FAIL — `no function commits_since`

- [ ] **Step 3: Implement `commits_since`**

`src/search/diff.rs` 의 `compute_file_changes` 함수 뒤에 추가:

```rust
use crate::git::commit::CommitInfo;

pub fn commits_since(
    repo: &GitRepo,
    old_oid: &str,
    new_oid: &str,
) -> Result<Vec<CommitInfo>, SearchError> {
    let r = repo.repository();
    let new = Oid::from_str(new_oid)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let old = Oid::from_str(old_oid)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let mut revwalk = r
        .revwalk()
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    revwalk
        .push(new)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    // hide old_oid — old_oid에 도달 가능한 커밋은 결과에서 제외
    revwalk
        .hide(old)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let mut out = Vec::new();
    for oid in revwalk.flatten() {
        if let Ok(c) = r.find_commit(oid) {
            out.push(CommitInfo::from_git_commit(&c));
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib search::diff::tests::test_commits_since_excludes_old_and_includes_new`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/diff.rs
git commit -m "commits_since 헬퍼 추가"
```

---

## Task 5: 기존 BM25 인덱스에서 path→doc_ids 맵 구성

**Files:**
- Modify: `src/search/indexer.rs`
- Test: `src/search/indexer.rs`

- [ ] **Step 1: Write the failing test**

`src/search/indexer.rs` 의 `mod tests` 블록 안에 추가 (`#[ignore]` 마커 없이 — 임베딩 불필요):

```rust
#[test]
fn test_collect_path_doc_ids_groups_by_path() {
    use crate::search::bm25::Bm25Index;
    use crate::search::{DocKind, DocMeta};

    let dir = tempfile::tempdir().unwrap();
    let idx = Bm25Index::create(dir.path()).unwrap();
    let mut w = idx.writer().unwrap();
    let mk = |id: u64, path: &str| DocMeta {
        doc_id: id,
        kind: DocKind::File,
        title: path.to_string(),
        commit_oid: "0".repeat(40),
        path: Some(path.to_string()),
        line_start: None,
        line_end: None,
    };
    idx.add_doc(&mut w, &mk(1, "a.rs"), "").unwrap();
    idx.add_doc(&mut w, &mk(2, "a.rs"), "").unwrap();
    idx.add_doc(&mut w, &mk(3, "b.rs"), "").unwrap();
    idx.commit(w).unwrap();

    let store = idx.scan_doc_store().unwrap();
    let map = collect_path_doc_ids(&store);
    assert_eq!(map.get("a.rs").map(|v| v.len()), Some(2));
    assert_eq!(map.get("b.rs").map(|v| v.len()), Some(1));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib search::indexer::tests::test_collect_path_doc_ids_groups_by_path`
Expected: FAIL — `no function collect_path_doc_ids`

- [ ] **Step 3: Implement helper**

`src/search/indexer.rs` 파일 끝쪽 (테스트 모듈 직전)에 추가:

```rust
pub(crate) fn collect_path_doc_ids(
    store: &std::collections::HashMap<u64, DocMeta>,
) -> std::collections::HashMap<String, Vec<u64>> {
    let mut out: std::collections::HashMap<String, Vec<u64>> = std::collections::HashMap::new();
    for meta in store.values() {
        if let Some(p) = &meta.path {
            out.entry(p.clone()).or_default().push(meta.doc_id);
        }
    }
    out
}

pub(crate) fn max_doc_id(store: &std::collections::HashMap<u64, DocMeta>) -> u64 {
    store.keys().copied().max().unwrap_or(0)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib search::indexer::tests::test_collect_path_doc_ids_groups_by_path`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/indexer.rs
git commit -m "collect_path_doc_ids/max_doc_id 헬퍼 추가"
```

---

## Task 6: `build_index_incremental` 오케스트레이션

**Files:**
- Modify: `src/search/indexer.rs`

이 task는 큰 함수를 추가하므로 단계가 더 많다.

- [ ] **Step 1: Read 기존 `build_index` 다시 확인**

Run: `rg -n 'fn build_index' src/search/indexer.rs`
Expected: 한 곳에서 정의됨. 이 함수는 그대로 두고 옆에 `build_index_incremental`을 추가한다.

- [ ] **Step 2: incremental 함수 추가**

`src/search/indexer.rs` 의 `build_index` 함수 뒤에 추가:

```rust
fn build_index_incremental<F>(
    repo: &GitRepo,
    index_dir: &Path,
    old_meta: &IndexMeta,
    current_oid: &str,
    opts: &IndexOptions,
    progress: F,
) -> Result<(), SearchError>
where
    F: Fn(&str),
{
    use crate::search::diff::{commits_since, compute_file_changes};

    progress("Opening existing index...");
    let bm25_dir = index_dir.join("bm25");
    let bm25 = Bm25Index::open(bm25_dir.clone())?;
    let vector_path = index_dir.join("vectors").join("index.tvim");
    let mut vector = VectorIndex::load(&vector_path)?;

    let store = bm25.scan_doc_store()?;
    let path_map = collect_path_doc_ids(&store);
    let mut doc_counter = max_doc_id(&store) + 1;

    progress("Computing file changes...");
    let changes = compute_file_changes(repo, &old_meta.head_oid, current_oid)?;
    let new_commits = commits_since(repo, &old_meta.head_oid, current_oid)?;

    let touched: Vec<String> = changes
        .modified
        .iter()
        .chain(changes.deleted.iter())
        .cloned()
        .collect();

    let mut writer = bm25.writer().map_err(SearchError::Tantivy)?;

    // Stale 문서 제거: modified + deleted 파일의 모든 doc_id
    for path in &touched {
        if let Some(ids) = path_map.get(path) {
            for id in ids {
                bm25.delete_doc(&mut writer, *id);
                vector.remove(*id);
            }
        }
    }

    // 신규 chunk 수집: 신규 커밋 메시지 + (added + modified) 파일
    let mut new_chunks: Vec<Chunk> = new_commits.iter().map(commit_to_chunk).collect();

    let new_oid = git2::Oid::from_str(current_oid)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let new_commit = {
        let rep = repo.repository();
        let c = rep
            .find_commit(new_oid)
            .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
        CommitInfo::from_git_commit(&c)
    };

    for path in changes.added.iter().chain(changes.modified.iter()) {
        if crate::git::tree::is_binary_blob(repo, &new_commit, path).unwrap_or(true) {
            continue;
        }
        let Ok(content) = crate::git::tree::read_blob(repo, &new_commit, path) else {
            continue;
        };
        if content.len() > opts.max_file_bytes {
            continue;
        }
        new_chunks.extend(crate::search::chunk::split_file(current_oid, path, &content));
    }

    progress(&format!(
        "Embedding {} new chunks (delta from {} files)...",
        new_chunks.len(),
        changes.added.len() + changes.modified.len() + changes.deleted.len()
    ));

    let model = EmbeddingModel::load()?;
    let dim = model.dim();
    if dim != old_meta.embedding.dim {
        return Err(SearchError::Embedding(format!(
            "embedding dim changed: {} → {}",
            old_meta.embedding.dim, dim
        )));
    }

    let mut all_ids: Vec<u64> = Vec::new();
    let mut all_vecs: Vec<Vec<f32>> = Vec::new();

    for chunk_batch in new_chunks.chunks(opts.batch_size) {
        let texts: Vec<String> = chunk_batch.iter().map(|c| c.embed_text()).collect();
        let embeddings = model.encode_batch(&texts)?;
        for (chunk, embedding) in chunk_batch.iter().zip(embeddings.iter()) {
            let doc_id = doc_counter;
            doc_counter += 1;
            let meta = chunk_to_meta(doc_id, chunk);
            bm25.add_doc(&mut writer, &meta, chunk.bm25_body())
                .map_err(SearchError::Tantivy)?;
            all_ids.push(doc_id);
            all_vecs.push(embedding.clone());
        }
    }

    bm25.commit(writer).map_err(SearchError::Tantivy)?;
    vector.add(&all_ids, &all_vecs)?;
    vector.save(vector_path)?;

    let meta = IndexMeta {
        version: INDEX_VERSION,
        head_oid: current_oid.to_string(),
        doc_count: doc_counter,
        indexed_at: chrono_now(),
        embedding: old_meta.embedding.clone(),
        bm25: old_meta.bm25.clone(),
        vector: old_meta.vector.clone(),
    };
    let meta_str = toml::to_string_pretty(&meta)?;
    std::fs::write(index_dir.join("meta.toml"), meta_str)?;
    progress(&format!(
        "Incremental update: +{} added, ~{} modified, -{} deleted, {} new commits",
        changes.added.len(),
        changes.modified.len(),
        changes.deleted.len(),
        new_commits.len()
    ));
    Ok(())
}
```

`EmbeddingMeta`/`Bm25Meta`/`VectorMeta`에 `Clone` derive가 없을 경우 `src/search/mod.rs`에서 추가. 확인:

Run: `rg -n '#\[derive\(.*\)\]' src/search/mod.rs | rg -i 'meta'`

`Clone`이 없으면 step 2 적용 후 빌드 시 컴파일 에러로 알 수 있다. 그 경우 다음 sub-step 수행:

```rust
// src/search/mod.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingMeta { ... }
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bm25Meta { ... }
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorMeta { ... }
```

`IndexMeta`도 같이 `Clone`을 붙인다 (`old_meta.head_oid` 비교용으로는 불필요하지만 호출 측에서 빌리기 편함).

- [ ] **Step 3: Build 확인**

Run: `cargo build --lib`
Expected: 성공

- [ ] **Step 4: Commit (테스트는 다음 task에서)**

```bash
git add src/search/indexer.rs src/search/mod.rs
git commit -m "build_index_incremental 추가"
```

---

## Task 7: `build_index` 진입점에 incremental 분기 추가

**Files:**
- Modify: `src/search/indexer.rs`

- [ ] **Step 1: 기존 build_index 본문 확인**

기존 `build_index` 본문(`src/search/indexer.rs:56` 부근)에서, `head_oid 일치 → 조기 반환` 직후의 `if stale { std::fs::remove_dir_all(&index_dir)?; }` 블록을 incremental 시도로 대체한다.

- [ ] **Step 2: 변경 적용**

기존 코드 (대략 indexer.rs:67–95):

```rust
    if index_dir.exists() {
        if opts.force {
            std::fs::remove_dir_all(&index_dir)?;
        } else {
            let meta_path = index_dir.join("meta.toml");
            if meta_path.exists() {
                let stale = match std::fs::read_to_string(&meta_path)
                    .ok()
                    .and_then(|s| toml::from_str::<IndexMeta>(&s).ok())
                {
                    Some(meta) if meta.version == INDEX_VERSION => {
                        let current_oid = head_oid(repo)?;
                        if meta.head_oid == current_oid {
                            progress("Index is up to date.");
                            return Ok(());
                        }
                        true
                    }
                    _ => {
                        progress("Rebuilding index (schema upgrade)...");
                        true
                    }
                };
                if stale {
                    std::fs::remove_dir_all(&index_dir)?;
                }
            }
        }
    }
```

다음으로 교체:

```rust
    if index_dir.exists() {
        if opts.force {
            std::fs::remove_dir_all(&index_dir)?;
        } else {
            let meta_path = index_dir.join("meta.toml");
            if meta_path.exists() {
                match std::fs::read_to_string(&meta_path)
                    .ok()
                    .and_then(|s| toml::from_str::<IndexMeta>(&s).ok())
                {
                    Some(meta) if meta.version == INDEX_VERSION => {
                        let current_oid = head_oid(repo)?;
                        if meta.head_oid == current_oid {
                            progress("Index is up to date.");
                            return Ok(());
                        }
                        // old_head_oid가 repo에 살아있고 모델 dim이 같으면 incremental 시도
                        let old_alive = git2::Oid::from_str(&meta.head_oid)
                            .ok()
                            .and_then(|o| repo.repository().find_commit(o).ok())
                            .is_some();
                        if old_alive {
                            progress("Attempting incremental update...");
                            match build_index_incremental(
                                repo,
                                &index_dir,
                                &meta,
                                &current_oid,
                                opts,
                                &progress,
                            ) {
                                Ok(()) => return Ok(()),
                                Err(e) => {
                                    progress(&format!(
                                        "Incremental update failed ({e}); falling back to full rebuild"
                                    ));
                                }
                            }
                        } else {
                            progress("Old head not in repo; full rebuild required");
                        }
                        std::fs::remove_dir_all(&index_dir)?;
                    }
                    _ => {
                        progress("Rebuilding index (schema upgrade)...");
                        std::fs::remove_dir_all(&index_dir)?;
                    }
                }
            }
        }
    }
```

`progress`가 `Fn(&str)` 트레잇 객체로 두 군데 (incremental + full) 모두에 전달되어야 하므로 `&progress` 참조 전달. `build_index_incremental` 시그니처의 `F: Fn(&str)`은 `&F`도 받지 못함 — generic 풀어주거나, 두 호출 모두 동일 generic을 받게 한다. 빌드 에러 시:

대안: `build_index_incremental`을 `progress: &dyn Fn(&str)`로 변경하고, `build_index`도 클로저를 `&progress`로 넘김. 또는 `build_index_incremental<F: Fn(&str)>(... progress: F)`인데 closure 소유권 이동 문제 → 가장 간단한 해법은 둘 다 `&dyn Fn(&str)`로 받는 helper로 통일:

```rust
fn build_index_incremental(
    repo: &GitRepo,
    index_dir: &Path,
    old_meta: &IndexMeta,
    current_oid: &str,
    opts: &IndexOptions,
    progress: &dyn Fn(&str),
) -> Result<(), SearchError> { /* 본문 동일, F 제거 */ }
```

그리고 `build_index` 안에서 `&progress` 전달, `build_index_incremental` 호출부의 `&progress`도 자연스레 fit. 단 `build_index` 자체의 시그니처 `F: Fn(&str)`은 외부 호출자가 클로저를 넘기는 부분이므로 변경하지 말고, 내부에서 `let progress = |s: &str| progress(s);` 같은 wrapper로 `&dyn Fn(&str)` 만들어 전달.

가장 깔끔: incremental 함수를 `&dyn Fn`으로 받고, `build_index` 본문에서 `let pcb: &dyn Fn(&str) = &progress;` 정의 후 두 호출에서 모두 `pcb` 사용.

- [ ] **Step 3: Build & clippy**

Run: `cargo build --lib && cargo clippy --all-targets`
Expected: 무경고

- [ ] **Step 4: 기존 테스트 회귀 없음 확인**

Run: `cargo test --lib search::`
Expected: 모두 PASS (1 ignored 유지)

- [ ] **Step 5: Commit**

```bash
git add src/search/indexer.rs
git commit -m "build_index에 incremental 분기 추가"
```

---

## Task 8: incremental e2e 통합 테스트

**Files:**
- Modify: `src/search/indexer.rs`

이 테스트는 임베딩 모델 로드(네트워크 첫 실행 시 hf-hub)가 필요하므로 `#[ignore]` 마커를 단다.

- [ ] **Step 1: Write the test**

`src/search/indexer.rs` 의 `mod tests` 블록 안에 추가:

```rust
#[test]
#[ignore]
fn test_incremental_update_preserves_old_and_adds_new() {
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::search::SearchEngine;

    let (dir, repo) = init_test_repo();
    add_file_commit(&repo, "alpha.rs", b"fn alpha() {}", "Add alpha");

    let git_repo = GitRepo::open(dir.path()).unwrap();
    let opts = IndexOptions::default();
    build_index(&git_repo, dir.path(), &opts, |_| {}).unwrap();

    let index_dir = index_dir_for(dir.path());
    let engine1 = SearchEngine::open(&index_dir).unwrap();
    let initial_count = engine1.doc_store.len();
    drop(engine1);

    // 새 커밋 + 파일 추가
    add_file_commit(&repo, "beta.rs", b"fn beta() {}", "Add beta");

    // 재인덱싱 (incremental 경로 진입)
    build_index(&git_repo, dir.path(), &opts, |_| {}).unwrap();

    let engine2 = SearchEngine::open(&index_dir).unwrap();
    assert!(
        engine2.doc_store.len() > initial_count,
        "incremental should add at least one new doc"
    );
    // 새 파일 chunk가 들어왔는지 확인
    let has_beta = engine2
        .doc_store
        .values()
        .any(|m| m.path.as_deref() == Some("beta.rs"));
    assert!(has_beta, "beta.rs should be indexed after incremental update");
}
```

- [ ] **Step 2: Run test**

Run: `cargo test --lib search::indexer::tests::test_incremental_update_preserves_old_and_adds_new -- --ignored`
Expected: PASS (hf-hub 모델 다운로드 시 첫 실행은 수십 초 소요)

이미 모델이 캐시되어 있으면 5~10초 내 종료.

- [ ] **Step 3: Commit**

```bash
git add src/search/indexer.rs
git commit -m "incremental 인덱싱 e2e 테스트 추가"
```

---

## Task 9: Manual smoke test on this repo

자동화 테스트 외에 실제 동작 확인.

- [ ] **Step 1: 현재 상태 확인**

Run: `ls -la .glc-index/meta.toml 2>/dev/null && head -3 .glc-index/meta.toml`
Expected: meta.toml 존재. head_oid 값 기록.

존재하지 않으면 먼저 다음 명령으로 전체 인덱싱:

Run: `cargo run --bin glc -- index`

- [ ] **Step 2: HEAD 변경 후 다시 index**

테스트용 commit 하나 추가 후 incremental 동작 관찰:

Run:
```bash
echo "// smoke test" >> src/main.rs
git add src/main.rs && git commit -m "smoke test"
cargo run --bin glc -- index 2>&1 | tail -10
```

Expected output: `Attempting incremental update...` 그리고 `Incremental update: +0 added, ~1 modified, -0 deleted, 1 new commits`.

- [ ] **Step 3: smoke test 커밋 되돌리기**

```bash
git reset --hard HEAD~1
cargo run --bin glc -- index 2>&1 | tail -10
```

Expected: `Old head not in repo; full rebuild required` 또는 incremental 시도 후 fallback. **`git reset --hard`는 destructive — 본인이 만든 smoke test 커밋만 되돌리는지 확인**. 의심되면 `git log --oneline -5`로 먼저 확인.

- [ ] **Step 4: 최종 정상 확인**

Run: `cargo run --bin glc -- index`
Expected: `Index is up to date.`

---

## Self-Review Checklist

- [x] 모든 task가 exact 파일 경로 명시
- [x] 각 step에 실제 코드 또는 명령 포함 (TBD 없음)
- [x] TDD 사이클(test → fail → impl → pass → commit) 준수
- [x] `delete_doc`/`remove` 메서드명 task 1, 2와 task 6에서 일치
- [x] `FileChanges`/`commits_since` 시그니처가 task 6 호출부와 일치
- [x] fallback path 명시 (old_head 부재 / embedding dim 변경 / incremental 에러 시 full rebuild)
- [x] e2e 테스트가 `#[ignore]` (네트워크 의존 — 기존 컨벤션 따름)

---

## Out of Scope (별도 plan)

1. **검색 품질 자동 평가** — `tests/fixtures/search_queries.toml` + MRR/NDCG 계산. 별도 plan으로 분리.
2. **Diff hunk 인덱싱** — `git log -p` hunk 단위 chunk. 별도 plan.
3. **code-aware reranking** — symbol/import 가중치, 별도 plan.
4. **Recency boost** — commit 날짜 기반 score decay. RRF 후 후처리이므로 단독 작은 plan.

---

## Execution Handoff

이 plan은 9개 task, 약 60~90분 추정. 

**1. Subagent-Driven (recommended)** — task당 fresh subagent + 리뷰
**2. Inline Execution** — 현 세션에서 batch 실행 + checkpoint

어느 방식을 쓰실지 알려주세요.
