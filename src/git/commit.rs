use crate::git::repo::{GitError, GitRepo};
use git2::Oid;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct CommitInfo {
    pub id: Oid,
    pub short_id: String,
    pub author: String,
    pub date: SystemTime,
    pub message: String,
}

impl CommitInfo {
    pub fn from_git_commit(commit: &git2::Commit) -> Self {
        let id = commit.id();
        let short_id = id.to_string()[..7.min(id.to_string().len())].to_string();
        let author = commit.author().to_string();
        let date = UNIX_EPOCH + Duration::from_secs(commit.time().seconds() as u64);
        let message = commit.message().unwrap_or("").trim().to_string();
        Self {
            id,
            short_id,
            author,
            date,
            message,
        }
    }
}

pub fn list_commits(repo: &GitRepo) -> Result<Vec<CommitInfo>, GitError> {
    let repository = repo.repository();
    let mut revwalk = repository.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

    let mut commits = Vec::new();
    for oid in revwalk {
        let oid = oid?;
        let commit = repository.find_commit(oid)?;
        commits.push(CommitInfo::from_git_commit(&commit));
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};

    #[test]
    fn test_list_commits() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First commit");
        add_file_commit(&repo, "b.txt", b"second", "Second commit");
        add_file_commit(&repo, "c.txt", b"third", "Third commit");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].message, "Third commit");
        assert_eq!(commits[2].message, "First commit");
    }

    #[test]
    fn test_commit_info_fields() {
        let (dir, repo) = init_test_repo();
        let oid = add_file_commit(&repo, "a.txt", b"hello", "Test message");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let c = &commits[0];
        assert_eq!(c.id, oid);
        assert!(c.short_id.len() <= 7);
        assert!(c.author.contains("Test"));
        assert_eq!(c.message, "Test message");
    }
}
