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

    /// Cheap snapshot of HEAD for change polling.
    /// Returns None on unborn HEAD or repository errors.
    pub fn head_info(&self) -> Option<(git2::Oid, String)> {
        let head = self.repo.head().ok()?;
        let oid = head.target()?;
        let name = head.shorthand().unwrap_or("HEAD").to_string();
        Some((oid, name))
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

    #[test]
    fn test_create_n_commits() {
        let (_dir, repo) = init_test_repo_with_n_commits(50);
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_head().unwrap();
        assert_eq!(revwalk.count(), 50);
    }

    #[test]
    fn test_head_info_unborn_head_returns_none() {
        let (dir, _repo) = init_test_repo();
        let git_repo = GitRepo::open(dir.path()).unwrap();
        assert!(git_repo.head_info().is_none());
    }

    #[test]
    fn test_head_info_tracks_new_commits() {
        let (dir, repo) = init_test_repo();
        let first = add_file_commit(&repo, "a.txt", b"a", "first");
        let git_repo = GitRepo::open(dir.path()).unwrap();

        let (oid1, name1) = git_repo.head_info().unwrap();
        assert_eq!(oid1, first);
        assert!(!name1.is_empty());

        let second = add_file_commit(&repo, "b.txt", b"b", "second");
        let (oid2, _) = git_repo.head_info().unwrap();
        assert_eq!(oid2, second);
    }

    #[test]
    fn test_head_info_detects_branch_switch_same_oid() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"a", "first");
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head_commit, false).unwrap();

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let (oid1, name1) = git_repo.head_info().unwrap();

        repo.set_head("refs/heads/feature").unwrap();
        let (oid2, name2) = git_repo.head_info().unwrap();

        assert_eq!(oid1, oid2);
        assert_ne!(name1, name2);
    }
}
