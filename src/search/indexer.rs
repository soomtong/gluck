use std::path::Path;
use anyhow::{Context, Result};
use crate::git::commit::list_commits;
use crate::git::repo::GitRepo;
use crate::git::tree::{is_binary_blob, list_tree, read_blob, EntryKind};
use super::bm25::Bm25Index;
use super::chunk::{commit_chunk, split_file, Chunk};
use super::embedding::EmbeddingModel;
use super::vector::VectorIndex;
use super::Meta;

const BATCH_SIZE: usize = 32;
const MAX_FILE_CHARS: usize = 4096; // per chunk

pub struct IndexConfig {
    pub batch_size: usize,
    pub max_file_bytes: usize,
    pub force: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self { batch_size: BATCH_SIZE, max_file_bytes: 1_048_576, force: false }
    }
}

pub fn build_index(repo_path: &Path, output_path: &Path, cfg: IndexConfig) -> Result<()> {
    build_index_with_model(repo_path, output_path, cfg, None)
}

/// `embedding_model` = None → uses EmbeddingModel::new() (fastembed)
/// `embedding_model` = Some(stub) → for tests (no download)
pub fn build_index_with_model(
    repo_path: &Path,
    output_path: &Path,
    cfg: IndexConfig,
    embedding_model: Option<EmbeddingModel>,
) -> Result<()> {
    let repo = GitRepo::open(repo_path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let head_oid = {
        let r = repo.repository();
        r.head().context("get HEAD")?
            .target().context("HEAD has no target")?
            .to_string()
    };

    // Skip if up-to-date (unless --force)
    let meta_path = output_path.join("meta.toml");
    if !cfg.force && meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if content.contains(&head_oid) {
                eprintln!("Index up-to-date (HEAD {}). Use --force to rebuild.", &head_oid[..7.min(head_oid.len())]);
                return Ok(());
            }
        }
    }

    // Old index with incompatible version: require --force
    if !cfg.force && meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            let meta: Result<Meta, _> = toml::from_str(&content);
            if let Ok(m) = meta {
                if m.version != Meta::CURRENT_VERSION {
                    anyhow::bail!(
                        "Index format version {} is incompatible. Run `glc index --force` to rebuild.",
                        m.version
                    );
                }
            }
        }
    }

    eprintln!("Building search index for {} ...", repo_path.display());

    let commits = list_commits(&repo).map_err(|e| anyhow::anyhow!("{e}"))?;
    if commits.is_empty() {
        anyhow::bail!("No commits found");
    }
    let head_commit = &commits[0];

    // Collect all chunks
    let mut all_chunks: Vec<Chunk> = Vec::new();
    let mut next_doc_id: u64 = 0;

    // 1. Commit messages
    eprintln!("  Indexing {} commits...", commits.len());
    for commit in &commits {
        all_chunks.push(commit_chunk(
            &commit.id.to_string(),
            &commit.short_id,
            &commit.message,
            next_doc_id,
        ));
        next_doc_id += 1;
    }

    // 2. HEAD file tree
    let tree = list_tree(&repo, head_commit)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let file_entries: Vec<_> = tree.iter()
        .filter(|e| matches!(e.kind, EntryKind::File))
        .collect();
    eprintln!("  Indexing {} files...", file_entries.len());

    for entry in &file_entries {
        if is_binary_blob(&repo, head_commit, &entry.path).unwrap_or(true) {
            continue;
        }
        let content = match read_blob(&repo, head_commit, &entry.path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.len() > cfg.max_file_bytes {
            continue;
        }
        let file_chunks = split_file(
            &entry.path,
            &content,
            &head_commit.id.to_string(),
            next_doc_id,
            MAX_FILE_CHARS,
        );
        let chunk_count = file_chunks.len() as u64;
        all_chunks.extend(file_chunks);
        next_doc_id += chunk_count;
    }

    eprintln!("  Total chunks: {}", all_chunks.len());

    // Clean up old index
    if output_path.exists() {
        std::fs::remove_dir_all(output_path)
            .context("remove old index")?;
    }
    std::fs::create_dir_all(output_path)
        .context("create index dir")?;

    // 3. Build BM25 index
    let bm25_path = output_path.join("bm25");
    Bm25Index::build(&bm25_path, &all_chunks)
        .map_err(|e| anyhow::anyhow!("BM25 build failed: {e}"))?;

    // 4. Generate embeddings + build turbovec index
    let model = match embedding_model {
        Some(m) => m,
        None => EmbeddingModel::new().map_err(|e| anyhow::anyhow!("Embedding model init: {e}"))?,
    };
    let dim = model.dim();
    let mut vector_index = VectorIndex::new(dim);

    eprintln!("  Generating embeddings (dim={dim}, batch={})...", cfg.batch_size);
    for batch in all_chunks.chunks(cfg.batch_size) {
        let texts: Vec<&str> = batch.iter()
            .map(|c| c.body.as_str())
            .collect();
        let embeddings = model.embed_batch(&texts)
            .map_err(|e| anyhow::anyhow!("Embedding failed: {e}"))?;
        let flat: Vec<f32> = embeddings.into_iter().flatten().collect();
        let ids: Vec<u64> = batch.iter().map(|c| c.doc_id).collect();
        vector_index.add(&flat, &ids)
            .map_err(|e| anyhow::anyhow!("VectorIndex add failed: {e}"))?;
    }

    let tvim_path = output_path.join("vectors/index.tvim");
    vector_index.write(&tvim_path)
        .map_err(|e| anyhow::anyhow!("VectorIndex write failed: {e}"))?;

    // 5. Write meta.toml
    let meta = Meta {
        version: Meta::CURRENT_VERSION,
        head_oid,
        doc_count: next_doc_id,
        indexed_at: unix_timestamp_str(),
        model_name: super::embedding::MODEL_NAME.to_string(),
        vector_dim: dim,
        vector_backend: "turboquant_4bit".to_string(),
    };
    let meta_content = toml::to_string_pretty(&meta)
        .context("serialize meta.toml")?;
    std::fs::write(&meta_path, meta_content)
        .context("write meta.toml")?;

    eprintln!("  Done. {} chunks indexed.", next_doc_id);
    Ok(())
}

fn unix_timestamp_str() -> String {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use super::super::embedding::EmbeddingModel;
    use tempfile::TempDir;

    fn stub_model() -> EmbeddingModel {
        // Use a dim that is a multiple of 8 for turbovec
        EmbeddingModel::new_stub(8)
    }

    fn build_test_index(repo_dir: &Path) -> (TempDir, std::path::PathBuf) {
        let index_dir = TempDir::new().unwrap();
        let output = index_dir.path().join(".glc-index");
        build_index_with_model(repo_dir, &output, IndexConfig::default(), Some(stub_model())).unwrap();
        (index_dir, output)
    }

    #[test]
    fn test_build_creates_expected_files() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() { println!(\"hello\"); }", "Initial commit");

        let (_index_dir, output) = build_test_index(repo_dir.path());

        assert!(output.join("meta.toml").exists(), "meta.toml missing");
        assert!(output.join("bm25").exists(), "bm25/ missing");
        assert!(output.join("vectors/index.tvim").exists(), "vectors/index.tvim missing");
    }

    #[test]
    fn test_meta_version_is_2() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn f() {}", "Add f");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let content = std::fs::read_to_string(output.join("meta.toml")).unwrap();
        let meta: Meta = toml::from_str(&content).unwrap();
        assert_eq!(meta.version, 2);
        assert_eq!(meta.vector_backend, "turboquant_4bit");
    }

    #[test]
    fn test_build_skips_if_up_to_date() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn f() {}", "Add f");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let mtime1 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();

        // Second build should skip
        build_index_with_model(repo_dir.path(), &output, IndexConfig::default(), Some(stub_model())).unwrap();
        let mtime2 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();
        assert_eq!(mtime1, mtime2, "second build should not modify meta.toml");
    }

    #[test]
    fn test_build_force_rebuilds() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn f() {}", "Add f");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let mtime1 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        let cfg = IndexConfig { force: true, ..Default::default() };
        build_index_with_model(repo_dir.path(), &output, cfg, Some(stub_model())).unwrap();
        let mtime2 = std::fs::metadata(output.join("meta.toml")).unwrap().modified().unwrap();
        assert_ne!(mtime1, mtime2, "--force should rebuild");
    }

    #[test]
    fn test_build_skips_binary_files() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() {}", "Add code");
        add_file_commit(&repo, "img.png", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "Add image");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let meta: Meta = toml::from_str(
            &std::fs::read_to_string(output.join("meta.toml")).unwrap()
        ).unwrap();
        // commits (2) + 1 text file (img.png skipped) = 3 chunks minimum
        assert!(meta.doc_count >= 3);
    }

    #[test]
    fn test_bm25_searchable_after_index() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "error.rs", b"fn handle_error() { panic!(\"oops\"); }", "Add error handling");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let bm25 = Bm25Index::open(&output.join("bm25")).unwrap();
        let results = bm25.search("error", 10).unwrap();
        assert!(!results.is_empty(), "should find 'error' after indexing");
    }

    #[test]
    fn test_doc_ids_unique_across_index() {
        let (repo_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.rs", b"fn a() {}", "Add a");
        add_file_commit(&repo, "b.rs", b"fn b() {}", "Add b");

        let (_index_dir, output) = build_test_index(repo_dir.path());
        let bm25 = Bm25Index::open(&output.join("bm25")).unwrap();
        let store = bm25.scan_doc_store().unwrap();

        let ids: Vec<u64> = store.keys().copied().collect();
        let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "all doc_ids must be unique");
    }
}
