use git2::Repository;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("Not a git repository: {0}")]
    RepositoryNotFound(String),
    #[error("Commit not found: {0}")]
    CommitNotFound(String),
    #[error("Tree walk failed: {0}")]
    TreeWalkFailed(String),
    #[error("Blob read failed: {0}")]
    BlobReadFailed(String),
    #[error("Diff computation failed: {0}")]
    DiffFailed(String),
    #[error("Git internal error: {0}")]
    Internal(#[from] git2::Error),
}

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn open(path: &Path) -> Result<Self, GitError> {
        let repo = Repository::discover(path)
            .map_err(|e| GitError::RepositoryNotFound(format!("{}: {}", path.display(), e)))?;
        Ok(Self { repo })
    }

    pub fn repository(&self) -> &Repository {
        &self.repo
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use git2::Signature;
    use tempfile::TempDir;

    pub fn init_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
        (dir, repo)
    }

    pub fn add_file_commit(
        repo: &Repository,
        path: &str,
        content: &[u8],
        message: &str,
    ) -> git2::Oid {
        let dir = repo.workdir().unwrap();
        let file_path = dir.join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();

        let parents: Vec<git2::Commit> = if repo.head().is_ok() {
            vec![repo.head().unwrap().peel_to_commit().unwrap()]
        } else {
            vec![]
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .unwrap()
    }

    #[test]
    fn test_open_valid_repo() {
        let (dir, _repo) = init_test_repo();
        assert!(GitRepo::open(dir.path()).is_ok());
    }

    #[test]
    fn test_open_invalid_path() {
        let dir = TempDir::new().unwrap();
        assert!(GitRepo::open(dir.path()).is_err());
    }
}
