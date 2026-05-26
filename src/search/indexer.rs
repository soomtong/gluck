use std::path::{Path, PathBuf};

use crate::git::commit::CommitInfo;
use crate::git::repo::GitRepo;
use crate::git::tree::{is_binary_blob, read_blob};
use crate::search::bm25::{Bm25Index, TOKENIZER as BM25_TOKENIZER};
use crate::search::chunk::{commit_to_chunk, split_file, Chunk};
use crate::search::diff::{commits_since, compute_file_changes};
use crate::search::embedding::EmbeddingModel;
use crate::search::vector::VectorIndex;
use crate::search::{
    Bm25Meta, DocKind, DocMeta, EmbeddingMeta, IndexMeta, SearchError, VectorMeta, INDEX_DIR_NAME,
    INDEX_VERSION,
};

pub struct IndexOptions {
    pub force: bool,
    pub batch_size: usize,
    pub max_file_bytes: usize,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            force: false,
            batch_size: 64,
            max_file_bytes: 1_000_000,
        }
    }
}

pub fn index_dir_for(repo_path: &Path) -> PathBuf {
    repo_path.join(INDEX_DIR_NAME)
}

pub fn index_status(index_dir: &Path) -> IndexStatus {
    let meta_path = index_dir.join("meta.toml");
    if !meta_path.exists() {
        return IndexStatus::Missing;
    }
    match std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| toml::from_str::<IndexMeta>(&s).ok())
    {
        Some(meta) if meta.version == INDEX_VERSION => IndexStatus::Ready,
        _ => IndexStatus::SchemaOutdated,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStatus {
    Missing,
    SchemaOutdated,
    Ready,
}

pub fn build_index<F>(
    repo: &GitRepo,
    repo_path: &Path,
    opts: &IndexOptions,
    progress: F,
) -> Result<(), SearchError>
where
    F: Fn(&str),
{
    let index_dir = index_dir_for(repo_path);

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
                        let progress_dyn: &dyn Fn(&str) = &progress;
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
                                progress_dyn,
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

    std::fs::create_dir_all(&index_dir)?;

    progress("Loading embedding model...");
    let model = EmbeddingModel::load()?;

    let bm25_dir = index_dir.join("bm25");
    let bm25 = Bm25Index::create(&bm25_dir)?;
    let mut bm25_writer = bm25.writer().map_err(SearchError::Tantivy)?;

    let dim = model.dim();
    let mut vector = VectorIndex::new(dim);

    let mut doc_counter: u64 = 0;

    progress("Indexing commit messages...");
    let commits = collect_commits(repo)?;
    let mut chunks: Vec<Chunk> = commits.iter().map(commit_to_chunk).collect();

    progress("Indexing HEAD files...");
    let head_oid_str = head_oid(repo)?;
    let head_commit = {
        let rep = repo.repository();
        let oid = git2::Oid::from_str(&head_oid_str)
            .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
        let commit = rep
            .find_commit(oid)
            .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
        CommitInfo::from_git_commit(&commit)
    };

    let head_tree = crate::git::tree::list_tree(repo, &head_commit)
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;

    for entry in &head_tree {
        if !matches!(entry.kind, crate::git::tree::EntryKind::File) {
            continue;
        }
        if is_binary_blob(repo, &head_commit, &entry.path).unwrap_or(true) {
            continue;
        }
        let content = match read_blob(repo, &head_commit, &entry.path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if content.len() > opts.max_file_bytes {
            continue;
        }
        let file_chunks = split_file(&head_oid_str, &entry.path, &content);
        chunks.extend(file_chunks);
    }

    progress(&format!(
        "Embedding {} chunks in batches of {}...",
        chunks.len(),
        opts.batch_size
    ));
    let mut all_ids: Vec<u64> = Vec::new();
    let mut all_vecs: Vec<Vec<f32>> = Vec::new();

    for chunk_batch in chunks.chunks(opts.batch_size) {
        let texts: Vec<String> = chunk_batch.iter().map(|c| c.embed_text()).collect();
        let embeddings = model.encode_batch(&texts)?;

        for (chunk, embedding) in chunk_batch.iter().zip(embeddings.iter()) {
            let doc_id = doc_counter;
            doc_counter += 1;

            let meta = chunk_to_meta(doc_id, chunk);
            bm25.add_doc(&mut bm25_writer, &meta, chunk.bm25_body())
                .map_err(SearchError::Tantivy)?;

            all_ids.push(doc_id);
            all_vecs.push(embedding.clone());
        }
    }

    bm25.commit(bm25_writer).map_err(SearchError::Tantivy)?;

    vector.add(&all_ids, &all_vecs)?;
    let vector_dir = index_dir.join("vectors");
    vector.save(vector_dir.join("index.tvim"))?;

    let meta = IndexMeta {
        version: INDEX_VERSION,
        head_oid: head_oid_str,
        doc_count: doc_counter,
        indexed_at: chrono_now(),
        embedding: EmbeddingMeta {
            model: crate::search::embedding::MODEL_ID.to_string(),
            dim,
        },
        bm25: Bm25Meta {
            tokenizer: BM25_TOKENIZER.to_string(),
        },
        vector: VectorMeta {
            backend: "turboquant_4bit".to_string(),
        },
    };

    let meta_str = toml::to_string_pretty(&meta)?;
    std::fs::write(index_dir.join("meta.toml"), meta_str)?;
    progress(&format!("Indexed {} documents.", doc_counter));
    Ok(())
}

fn build_index_incremental(
    repo: &GitRepo,
    index_dir: &Path,
    old_meta: &IndexMeta,
    current_oid: &str,
    opts: &IndexOptions,
    progress: &dyn Fn(&str),
) -> Result<(), SearchError> {
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

    let new_oid =
        git2::Oid::from_str(current_oid).map_err(|e| SearchError::Git(e.to_string()))?;
    let new_commit = {
        let rep = repo.repository();
        let c = rep
            .find_commit(new_oid)
            .map_err(|e| SearchError::Git(e.to_string()))?;
        CommitInfo::from_git_commit(&c)
    };

    for path in changes.added.iter().chain(changes.modified.iter()) {
        if is_binary_blob(repo, &new_commit, path).unwrap_or(true) {
            continue;
        }
        let Ok(content) = read_blob(repo, &new_commit, path) else {
            continue;
        };
        if content.len() > opts.max_file_bytes {
            continue;
        }
        new_chunks.extend(split_file(current_oid, path, &content));
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
            "embedding dim changed: {} -> {}",
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

    // Partial failure window: if vector.save fails after bm25.commit succeeds,
    // next run sees old head_oid (meta.toml unwritten) and re-attempts incremental
    // from a baseline that no longer matches BM25 state. Task 7's fallback to
    // full rebuild on incremental error recovers from this; reordering commits
    // here doesn't help because vector and bm25 derive doc_counter from each other.
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
    let total_changes = changes.added.len() + changes.modified.len() + changes.deleted.len();
    if total_changes == 0 && new_commits.is_empty() {
        progress("Index head fast-forwarded (no content change).");
    } else {
        progress(&format!(
            "Incremental update: +{} added, ~{} modified, -{} deleted, {} new commits",
            changes.added.len(),
            changes.modified.len(),
            changes.deleted.len(),
            new_commits.len()
        ));
    }
    Ok(())
}

fn collect_commits(repo: &GitRepo) -> Result<Vec<CommitInfo>, SearchError> {
    let rep = repo.repository();
    let mut revwalk = rep
        .revwalk()
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    revwalk
        .push_head()
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let mut commits = Vec::new();
    for oid in revwalk.flatten() {
        if let Ok(commit) = rep.find_commit(oid) {
            commits.push(CommitInfo::from_git_commit(&commit));
        }
    }
    Ok(commits)
}

fn head_oid(repo: &GitRepo) -> Result<String, SearchError> {
    let rep = repo.repository();
    let head = rep
        .head()
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
    let oid = head
        .peel_to_commit()
        .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?
        .id();
    Ok(oid.to_string())
}

fn chunk_to_meta(doc_id: u64, chunk: &crate::search::chunk::Chunk) -> DocMeta {
    use crate::search::chunk::Chunk;
    match chunk {
        Chunk::CommitMessage { oid, title, .. } => DocMeta {
            doc_id,
            kind: DocKind::Commit,
            title: title.clone(),
            commit_oid: oid.clone(),
            path: None,
            line_start: None,
            line_end: None,
        },
        Chunk::WholeFile {
            commit_oid, path, ..
        } => DocMeta {
            doc_id,
            kind: DocKind::File,
            title: path.clone(),
            commit_oid: commit_oid.clone(),
            path: Some(path.clone()),
            line_start: None,
            line_end: None,
        },
        Chunk::Symbol {
            commit_oid,
            path,
            symbol_name,
            line_start,
            line_end,
            ..
        } => DocMeta {
            doc_id,
            kind: DocKind::Symbol,
            title: format!("{} ({})", symbol_name, path),
            commit_oid: commit_oid.clone(),
            path: Some(path.clone()),
            line_start: Some(*line_start),
            line_end: Some(*line_end),
        },
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

/// Group existing index doc_ids by their path.
///
/// Assumes the current indexing model: HEAD-snapshot only, so each path's
/// docs (one `WholeFile` + zero or more `Symbol`) all represent the same
/// commit_oid and should be invalidated together on path modification.
/// If history-aware (per-commit) file indexing is ever introduced, this
/// grouping needs to key on (path, commit_oid) instead.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::search::SearchEngine;

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

    #[test]
    #[ignore] // Requires network/hf-hub on first run; run with `cargo test -- --ignored`
    fn test_indexed_results_carry_commit_oid() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "alpha.rs", b"fn alpha() {}", "Add alpha");
        add_file_commit(&repo, "beta.rs", b"fn beta() {}", "Add beta function");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let opts = IndexOptions::default();
        build_index(&git_repo, dir.path(), &opts, |_| {}).unwrap();

        let index_dir = index_dir_for(dir.path());
        let engine = SearchEngine::open(&index_dir).unwrap();

        // Every doc must have a 40-char hex commit_oid (not the old empty string).
        for meta in engine.doc_store.values() {
            assert_eq!(
                meta.commit_oid.len(),
                40,
                "expected 40-char oid, got {:?} for {:?}",
                meta.commit_oid,
                meta.title
            );
            git2::Oid::from_str(&meta.commit_oid).expect("valid oid");
        }
        assert!(!engine.doc_store.is_empty());
    }

    #[test]
    #[ignore] // Requires network/hf-hub on first run; run with `cargo test -- --ignored`
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
}
