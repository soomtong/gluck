use std::path::{Path, PathBuf};

use crate::git::commit::CommitInfo;
use crate::git::repo::GitRepo;
use crate::git::tree::{is_binary_blob, read_blob};
use crate::search::bm25::{Bm25Index, TOKENIZER as BM25_TOKENIZER};
use crate::search::chunk::{commit_to_chunk, split_file, Chunk};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::search::SearchEngine;

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
}
